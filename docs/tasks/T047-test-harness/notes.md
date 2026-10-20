# T047 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前的规模估计

估算前读过的材料：
- 任务书；
- `handoff-notes.md` 第一列含 T047 的各行：T013、T022、T024、T027、T030、T033、T037、T039、T050、T052、T034；
- 设计 5.2.16、5.9.3、5.9.6、5.9.9；
- 代码：`quiescence.rs`、`clock/manual.rs`、各参与者（写线程、原始帧写线程、Maintenance、索引、会话 actor、`AgentRuntime`、网关的连接与回复 task、账本清理）、`cupper-protocol` 的 `test/*` 和 `diag/recording_export` 类型、`recording.rs`、`tools/sanitize`（TypeScript）、SP-4a 的 `measure.rs`、xtask 的 `perf` 作业（现在只是占位）。

行数指 rustfmt 展开后的行，含文档注释和空行。

### 第一部分：静止屏障、`test/clock_advance`、`test/run_maintenance`、`test/seed`

| 模块 | 内容 | 草估 |
|---|---|---|
| `quiescence.rs` | 每类参与者的忙、闲计数；`Participant::idle(fut)`：等待的 future 返回 Pending 才记为闲，任何唤醒在唤醒器里先记为忙（见待裁定 1），所以在途的消息和已触发、尚未被消费的定时器不必逐个通道去数；`settled()` 等全部参与者都闲 | 150 |
| 各参与者接线 | 写线程：提交到回执之间计为在途，阻塞在 `recv` 且没有在途时才算闲；原始帧写线程；Maintenance；`spawn_blocking` 的各调用点（读池、信任扫描、git）；网关的连接任务和等待回复的 task；账本清理；索引 actor 和它另起的 task；会话 actor；`AgentRuntime` | 200 |
| `test/clock_advance` | 逐个到期时间推进（`next_deadline` 加 `advance_to`），每一步等静止；`Quiescence` 和 `ManualClock` 留在 `Host` 上；`test/*` 的端口只在 test-harness 构建里登记 | 110 |
| `test/run_maintenance` | 四种任务；`Routes` 加公开的账本访问 | 45 |
| `test/seed` | 按 profile 经正常写入路径灌入：`sessions` 行、会话流的事件（`turn.completed` 带出回合快照和检查点）、索引里的摘要；SP-1 的条目类型比例和文本长度一次算好写成常量；单回合 2,000 条目 | 250 |
| 其余注入点 | `notifier` 归 T150；`runtime.registry_url`、`rss_samples` 的接收方还没有，只核对能接上的那些 | 30 |
| **实现小计** | | **785** |
| 测试 | IT-clock-advance（含"全部 actor 类型都已登记"：扫描源码里的 `ActorKind::new` 与宿主登记的种类比对）150；IT-seed 80；屏障的单元测试 80 | 310 |
| **第一部分合计** | 草估 1,095；按实测比约 1,175；实现乘 1.7 约 1,645 | |

### 第二部分：`diag/recording_export`

| 模块 | 内容 | 草估 |
|---|---|---|
| 导出 | 按会话找出它用过的适配器实例；从 `raw.db` 读帧；按实例分段，`ts` 换算成段内相对时间；`Header::to_line`、`Frame::to_line` 逐行写出；写成 blob 返回 | 150 |
| `stderr` 和 `note` 方向 | 驱动写原始帧时补上这两种：实例启动、退出的 note，stderr 逐行写入（会改到 T050、T055 的代码） | 80 |
| 脱敏 | 见待裁定 2 | 20–150 |
| IT-recording-export | 真实宿主加假 agent 跑一个回合；导出；格式库读回；过脱敏扫描 | 120 |
| **第二部分合计** | 约 370–500 | |

### 第三部分：性能测量与趋势

| 模块 | 内容 | 草估 |
|---|---|---|
| `tests/perf.rs`（迁移 `measure.rs`） | PERF-host-cold-start（`test/seed` 灌 200 个会话，从拉起到拿到索引快照）、PERF-index-snapshot（200 个会话）、PERF-host-memory（6 个假 agent 会话）；每项 30 个样本，报 p50、p95、最大值 | 200 |
| xtask 的 `perf` 作业 | 现在只有占位：跑 release 构建的测量、解析结果、写入历史序列（只留数值和版本）、按历史中位数的 1.5 倍判定 | 180 |
| 测试 | 历史序列的读写与判定 | 60 |
| **第三部分合计** | 约 440 | |

### 合计

| 口径 | 合计 |
|---|---|
| 草估 | 约 1,900–2,040 |
| 按实测比 | 约 2,000–2,150 |
| 实现乘 1.7 | 约 2,500 以上 |

三种口径都超过约 1,500 的上限，所以开工前停下，见下一节。

## 汇报与拆分建议

**方案 A：按任务书的三分法拆成三个任务（建议）。**

- **T047（关键路径）**：静止屏障、`test/clock_advance`、`test/run_maintenance`、`test/seed`、注入点核对；验收 IT-clock-advance、IT-seed。按实测比约 1,175，乘 1.7 约 1,645。
  - 如果你仍按乘 1.7 计、认为这一份超线，可以把 `test/seed` 和 IT-seed 再拆出去：屏障一份约 850，seed 一份约 400。
- **T047b**：`diag/recording_export`；验收 IT-recording-export。约 370–500，取决于待裁定 2。
- **T047c**：性能测量与趋势；验收三个 PERF。约 440。依赖 T047（要用 `test/seed`）。

**依赖关系要请你核对。**
- T064 的计划一行写着"录制脱敏后入库"，看起来要用 `diag/recording_export`，所以 T064 可能还依赖 T047b，不只依赖第一部分。
- PERF-index-snapshot 只要索引里有 200 个会话摘要。`test/seed` 写行后由索引补出摘要（T034 的对账也会按行补出），不需要等 T100 的状态计算。

## 待裁定（开工前请一并定）

1. **屏障的判定方法。**
   - 交接要求"统计在途的消息和已触发、尚未被消费的定时器"。
   - 我打算不逐个通道去数，而是让 `Participant::idle(fut)` 给内层 future 包一个唤醒器：任何唤醒（消息到达、定时器触发、子任务完成）都先在唤醒器里把这个参与者记为忙，等它下次被 poll、内层返回 Pending 时再记为闲。这样只要发送方或定时器唤醒了某个参与者，屏障就不会判它为静止。
   - 例外有三类，各自另计：
     - 写线程不是 future，按"提交到回执之间"计在途；
     - 另起的 task 和 `spawn_blocking` 在发起方同步拿到参与者，随 task 结束释放；
     - 等待外部 I/O 的连接任务算闲，这与设计里"等待外部进程时不用 `clock_advance`"一致。
   - 这样理解是否可以？
2. **`diag/recording_export` 的脱敏。** `tools/sanitize` 是 TypeScript 工具，宿主跑不了它。两种做法：
   - **(a)** 宿主导出未脱敏的录制（只有 test-harness、dev-auth 构建有这个方法），由调用方（T064 的 `xtask compat`、IT）在入库前跑 `tools/sanitize`。IT-recording-export 在测试里调用 node 跑一遍脱敏和扫描。
   - **(b)** 把脱敏规则移植进宿主。约多 150 行，两份规则也要保持一致。

   我倾向 (a)。
3. **`stderr` 和 `note` 写进原始帧，会改到驱动（T050、T055）的代码。**
   - 实例启动和退出各写一条 note；
   - stderr 逐行写一帧。

   stderr 现在经 `AdapterLog` 写进日志文件。要不要同时进 `raw.db`？我倾向同时写，并按原始帧的结构化脱敏规则处理。
4. **性能历史序列的位置和格式。**
   - 建议放在 `testdata/perf/history.jsonl`，每行 `{"date", "version", "metric", "p50_ms", "p95_ms", "max_ms", "n"}`，不含机器名和路径。
   - 由 `xtask ci perf` 追加并据此判定。
   - 这一作业现在是占位，我理解实现它也在范围内，请确认。
5. **`test/seed` 与索引的关系。** 灌入的会话要出现在索引里。
   - 建议给 `IndexHandle` 加一个只在 test-harness 构建里存在的入口，按 `sessions` 行重新对账（复用 T034 的 `reconcile`）。
   - 否则只能重启宿主才看得见这些会话。

## 拆分之后（计划 v1.3.21）

编排者采用方案 A：本任务只做静止屏障、各参与者接线、`test/clock_advance`、`test/run_maintenance`、`test/seed`、`host/status.pid` 和注入点核对。录制导出归 T116，性能与趋势归 T117。5 项待裁定全部认可。上限约 1,500 行，硬停止线 1,600；推算超线时把 seed 拆出去。

### 收窄后的估计

| 模块 | 估计 |
|---|---|
| `quiescence.rs`：忙闲计数、`Participant::idle` 的唤醒器、`Hold`（脱离发起方的工作和写线程的在途作业）、`settled()`，含单元测试 | 230 |
| 参与者接线：写线程、原始帧写线程、Maintenance、账本清理、网关（连接、回复 task、通知、发送循环、订阅表）、索引、会话 actor、`AgentRuntime` | 200 |
| `Host` 留下 `Quiescence` 和 `ManualClock`；`test/*` 端口；`test/clock_advance`；`test/run_maintenance`；`Routes` 的账本访问 | 180 |
| `test/seed`；索引按表重新对账的 test-harness 入口 | 260 |
| IT-clock-advance（含枚举用例）、IT-seed | 230 |
| **合计** | **约 1,100** |

### 实际行数

按提交统计（`git diff --numstat`，只算 `crates/`）。

| 提交 | 内容 | 增 | 删 |
|---|---|---|---|
| `feat(host): the quiescence barrier counts …` | `quiescence.rs`（含单元测试 2 个） | 269 | 23 |
| `feat(host): test/clock_advance, run_maintenance and seed …` | 各参与者接线、`Routes::watched_by`、`supervisor/harness.rs`、`supervisor/seed.rs`、索引的重新对账入口、UNIT-unsupported-set | 631 | 96 |
| `test(host): IT-clock-advance and IT-seed` | 两个 IT、种类枚举用例、seed 拒收检查 | 175 | 10 |
| **合计** | | **1,075** | 129 |

净增约 946 行，比估计的约 1,100 少。

### 实现要点

- **屏障**（裁定第 1 条）：
  - 参与者从 `enter` 起就算忙；只有在 `Participant::idle(fut)` 里、并且内层 future 返回 Pending 时，才记为闲。
  - `idle` 给内层 future 换上自己的唤醒器：任何唤醒都先记为忙，再转给原来的唤醒器。如果唤醒发生在 poll 和"记为闲"之间，按唤醒计数判定，不会被覆盖成闲。
  - 参与者结束后，迟到的唤醒不再计数。
  - `idle_blocking` 给写线程用；`Hold` 给脱离发起方的工作用。`settled()` 等忙的参与者和 `Hold` 都归零。
- **各参与者**：
  - 写线程：每个提交的操作带一个 `Hold`，回执发出之后才放；
  - 原始帧写线程：帧来自外部的适配器，不持有 `Hold`；
  - Maintenance、账本清理（新种类 `ledger_sweeper`）：在定时器上 idle；
  - 网关：连接（`gateway_conn`，读循环、握手期限、关闭宽限）、发送循环（`gateway_send`，队列和合并窗口）、订阅表（`gateway_subscriptions`）、等待回复的 task（`gateway_reply`）；客户端通知持有 `Hold`；
  - 索引 actor：在邮箱上 idle；它另起的准备 task 持有 `Hold`，把结果交给 actor 之后才放；
  - 会话 actor 和 `AgentRuntime`：把 `select!` 改成先在 `idle` 里等到"下一件事"，再在外面处理。
- **`test/clock_advance`**：
  - 在屏障之外单独起一个 task 执行；回复所在的 task 在等它时是闲的，所以屏障能静止。
  - 先等静止，然后沿 `next_deadline` 逐个 `advance_to`，每一步都等静止，最后推进到目标时间，再等一次静止。
  - 宿主跑在系统时钟上时，返回 `unsupported`。
- **`test/run_maintenance`**：`ledger` 调用 `Ledger::sweep`，其余三种调用 `Maintenance::run`。为此另建了一个 `Maintenance` 实例；`Routes::ledger` 已改为公开。
- **`test/seed`**：
  - 会话挂在排序第一的项目下，所以需要先添加一个项目。
  - 每个会话：`session.configured` 和 `sessions` 行一起写；每个回合一个批次，含 `turn.started`、用户消息、按比例生成的条目、`turn.completed`，同一事务里写 `turns` 行并更新 `last_turn_end_seq`。回合快照和检查点由流的写者照常带出。
  - 视图拒收任何事件就报错。
  - 最后让索引按表重新对账（`IndexHandle::reconcile`，只在 test-harness 构建里有）。
- **SP-1 的比例**：用一次性脚本统计了 48 份录制（2026-10-20）。每个 prompt 里，agent 消息 68 条（文本均长 67 字符）、工具调用 67 个、思考 23 条（均长 44 字符），每回合平均 2.5 个条目。据此取每 20 个条目里消息 9、工具 8、思考 3。
- **种类枚举**：in-crate 用例启动真实的 `Host`，用它登记的种类与源码里所有 `ActorKind::new("…")` 比对。为此 `Host` 在 test-harness 的测试构建里留着 `quiescence` 字段。
- **注入点核对**：`clock`（本任务接上手动时钟）、`durations`、`failpoints`、`boot_session_uuid`、`agents`、`login_env` 都已接上；`notifier` 归 T150，`runtime.registry_url` 归 T056/T115，`rss_samples` 归 T008。`host/status.pid` 在 T037 时已经有了。

## 给其他任务的接入点

- **新增 actor 的任务**（T059、T100、T116 等）：
  - 种类要在 `Host::start` 里登记，否则枚举用例会失败；
  - 等待点要放进 `Participant::idle`；
  - 脱离发起方的 task 要持有 `Hold`，在把结果交出去之后再放。
- **T116**：在 `supervisor/harness.rs` 旁边登记 `diag/recording_export`（test-harness 和 dev-auth 构建），并删掉 UNIT-unsupported-set 里对应的 `Until("T116")` 一行。
- **T117**：`test/seed` 要求先有一个项目。冷启动数据集（200 个会话、单会话 1,000 条消息）用 `items_per_turn` 和 `turns` 组合出来；索引快照在 seed 返回时已经包含这些会话。
