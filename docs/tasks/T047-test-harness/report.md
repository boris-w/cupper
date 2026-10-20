# T047 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T047-test-harness`
- 完成日期：2026-10-20

## 做了什么

收窄后的 T047 已完成：静止屏障接上了宿主里的全部参与者，`test/clock_advance`、`test/run_maintenance`、`test/seed` 三个测试接口都能用。IT-clock-advance 和 IT-seed 通过，在干净的 worktree 上 `cargo xtask ci pr` 一次通过。净增约 946 行（增 1,075、删 129，只算 `crates/`），在约 1,500 的上限之内。

1. **静止屏障**（`quiescence.rs`，按裁定第 1 条）
   - 参与者从 `enter` 起就算忙。只有在 `Participant::idle(fut)` 里、并且内层返回 Pending 时，才记为闲。
   - `idle` 给内层换上自己的唤醒器：任何唤醒都先记为忙，再转给原来的唤醒器。所以在途的消息、已触发但还没被消费的定时器，都从唤醒那一刻起算作忙。
   - 如果唤醒发生在 poll 和"记为闲"之间，按唤醒计数判定，不会被误判成闲。参与者结束之后，迟到的唤醒不再计数。
   - 另有三样配套：
     - `idle_blocking` 给写线程用；
     - `Hold` 给脱离发起方的工作用；
     - `settled()` 一直等到忙的参与者和 `Hold` 都归零。
2. **各参与者接线**
   - **写线程**：每个提交的操作带一个 `Hold`，回执答复之后才释放；空闲时阻塞在 `recv` 上。
   - **原始帧写线程**：阻塞时算闲。帧来自外部适配器，不持有 `Hold`。
   - **Maintenance、账本清理**：在定时器上 idle。账本清理是新种类 `ledger_sweeper`。
   - **网关**：连接（读循环、握手期限、关闭宽限）、发送循环（队列和合并窗口）、订阅表、等待回复的 task 都是参与者；客户端通知持有 `Hold`。这些种类经 `Routes::watched_by` 登记。
   - **索引**：在邮箱上 idle；它另起的 task 持有 `Hold`，把结果交给 actor 之后才释放。
   - **会话 actor、`AgentRuntime`**：`select!` 改成先在 `idle` 里等到下一件事，然后在外面处理。
3. **`test/clock_advance`**
   - 在屏障之外的单独 task 里执行，所以回复所在的 task 等待时是闲的，屏障能够静止。
   - 先等静止，然后沿 `next_deadline` 逐个调用 `advance_to`，每一步都等静止，最后推进到目标时间。
   - 宿主跑在系统时钟上时返回 `unsupported`。
   - `ManualClock` 由 test-harness 端口持有，`Quiescence` 留在 `Host` 上，供测试使用。
4. **`test/run_maintenance`**：`ledger` 调用 `Ledger::sweep`，`raw_frames`、`audit`、`orphan_blobs` 调用 `Maintenance::run`。`Routes::ledger` 改为公开。
5. **`test/seed`**
   - 会话挂在排在第一位的项目下，所以要先添加一个项目。
   - 写入走正常路径：`session.configured` 和 `sessions` 行一起写；每个回合一个批次，同一事务里写 `turns` 行，回合快照和检查点由流的写者照常带出。视图拒收任何事件时报错。
   - 写完后，经 test-harness 专用的 `IndexHandle::reconcile` 让索引按表重新对账（裁定第 5 条）。
   - SP-1 的比例：用一次性脚本统计了 48 份录制，每 20 个条目里消息 9 条（约 67 字符）、工具调用 8 个、思考 3 条（约 44 字符）。
6. **其他**：
   - `host/status.pid` 在 T037 时已经有了。
   - 能接上的注入点都已接上：`clock`（本任务）、`durations`、`failpoints`、`boot_session_uuid`、`agents`、`login_env`。
   - 剩下的注入点：`notifier` 归 T150，`registry_url` 归 T056/T115，`rss_samples` 归 T008。
   - UNIT-unsupported-set：三个 `test/*` 方法的理由改为 `Builds`，test-harness 构建会服务它们；`diag/recording_export` 改为 `Until("T116")`。

## 关键取舍

- **用唤醒器判定忙闲，不逐个通道计数**：这个做法覆盖所有消息来源和定时器，各 actor 只需要把等待点包进 `idle`。
- **会话 actor 和 runtime 拆开"等"和"做"**：处理过程中等写线程或读池时，参与者保持忙，屏障不会在处理中途误判静止。
- **种类枚举用例**：in-crate 启动真实的 `Host`，用它登记的种类和源码里所有 `ActorKind::new("…")` 比对。新增一种 actor 却没登记时，这个用例就会失败。

## 验证方式

- [x] **IT-clock-advance**，两个用例：
  - `it_clock_advance_returns_once_the_host_settled`：真实宿主进程，手动时钟。
    - 一条只收了 `host/hello`、没有握手的连接：推进 9,999 ms 后，再推进 1 ms，返回时连接已经被关闭；
    - 推进 49 小时后，账本每小时一次的清理经写线程删掉了超过 48 小时的命令行，`clock_advance` 返回时库里已经是 0 行。
  - `it_clock_advance_the_host_registers_every_kind_of_actor`：in-crate 的种类枚举用例。
- [x] **IT-seed**（`it_seed_writes_sessions_the_normal_way`）：灌入 3 个会话、每个 2 个回合、每回合 20 个条目，外加一个 2,000 条目的回合。
  - 结果：`sessions` 3 行、`turns` 7 行、`turn_snapshots` 7 行；会话检查点 3 个；最大回合的快照超过 4,000 个 op；索引里写了 3 条 `session.upserted`。
- [x] 屏障的单元测试：忙闲、唤醒、`Hold`、手动时钟触发的定时器。
- [x] 不睡眠。
- [x] 测试名以用例编号开头；分支上 `tests/ids.toml` 的 T047 段没有改动。
- [x] 四种特性组合（release、dev-auth、test-harness、全部）都跑过 clippy 和宿主测试；生命周期集成测试连续跑 5 次，都通过。
- [x] 在干净的 worktree 上跑了 `cargo xtask ci pr --record docs/tasks/T047-test-harness/ci/`，结果是 rust 1592、protocol 185、ts 353、license、testdata-scan 全部通过，摘要 `ci/pr-1908c99677c9.json` 已提交。其中 `sensitive-words` 因本机没有词表而跳过。

## 提交列表

```
afff320 docs(task): T047 CI summary (pr at 1908c99677c9)
1908c99 docs(task): T047 notes: actual lines, barrier points, hand-offs
6d08ae4 test(host): IT-clock-advance and IT-seed
bbb6b77 feat(host): test/clock_advance, run_maintenance and seed on the barrier
3ccbde2 feat(host): the quiescence barrier counts busy participants and holds
3991a73 docs(task): narrowed T047 estimate after the split
1374e72 docs(task): estimate T047 by module and propose a split
```

## 遗留问题 / 范围外发现

1. **给新增 actor 的任务（T059、T100、T116 等）**：
   - 种类要在 `Host::start` 里登记，否则枚举用例会失败；
   - 等待点要包进 `Participant::idle`；
   - 脱离发起方的 task 要持有 `Hold`，把结果交出去之后再释放。
2. **T116**：在 `supervisor/harness.rs` 旁登记 `diag/recording_export`，并删掉 UNIT-unsupported-set 里的 `Until("T116")`。
3. **T117**：`test/seed` 要求先有一个项目；seed 返回时，索引快照里已经有这些会话。
4. **屏障覆盖不到的地方**：等待外部 I/O 的环节（适配器输出、子进程）不在屏障范围内，这和设计里"等待外部进程时用事件做屏障"一致。原始帧不持有 `Hold`，如果 T116 的 IT 需要等原始帧落盘，要另想办法。
5. **会话 actor 的 attach**：另起的那个 task 没有持有参与者，它等的是 `AgentRuntime` 的回复，而回复要等适配器这个外部进程。中间有一个很短的空档（回复已经发出、还没转给会话 actor），目前没有用例依赖这个空档。