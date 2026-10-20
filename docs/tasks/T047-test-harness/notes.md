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
