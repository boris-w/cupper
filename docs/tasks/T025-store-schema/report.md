# T025 汇报

- 执行者：子 agent（Rust 工程师，宿主存储）
- 分支：`feat/T025-store-schema`（worktree `.claude/worktrees/T025`）
- 完成日期：2026-10-19

## 做了什么

`0001` 迁移、迁移框架、raw.db 重建、事件升级器和 `view_v` 判定都做完了。验收标准全部满足，`cargo xtask ci pr` 通过。

代码在 `crates/cupper-host/` 下：

**`0001` 迁移**（`migrations/0001_initial.sql`）
- 建好 5.4.2 里 host.db 的 16 张表和 9 个索引，注释改成英文。
- raw.db 的表结构在 `src/store/raw.sql`。

**迁移框架**（`src/store/migrate.rs`）
- 入口：`migrate(&mut conn, scripts, backups_dir, &dyn Clock) -> Result<Migrated, MigrateError>`。
  - `MIGRATIONS` 用 `include_str!` 编进二进制。
  - 测试可以传入自己的脚本列表。
  - 脚本的版本号就是它在列表里的位置。
- 每个脚本在自己的 `BEGIN IMMEDIATE` 事务里执行，同一个事务里写 `meta.schema_version`。脚本失败时，事务在 drop 时回滚。
- 启动时按版本分三种情况：
  - 版本过新：返回 `TooNew{found, supported}`，对应退出码 4。
  - 版本较旧：先备份，再执行缺少的脚本。
    - 备份文件是 `backups/host-v<旧版本>-<YYYYMMDDTHHMMSSZ>.db`，时间取自 `Clock`。
    - 已有同一版本的备份时直接用它，不再备份。
  - 迁移失败：返回 `Failed{name, backup, source}`，对应退出码 6。`backup` 只有新库为 `None`。
- 新库的 meta 初值写在第一个脚本的事务里：`host_id`（`HostId::from_uuid(new_host_uuid().as_u128())`）、`created_at`、`layout_version`、`data_epoch = 0`。
- `LAYOUT_VERSION = 1`。它的文档注释就是给以后 Rust 迁移函数留的位置。
- 故障点 `store.migration_committed`：每个脚本提交后命中一次，已登记在 `failpoint.rs`。

**meta**（`src/store/meta.rs`）
- `MetaKey` 枚举列出 5.4.2 的 8 个键，文档写明每个键由谁写入。`last_start_at`、`last_clean_exit_at`、`boot_session_uuid` 只建了位置。
- 提供 `meta::get` 和 `meta::set`。

**raw.db**（`src/store/raw.rs`）
- 用法：`prepare_raw(&mut conn, &RAW_SCHEMA) -> RawDb::{Kept, Rebuilt{from}}`。
- 版本记在 `PRAGMA user_version`。版本不同（包括更新的版本）时，在一个事务里删掉所有表、重新建表、写版本号。

**升级器**（`src/store/upcast.rs`）
- `Upcaster{event_type, from, upgrade: fn(Value) -> Result<Value, &str>}`。
- `Upcasters::new` 检查每种类型的升级器链：从 v1 开始、连续、不重复，否则 panic。
- `current(type)` 给出写入版本；`upcast(type, v, data)` 逐级升级。`v` 为 0 或比当前版本新时报 `UnknownVersion`，旧数据缺字段时报 `Malformed`。
- `UPCASTERS` 目前为空，所有事件都是 v1。
- `view_v` 判定：`VIEW_VERSION = 1`，`checkpoint_fate(view_v)` 返回 `Use` 或 `Discard`（版本不相等就丢弃）。

**其他**
- `Cargo.toml`：新增依赖 `cupper-protocol`（为了 `HostId`，5.1.3 里本来就有这条依赖），并声明 `[[test]] migrations` 需要 `test-harness`。
- `store/testutil.rs`：新增 `design_sql`（读取设计文档里的 SQL 块）和 `assert_same_schema`。

## 关键取舍

1. **新库的 meta 初值放在第一个脚本的事务里，不在启动时另写。** 另写的话，脚本提交后、写初值前崩溃，会留下"有 `schema_version`、没有 `host_id`"的库。
2. **备份先写 `backups/host.partial`，落盘后再改名。**
   - 步骤：`sync_all`（macOS 上就是 `F_FULLFSYNC`）→ 改名 → fsync 目录。
   - 原因：已有同版本备份时会跳过备份，`VACUUM INTO` 中途崩溃留下的半截文件如果直接用正式文件名，下次启动会被当成备份。
   - 每次备份前先删掉残留的 `host.partial`。
3. **备份文件名里的时间用 UTC 的 `YYYYMMDDTHHMMSSZ`。** 人能看懂，按名字排序就是按时间排序，以后"保留最近 3 份"可以直接用。没有引入日期库，单元测试覆盖了闰日和世纪边界。
4. **表结构比对的是设计文档本身。**
   - 测试执行 `04-data-model.md` 5.4.2 的 SQL 块，用 pragma 逐条比对列、外键、索引、`WITHOUT ROWID` 和部分索引的条件。
   - 失败时只列出两边不同的行。
   - T160 之后会有多个脚本，比对的是"全部脚本执行完"的结果，仍然成立。
5. **raw.db 删表重建，不删文件。** 连接已经由 `open_writer` 打开，删一个正在打开的文件不安全。删表后的空闲页由之后的写入复用。
6. **退出码映射没有做**（brief 把它划给了 T037），只在错误类型的文档里写明对应的退出码。
7. **"每个升级器都有样例"做成了测试。** 测试里有一张样例表，`UPCASTERS` 里没有"旧样例 → 新样例"的升级器会让测试失败。框架本身用一个测试专用的三版本事件类型验证。
8. **`VIEW_VERSION` 放在宿主的 `store::upcast`。** 开工时本分支上还没有 T014 的视图。

## 验证方式

- **CI**：在干净的 worktree 上，提交 1d2da11 跑 `cargo xtask ci pr --record docs/tasks/T025-store-schema/ci/`，通过：rust 441 项、ts 64 项、license 1 项，`IT-migrations` 为 pass。摘要 `pr-1d2da11cc803.json` 已提交，内容里没有路径和用户名。
- **对照验收标准：**
  - [x] **IT-migrations**（`tests/migrations.rs`，需要 `test-harness`，测试脚本接在真实脚本之后）：
    - 新库迁移到最新版本，meta 初值逐项核对；再迁移一次什么都不做。
    - 先做的一次迁移失败：返回 `Failed`，事务回滚，备份文件名是 `host-v1-20260101T000100Z.db`，备份里保留了旧版本和旧数据，`backups/` 是 0700。下一次迁移沿用这份备份，`backups/` 里始终只有它一个文件。
    - 新库上迁移失败时 `backup` 为 `None`。
    - 版本过新时返回 `TooNew`，库不变，不做备份。
    - 两个脚本之间崩溃：子进程（测试二进制自己）用 `store.migration_committed` 退出，退出码 99。之后 `integrity_check` 通过，版本停在前一个脚本，重启后只执行下一个脚本，不重复执行。
    - raw.db 版本相同时保留数据；结构变了就重建，旧数据清空，`user_version` 更新。
  - [x] **升级器单元测试**：每个升级器都按样例从旧样例得到新样例；v1→v3 逐级升级；未知版本和缺字段的旧数据都报错；链有缺口或重复时 panic。
  - [x] **`0001` 与 5.4.2 一致**：host.db、raw.db 各有一条比对测试。
  - [x] CI 摘要已随任务提交。
  - [x] 测试名以编号开头（`it_migrations_`）；崩溃用的子进程测试名不带编号，不计入用例。
- **其他检查：**
  - 四种特性组合下 clippy（`-D warnings`）和测试都通过；store 模块的 rustdoc 没有警告。
  - 稳定性：IT 连跑 30 次、store 单元测试连跑 20 次，全部通过。
- **变异检查**，都被测出来了：
  - 表结构 8 项：默认值、`DESC`、部分索引的条件、`WITHOUT ROWID`、外键、唯一索引、`NOT NULL`、列上的 `UNIQUE`。
  - 迁移框架 5 项：不跳过已执行的脚本、忽略已有备份、脚本不放在事务里、"过新"的判断差一、所有情况都备份。
- **规模**：相对分支起点新增 1,348 行（不含 `Cargo.lock` 和 `docs/`）。
  - 其中 SQL 206 行，实现与文档约 510 行，测试 630 行。
  - 开工前估计约 1,210 行，没有超过约 1,500 行的上限。逐个提交的统计在 `notes.md` 里。

## 提交列表

```
7a375af docs(task): record the T025 pr CI summary
1d2da11 docs(task): record the T025 line counts and trade-offs
8bc4bb7 feat(store): add the event upcaster framework and the checkpoint view version
86d29f1 feat(store): rebuild raw.db when its schema version changes
a307c0e feat(store): migrate host.db with a backup first and resume after a crash
e8ed915 feat(store): add migration 0001 with the host.db tables of design 5.4.2
2368a92 docs(task): estimate the T025 size by module
```

## 遗留问题 / 范围外发现

1. **`backups/` 只保留最近 3 份还没有做。** 5.4.5 规定迁移成功后回收，brief 没有列入。建议归 T027 的保留清理：按文件名排序即可，忽略 `host.partial`。
2. **交接给 T037（启动接线）：**
   - host.db 的顺序：`open_writer(host.db, Database::Host)` → `store::migrate(&mut conn, MIGRATIONS, &data_dir.join("backups"), &*clock)` → 用 `meta::get` 读出 epoch 判定的输入，用 `meta::set` 写 `LastStartAt`、`BootSessionUuid`、`DataEpoch` → `Writer::spawn`。
   - raw.db：`open_writer(raw.db, Database::Raw)` → `prepare_raw(&mut conn, &RAW_SCHEMA)`。
   - 退出码映射：`TooNew` → 4；`Failed` → 6，界面显示 `backup`。`Backup`、`Unreadable`、`BadVersion` 建议映射为 5（磁盘满、权限问题或库已损坏），由 T037 决定。
3. **交接给 T026：**
   - 写事件时 `v` 取 `Upcasters::host().current(type)`，可以替代 T011 交接事项里的 `EVENT_VERSION`；读事件时调用 `upcast(type, v, data)`。
   - 写检查点时 `view_v` 取 `VIEW_VERSION`；读检查点时调用 `checkpoint_fate(view_v)`。
   - **需要定一下**：`turn_snapshots.v` 是整个快照一个版本，而事件的 `v` 是按类型分的，ops 里的事件体也不带 `v`。T026 要定下这一列的含义，例如让每个 op 带自己的 `v`。
4. **`VIEW_VERSION` 的位置。** main 上 T014 已经合并，视图的 JSON 属于 schema。要不要把它挪到 `cupper-protocol::view`，请你决定。
5. **以后的迁移脚本要注意外键。** 脚本在 `foreign_keys=ON` 下执行。T160 之后如果有脚本按 SQLite 的 12 步法重建表，需要在事务外关闭外键，目前的框架没有处理。
6. **已有问题（范围外，没有改）**：`RUSTDOCFLAGS=-D warnings cargo doc -p cupper-host --all-features` 会失败。原因是 `driver/tap/mod.rs`（T050）的文档链接指向了私有的 `redact`。CI 不跑 rustdoc，所以没有被发现。
7. **合并**：分支开出后 main 合并了 T014，只改了 `cupper-protocol` 和文档。我核对过，main 上的 `cupper-host` 和 `Cargo.lock` 都没变，预计不会冲突。
