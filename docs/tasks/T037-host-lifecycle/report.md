# T037 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T037-host-lifecycle`
- 完成日期：2026-10-19

## 做了什么

收窄后的 T037 已经做完：宿主能按启动顺序真正启动；`host/exit`、SIGTERM 和写线程死亡三种情况都会触发有序退出。IT-lifecycle 通过。`cargo xtask ci pr` 第二次运行全部通过；第一次被一个 T058 的性质测试挡住，那不在本任务范围内（见"遗留问题"第 1 条）。

规模：增加约 1,342 行，删除 55 行，在约 1,500 行的上限内。统计只算 `crates/` 和 `tests/ids.toml`，不含 `Cargo.lock`。每个提交的行数与估计的对比写在 `notes.md`。

1. **主线程的 run loop**（`cupper-sys/src/runloop.rs`）
   - `RunLoop` 往 run loop 里加了一个自己的 version 0 source。`RunLoopStop::stop()` 可以从任意线程调用。
   - 这样空的 run loop 不会一跑就返回，run 之前先发出的 stop 也不会丢。
   - `unsafe` 都在 `cupper-sys` 里，每处都有 `SAFETY:` 注释。
2. **`main` 重组**
   - 先解析命令行，处理 `--build-info`。发布构建随即把 stdio 指向 `/dev/null`；`dev-auth` 构建接着核对拉起方。
   - 然后建多线程运行时（worker 数为 min(4, 核数)），读启动载荷，装故障点。读载荷的构建在这之后才重定向 stdio。
   - `Host::start` 在主线程上 `block_on`，失败时直接按退出码返回。
   - 启动成功后，`Host::run` 交给 worker 执行，主线程跑 run loop。宿主退出时（包括 panic）停掉 run loop，再取出退出码。
3. **数据目录**（`startup::prepare_data_dir`）：不存在就建成 0700；属主是当前用户就收紧到 0700；属主不对或者不是目录，以退出码 5 退出。
4. **启动第 3–10 步**（`supervisor/mod.rs` 的 `Host::start`）
   - 依次是：
     1. 数据目录；
     2. `host.lock`；
     3. host.db：`open_writer`、`migrate`，然后在同一个事务里做 epoch 判定、写 meta，并改写上次运行留下的行（`supervisor/recover.rs`）；
     4. 写线程和原始帧写线程（两者连同 Maintenance 都先登记进静止屏障）；
     5. 读池、`Streams`、`BlobStore`、Maintenance；
     6. 另开 `open_reader` 装载账本，启动账本清理任务；
     7. `Routes::new(ledger)`，登记 `host/status`、`host/exit`；
     8. 网关，`dev_key` 带上拉起方 ID；
     9. SIGTERM 处理、绑定套接字、开始 accept。
   - 退出码：
     - 3：`host.lock` 已被占用；
     - 4：`TooNew`；
     - 6：迁移 `Failed`；
     - 5：数据目录、打开数据库、`Backup`、`Unreadable`、`BadVersion`、meta 值无效、账本装载失败；
     - 1：读不到启动会话 UUID、起不了线程或信号处理。
5. **启动时清理状态**：只改库，不追加事件。
   - 运行中的回合改为 `interrupted`，写上 `ended_at`；
   - 待处理的交互改为 `expired`，原因 `host_restarted`；
   - `sessions.state` 从 `running` 改为 `interrupted`。
6. **写线程死亡通知**（改动 T024 的代码）：`WriterThread::ended()` 在写线程结束时完成，panic 也算。宿主随即有序退出，退出码 1，不写 `last_clean_exit_at`。
7. **有序退出**（`Host::exit`），顺序是：
   1. 停止 accept；
   2. 广播 `host/shutdown {user_quit, restart_expected: false}`；
   3. 各部件依次 `stop(force)`；
   4. 停清理任务和账本清理任务；
   5. 提交 `last_clean_exit_at`，作为最后一个写入；
   6. 停原始帧写线程和写线程；
   7. 关闭所有连接：先发完队列里的帧，最多等一个关闭宽限；
   8. 删除套接字，释放锁。

   只有不带 `force` 的 `host/exit` 算非强制；SIGTERM 和写线程死亡都按强制处理。
8. **网关**（改动 T030 的代码）
   - 新增已初始化连接的登记表 `gateway/clients.rs`，由它推出 `ClientInfo`，T034 可以复用。
   - 新增 `broadcast::<N>()`。
   - 新增 `close_all()`：经 `closing` 通知读循环结束，并等所有连接任务退出。
   - 新增关闭原因 `CloseReason::HostExiting`。
9. **`host/status`、`host/exit`**（`supervisor/control.rs`）
   - `host/status` 返回：
     - 版本；
     - `pending_upgrade`：在 T059 之前固定为 false；
     - 回合数和交互数：由各部件汇总，现在没有部件，所以是 0；
     - 客户端列表；
     - pid：只在 test-harness 构建里有。
   - `host/exit`：不带 `force` 且有回合在运行时回 `conflict/turn_running`；否则先把账本 ticket 落盘，再通知监护者退出。
   - 这两个方法已从 UNIT-unsupported-set 的允许表里删掉，测试夹具的方法表也登记了它们。
10. **接入点**：新增 `supervisor::Part` trait，包含 `activity()` 和 `stop(force)`，由 T043、T055、T034 依次加进去。具体写在 `notes.md` 的"给其他任务的接入点"。

## 关键取舍

- **清理任务提前停**：比任务书写的早一步，在写 `last_clean_exit_at` 之前停。这样它在途的写入不会排到 `last_clean_exit_at` 后面，`last_clean_exit_at` 才是写线程停之前的最后一个操作。
- **连接放到最后关**：`host/exit` 的回复要等回复的 future 完成后才进发送队列。连接最后关，就能保证回复在关闭前入队。它和 `host/shutdown` 谁先谁后不保证，这符合裁定第 8 条。
- **`Streams` 已组装但还没有使用者**：字段标了 `#[expect(dead_code, reason = …)]`。T043 或 T034 接上之后，编译器会报"期望未满足"，提醒删掉这个标注。
- **审计接收端先丢弃**（`_audit`），由 T059 接到写线程。
- **启动清理没有单独写单元测试**：由 IT-lifecycle 在进程级覆盖，省了约 45 行。
- **SIGTERM 用 tokio 的 `signal` 特性**：新增依赖 `signal-hook-registry`、`errno`，都是 MIT OR Apache-2.0，license 作业已通过。
- **改了 T023 的一个用例**：`unit_release_features_host_does_not_read_stdin` 原来断言宿主"not assembled"、退出码为 1。宿主组装好之后会一直运行（我第一次跑 CI 时它就挂住了），所以改为：等它开始监听，确认 stderr 为空，发 SIGTERM 后断言退出码为 0。

## 验证方式

- [x] **IT-lifecycle**：主体在 `tests/lifecycle.rs`（`required-features = ["test-harness"]`），crate 内另有两个 `it_lifecycle_*` 用例。
  - `..._startup_ends_what_the_last_run_left_running`：启动前直接写库，造出运行中的回合、待处理的交互和运行中的会话；启动后断言它们分别变成 `interrupted`、`expired/host_restarted`、`interrupted`，对照用的行保持不变。
  - `..._host_exit_and_sigterm_exit_in_order`：`host/exit` 和 SIGTERM 各跑一遍，断言：
    - `host/status` 的内容；
    - 客户端先收到 `host/shutdown`，然后连接关闭；
    - 退出码为 0；
    - `last_clean_exit_at` 不小于 `last_start_at`；
    - 套接字已删除，锁能重新拿到。
  - `..._startup_failures_exit_3_to_6`：断言退出码 3、4、5（数据目录是个文件；`schema_version` 被写乱）、6。
  - crate 内的两个：
    - `..._gateway_shutdown_reaches_clients_then_all_connections_end`：网关层；
    - `..._host_exit_needs_force_while_turns_run`。
  - 在 all-features 下连跑 5 次，全部通过。
- [x] **与真实宿主握手**：用 CLI 的 `harness` 客户端连上真实的 `cupper-host`，双向证明都通过，principal 为 `pr_test`。
- [x] **测试名**都以用例编号的 snake_case 开头。
- [x] **`cargo xtask ci pr --record docs/tasks/T037-host-lifecycle/ci/`**：在干净的 worktree 上全部通过（rust 1075、protocol 176、ts 291、license、testdata-scan），摘要 `ci/pr-90f6c710a21a.json` 已提交。第一次运行失败了，原因见"遗留问题"第 1 条。
- 本地在 release、dev-auth、test-harness、all-features 四种特性组合下都跑过 clippy（`-D warnings`）和宿主测试。

## 提交列表

```
2a6bde0 docs(task): T037 CI summary (pr at 90f6c710a21a)
90f6c71 docs(task): T037 notes: the release stdin test
f3fc794 test(host): a release host that ignored its stdin comes up
c29c3cd docs(task): T037 notes: actual lines, startup and exit order, hand-offs
4ba7062 test: register the T037/T059 split in tests/ids.toml
4174758 test(host): IT-lifecycle on the host process
6a926b2 feat(host): assemble the host, serve, and exit in order
dff18b3 feat(host): keep the data directory private to this user
a640efb feat(host): gateway client table, host/shutdown broadcast and close_all
c6e33ad feat(host): tell when the store writer thread has died
b832a88 feat(sys): a run loop that another thread can stop
1fb2a12 docs(task): narrowed T037 estimate after the split
486d67f Merge branch 'main' into feat/T037-host-lifecycle
57de3d9 docs(task): estimate T037 by module and propose a split
```

## 遗留问题 / 范围外发现

1. **T058 的性质测试有一个可复现的反例**（范围外，我没有修）。
   - 用例：`orchestrator::session_props::prop_autonomous_dequeue_guard_queue_waits_while_the_agent_works`。第一次跑 CI 时随机撞上，断言"Guard: after a turn ended on silence, the queue waits for 30 s of quiet"失败。
   - 种子：`cc 7390a05ebe580707816e5bab38bbf67b0501e1d444018d9994dbaf6d6edc9900 # shrinks to ops = [Send, Send, Interrupt, Text(0), Advance(25000), Advance(25000), Thread(true)]`。把它放进 `crates/cupper-host/proptest-regressions/orchestrator/session_props.txt` 就能每次复现。main 上的代码与本分支相同，所以 main 也有这个问题。
   - 这个回归文件没有被 git 忽略，所以同一次运行里 protocol 作业也报了"工作树被改动"。我把种子挪出工作树后重跑，才得到通过的摘要；种子另外记在了 `notes.md` 里。
   - 建议另开 fix 任务，或者交给 T043（它要照这个组合核心接线）。另外，`proptest-regressions/` 要么入库，要么加进 `.gitignore`，需要定一下。
2. **拣了 main 上的一个提交**：main 在合入 `486d67f` 之后又提交了 `6d214fd`，内容是 `tests/ids.toml` 里 T037 与 T059 的拆分。本分支的文档已经带上 IT-idle-exit，缺了这条登记，`testdata-scan` 会失败。所以我用 `cherry-pick -x` 把它拣了过来，即 `4ba7062`。改动与 main 完全相同，合并时不会冲突。我没有做 merge 或 rebase。
3. **交给后续任务的事项**（详见 `notes.md`）：
   - **T043**：
     - 实现 `Part`，排在第一个；
     - 删掉 `streams` 上的 `#[expect(dead_code)]`；
     - 载入会话时补写 `turn.completed{interrupted}` 和交互的 `expired` 事件，并决定是否暂停队列；
     - 改用 `Routes::start` 时一并调用 `Control::route`。
   - **T055**：
     - 实现 `Part`，排在第二个，在 `stop` 里按 ADR-09 关闭适配器；
     - `raw.sink()` 在 `Host::start` 里取。
   - **T034**：
     - 实现 `Part`，排在第三个，在 `stop` 里写索引流的检查点；
     - `clients.updated` 用 `Gateway::clients()`；
     - 启动第 7 步（加载索引流）放在绑定套接字之前。
   - **T059**：
     - 空闲、孤立、待升级作为新的 `Exit` 取值，从 `Host::run` 进入同一条退出路径；
     - `host/shutdown` 的 `reason` 改为按 `Exit` 取值，空闲用 `idle`，待升级用 `upgrade`；
     - 把 `_audit` 接到写线程，`HostExiting` 在审计里记为 `host_exiting`；
     - 清理残留放在 `maintenance.spawn` 之前；
     - `pending_upgrade` 改为取自检结果。
   - **T047**：`test/clock_advance` 需要的 `Quiescence` 和 `ManualClock` 目前只是 `Host::start` 里的局部变量，届时要留在 `Host` 上；账本清理任务还没有登记进静止屏障。
   - **T028**：stdio 重定向之后的启动失败原因现在写进了 `/dev/null`，要改为写进 host.log。