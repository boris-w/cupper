# T025 存储 II：表结构、迁移与升级器

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主存储）
- 分支：`feat/T025-store-schema`
- 依赖：T023、T024（均已合并）。M1a 第 3 波；T026（关键路径）、T027 依赖它。

## 背景

宿主自己的事件日志（SQLite WAL）是唯一的历史来源（ADR-06）。T024 已经做好连接与持久性、写线程、读池和 `data_epoch` 判定（`crates/cupper-host/src/store/`）。本任务在它的基础上建立表结构、迁移框架和事件升级器框架。业务读写（追加事件、检查点、历史分页等）由 T026、T027 做。

## 范围

**做**（计划 T025 一行；设计 5.4.2、5.4.6；计划 P-4）：
1. **`0001` 迁移**：
   - `host.db` 的全部表和索引，照 5.4.2 写；
   - `raw.db` 的表也要建好。`raw.db` 是可丢弃的诊断数据，结构变了就直接重建，不做迁移（5.4.6）。怎么判断"结构变了"由你定，例如用 `PRAGMA user_version`。
   - 在 T160 之前，表结构只改 `0001`，不新增迁移脚本（P-4）。
2. **迁移框架**：
   - 脚本放在 `migrations/NNNN_<名称>.sql`，编进二进制；
   - 每个脚本在自己的事务里同时更新 `meta.schema_version`；
   - 两个脚本之间崩溃后，重启从下一个脚本继续，已经执行过的不重跑（测试 #11）；
   - 测试可以注入一组脚本。
3. **启动时的版本处理**（5.4.6），对外给出有类型的结果，由 T037 映射到退出码：
   - 版本比本程序支持的新：退出码 4（`HostExit::DataTooNew`）；
   - 版本较旧：先 `VACUUM INTO 'backups/host-v<旧版本>-<时间>.db'` 备份（已有同一版本的备份就不再备份），再逐个迁移；
   - 迁移失败：回滚当前事务，退出码 6（`HostExit::MigrationFailed`）。结果里要带上备份位置，界面要显示它。
4. **新库的 meta 初始值**：
   - `host_id`；
   - `created_at`、`layout_version`、`data_epoch`（初值 0）。
   - `last_start_at`、`boot_session_uuid` 这几项每次启动都要写，由 T037 负责，本任务只建好位置。
5. **事件升级器框架 `store::upcast`**（5.4.6、ADR-06）：
   - 按事件类型和 `v` 逐级升级到当前版本；
   - 目前所有事件都是 v1，框架用一个测试专用的升级器验证；
   - 每个升级器都配"旧样例 → 新样例"的单元测试。
6. **视图检查点的 `view_v`**：
   - 提供"版本不一致就丢弃检查点"的判定；
   - 丢弃之后怎么重建视图，归 T026。

**不做**：
- 业务读写，包括事件追加、检查点读写、回合快照、历史分页（T026）；
- blob、原始帧写入与保留清理（T027）；
- 启动顺序的接线，包括退出码映射和写 meta 的启动字段（T037）；
- `layout_version` 变化时的 Rust 迁移函数，现在只留接口位置。

## 验收标准

- [ ] 新增 IT-migrations（集成测试，带 `test-harness` 特性），第二个脚本由测试注入，覆盖：
  - 新库从零迁移到最新版本；
  - 旧版本先备份再迁移，同一版本的备份不重复做；
  - 两个脚本之间崩溃后从下一个脚本继续。可以用故障点（`failpoint!`，在 `failpoint.rs` 的清单里登记，并在子进程里触发），或者其他确定性的办法；
  - 版本过新时返回"过新"，迁移失败时返回"失败"和备份位置；
  - `raw.db` 结构变化时重建。
- [ ] 新增升级器框架的单元测试：用测试专用升级器跑通"旧样例 → 新样例"，逐级升级。
- [ ] `0001` 执行后，表和索引与 5.4.2 一致。可以用 `sqlite_schema` 比对，也可以逐表检查列。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T025-store-schema/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头（`it_migrations_` 等）。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **打开连接与迁移的顺序**：
  - 用 `store::open_writer(path, Database::Host)`（`raw.db` 用 `Database::Raw`）打开连接，迁移在这个连接上跑完，再把连接交给 `Writer::spawn`；
  - 迁移需要自己的事务，`VACUUM INTO` 也不能在事务里执行，所以迁移不能作为写线程的操作；
  - `Readers::open` 放在写连接打开之后。（T024）
- **宿主 ID** 用 `HostId::from_uuid(new_host_uuid().as_u128())`。（T023）
- **原始帧的方向 `Dir`**：目前有 `send`、`recv`、`recv_nonjson`。表的 `dir` 列按 5.4.2 写全五种取值，`stderr` 和 `note` 由写入它们的任务补到枚举里。（T050）
- **惯例**：
  - 时间只经 `Clock` 获取，`created_at` 和备份文件名里的时间都从 `Clock` 来；
  - `tests/` 下用到 `ManualClock` 等测试设施的集成测试，要声明 `required-features = ["test-harness"]`；
  - `lib.rs` 只放 `mod` 声明。
- **rusqlite**：锁定 `=0.40.2`，关掉了默认特性。需要 `prepare_cached` 时可以打开 `cache` 特性，它引入的 foldhash 是 Zlib 许可证，已在 allowlist 里。

## 规模

计划估为 M（400–900 行）。SQL 本身就有不少行，估计会偏大。**开工前先在 `notes.md` 里按模块写行数估计**；动手后每完成一个模块统计一次（守则第 8 条）。预计会明显超出约 1,500 行的上限时，先停下来汇报。

## 参考资料

- 设计：
  - `04-data-model.md` 5.4.1、5.4.2、5.4.4、5.4.6；
  - `02-host.md` 5.2.8（事件存储）、5.2.13（启动契约）；
  - `09-testing.md` 5.9.8（故障点、测试 #11）。
- ADR-06；计划 1.5 中的 P-4。
- T023、T024 的代码和汇报：`exit.rs`、`failpoint.rs`、`store/`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。其他物料（`notes.md` 等）由执行者在任务分支上提交。
