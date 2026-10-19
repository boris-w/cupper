# T037 进程监护与宿主生命周期

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T037-host-lifecycle`
- 依赖：T020、T026、T027、T030（均已合并；T033、T039 也已合并）。松弛约为 2。T034、T047 等依赖它。

## 背景

宿主的各个部件已经各自做好：
- 启动载荷与退出码（T023）；
- 写线程（T024）、迁移（T025）、流（T026）、blob、原始帧与清理（T027）；
- 网关（T030、T039）、分发与账本（T033）；
- 策略（T031）。

现在的 `main` 只读启动载荷，还没有把这些部件组装成一个能运行的宿主。本任务做三件事：
- 按 5.2.13 的启动 10 步，把这些部件串起来；
- 实现 `Supervisor`（进程监护与生命周期）；
- 给后续任务留好接入点。会话端口（T043）、索引端口（T034）、驱动（T055）还没有。

## 范围

**做**（计划 T037 一行；设计 5.2.13、5.5.6）：
1. **启动 10 步串联**（5.2.13）：
   - **数据目录**：权限 0700，检查属主。
   - **`host.lock`**：已有宿主时以退出码 3 退出。这一步在 T030 的 `listen` 里。
   - **host.db**：`open_writer` → `migrate` → epoch 判定，写 meta → `Writer::spawn`。退出码：`TooNew` 为 4；`Failed` 为 6，界面显示备份位置；`Backup`、`Unreadable`、`BadVersion` 为 5。
   - **raw.db**：`open_writer` → `prepare_raw` → `RawWriter::spawn`。
   - **组装其余部件**：`Streams`、`BlobStore`、`Maintenance`、账本（`Routes::start`）、网关（`Gateway::new`）。
   - **注入点**：把 `test_config.clock` 和 `Durations` 分发给各个模块。
   - **stdio 重定向**：调用 `cupper_sys::redirect_stdio_to_null()`。读启动载荷的构建，读完之后再调用；发布构建一开始就调用。
   - **拉起方 ID**：把 `startup::launcher()` 返回的拉起方 ID 放进 `GatewayConfig::dev_key`。
2. **`Supervisor`**：
   - **清理残留**：按 `adapters` 表，清理上次运行留下的适配器进程组。关闭顺序见交接事项。
   - **空闲退出**：空闲时长接 `test_config.durations`。
   - **有序退出**，按 5.2.13：
     - 停止 accept；
     - 向所有连接发 `host/shutdown` 通知；
     - 强制退出时，把运行中的回合标为 `interrupted`；
     - 按 ADR-09 的顺序关闭适配器，现在还没有适配器，留好接口；
     - 写检查点，清空写线程的队列，写 `last_clean_exit_at`；
     - 依次停掉清理任务、原始帧写线程、写线程。
   - **SIGTERM** 按有序退出处理。
   - **`host/status`、`host/exit`**：在 T033 的方法表里登记，接上后从 UNIT-unsupported-set 的允许表里删掉相应行。
   - **写线程 panic 当作致命错误**：写线程结束之后，宿主有序退出。
3. **自检与待升级**（5.2.13、5.5.6）。
4. **主线程跑 `CFRunLoop`**：按 5.2.13 的方案重组 `main` 的运行时结构。
5. **"孤立"状态**：`dev-auth` 构建里，拉起宿主的父进程退出后，不再开始新回合；运行中的回合结束后，有序退出。会话部分现在还没有，用状态和接口表达，T043 接上。
6. **启动时清理状态**：上次运行留下的运行中回合标为 `interrupted`，待处理的交互标为 `expired`（`host_restarted`）。直接改库即可；会话 actor 的恢复归 T043。
7. **审计落盘**：网关（T030、T039、T033）的审计记录写进 `audit` 表。审计的正式格式由本任务定下，包括 `handshake_timeout`。host.log 由 T028 负责，本任务不做；订阅日志（Q16）在 T028 合并后再写进 host.log。

**不做**：
- 防休眠（T156）；
- 准备阶段的会话恢复（T043）；
- 会话端口、索引端口的实现（T043、T034）；
- host.log 与脱敏（T028）。

## 验收标准

- [ ] 新增 IT-self-check（5.2.13 的自检）。
- [ ] 新增 IT-lifecycle：
  - 启动前直接写库，造出运行中的回合和待处理的交互，断言启动后它们分别转为 `interrupted`、`expired`；
  - 空闲退出；
  - 有序退出：`last_clean_exit_at`、`host/shutdown`，以及 SIGTERM；
  - 各类启动失败的退出码：3、4、5、6。
- [ ] 新增 UNIT-orphan-policy。
- [ ] 宿主进程能真正启动：用 T039 的网关夹具或宿主本身，连上并握手成功。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T037-host-lifecycle/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **关闭进程组**（T020）：
  - 先用 `child.group().with_descendants()` 记下所有进程组；
  - 再逐组 `signal(Term)`，用 `Clock` 限时等待，并用 `is_alive()` 核对；
  - 最后对还在的组 `signal(Kill)`。
  - 持久化时存 `pgid` 和 `start.as_micros()`。
- **`main`**（T023）：
  - stdio 重定向；
  - 按 CFRunLoop 的方案重组；
  - 检查数据目录的 0700 和属主；
  - 分发 `test_config.clock` 和 `Durations`。
- **fd 副本**（T021）：任何 `posix_spawn` 都会短暂复制整张 fd 表。判断对端是否关闭，以读到 EOF 为准。
- **epoch 与写线程**（T024）：
  - 启动第 5 步：`epoch::decide(LastExit::from_meta(..), Boot::since(meta.boot_session_uuid, current_boot_session(&payload)?))`，然后写 `last_start_at` 和 `boot.to_string()`；
  - `Writer::spawn` 之前先 `quiescence.register(store::WRITER)`，写线程的配置用 `WriterConfig::for_startup(&payload, &durations)`；
  - 有序退出时，把写 `last_clean_exit_at` 作为 `stop()` 之前的最后一个操作提交；
  - 写线程上的操作 panic 时，宿主有序退出。
- **迁移**（T025）：host.db 的启动顺序和退出码，见范围第 1 条。
- **流**（T026）：`Streams::new(writer, readers, clock, StreamsConfig::new(epoch, &durations))`。
- **存储**（T027）：
  - 在静止屏障里登记 `store::RAW_WRITER`、`store::retention::MAINTENANCE`；
  - `Maintenance::new(..).spawn(&quiescence)` 放在"按 `adapters` 表清理残留"之后；
  - 有序退出时先停清理任务，再 `RawWriterThread::stop()`；
  - `raw.sink()` 交给 T055，现在留好接口。
- **网关与分发**（T030、T033、T039）：
  - 组装：`Routes::start(conn, writer, clock, durations, sessions, index)` → `(routes, sweeper)`，再 `Gateway::new(config, clock, durations, routes)`；
  - 现在还没有会话端口和索引端口：用 `Routes::default()` 加上本任务登记的 `host/status`、`host/exit`。T043、T034 接上时，改用 `Routes::start` 的完整形式，接线点写进 `notes.md`；
  - 有序退出时 abort 账本清理任务；
  - 关闭原因 `HandshakeTimeout` 在审计里记为 `handshake_timeout`。
- **惯例**：
  - 时间只经 `Clock`；
  - 拉子进程一律经 `cupper_sys::spawn`；
  - `unsafe` 只能在 `cupper-sys`；
  - 用到 `ManualClock` 的集成测试，声明 `required-features = ["test-harness"]`。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 规模

计划估为 L，上限约 1,500 行。这个任务的组装面很广，**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**。推算会超出时，开工前就停下来汇报，并给出拆分建议，例如把"启动串联与组装"和"Supervisor 与退出"分开。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**预计会超出上限时，必须先停下来汇报，不要继续写**。

## 参考资料

- 设计：
  - `02-host.md` 5.2.13（启动契约、启动 10 步、有序退出）、5.2.16；
  - `05-flows.md` 5.5.6；
  - `08-security.md` 5.8.1（孤立状态）；
  - ADR-09（关闭顺序）。
- T020–T033、T039 的代码和 `notes.md`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
