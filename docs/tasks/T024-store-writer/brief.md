# T024 存储 I：写线程、读池与持久性

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主存储）
- 分支：`feat/T024-store-writer`
- 依赖：T010、T022（都已合并）。M1a 第 2 波；T025、T026 依赖它。

## 背景

宿主的事件日志存在 SQLite（WAL）里，是唯一的历史来源（ADR-06）。设计 5.2.8 规定的存储模型是：
- 一个专用的写线程，做组提交，每个操作包在一个 savepoint 里；
- 4 个只读连接；
- 两档持久性：默认 `NORMAL` 加 `checkpoint_fullfsync`，`pending_external` 用 `FULL` 加 `fullfsync`（v1.3 定案）；
- 断电后 seq 可能被重用，用 `data_epoch` 来判定（5.2.8、5.9.8）。

本任务只实现这套机制，不建表。表结构和迁移由 T025 负责。

## 范围

**做**（计划 T024 一行）：
- **写线程**：
  - 组提交：攒够数量或队列为空就刷出；
  - 每个操作用一个 savepoint，一个操作失败不影响同批的其他操作；
  - 只在 `SystemClock` 下等 2 ms 攒批，测试中不等，但攒批照常开着（设计评审 D12）；
  - 每个操作返回确认回执。
- **读池**：4 个只读连接。
- **两档持久性配置**：pragma 的设置，以及按批切换 `FULL` 加 `fullfsync`，供 `pending_external` 使用。
- **`data_epoch`**：
  - 判定写成纯函数：根据"上次是否异常退出"和"启动会话 UUID 是否变化"，决定 epoch 是否加 1；
  - 两个输入都可以注入：启动会话 UUID 经 `test_config.boot_session_uuid` 传入，读取由 `cupper-sys::boot_session_uuid()` 提供。

**不做**：表结构与迁移（T025）；流与检查点（T026）；业务读写。

## 验收标准

- [ ] 新增 UNIT-writer-savepoint：同一批里一个操作失败，其余操作照常提交；回执与操作一一对应；攒批的边界（够数、队列为空）。
- [ ] 新增 UNIT-epoch-decision：IT-NFR2-epoch 用到的决策表，包括正常退出和异常退出、同一次开机和重启之后；`kill -9` 不会让 epoch 加 1。
- [ ] 写线程登记为静止屏障的参与者（T022 的 `Quiescence`）。
- [ ] 两档持久性的 pragma 在打开连接时生效，测试中读回 pragma 的值来核对。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T024-store-writer/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（`docs/tasks/handoff-notes.md`）

- "只在 `SystemClock` 下等 2 ms"：`Clock` 没有"是否虚拟时钟"的查询。可以从启动配置（`test_config.clock`）判断，也可以给 trait 加一个方法。选哪种写明理由（T022）。
- 写线程是 std 线程，需要登记为静止屏障的参与者（T022 给 T047 的建议）。
- 新增的时长，例如 2 ms，登记到 `durations.rs`；需要故障点时，用 `failpoint!` 并登记（T023）。
- 集成测试要用 `ManualClock` 时，声明 `required-features = ["test-harness"]`（T022）。

## 注意

- rusqlite 用 bundled 构建（5.1.3），锁精确版本，取发布满约两周的版本。
- SQLite 的调用都在写线程或 `spawn_blocking` 里进行，async 工作线程上不做同步 I/O（SP-4a 的教训）。
- 规模上限约 1,500 行。每完成一个模块统计一次行数（`CLAUDE.md` 守则第 8 条），预计会超出时先停下来汇报。

## 参考资料

- 设计：`02-host.md` 5.2.8、5.2.16；`04-data-model.md` 5.4.2（连接参数）；`09-testing.md` 5.9.8；`13-open-issues.md` 的 Q15（已关闭）。
- ADR-06（含设计评审修订：持久性定案）；SP-4a 的 `store.rs`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。
