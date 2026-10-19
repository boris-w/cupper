# T040 会话纯核心 I：回合状态机与队列

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主编排）
- 分支：`feat/T040-turn-machine-queue`
- 依赖：T017、T022（均已合并）。M1a 第 3 波；T041（自发回合）、T043（会话 actor，关键路径）、T053（Claude 提取层）依赖它。

## 背景

会话编排里有状态的决策都写成纯核心（设计 5.2.16）。纯核心不做 I/O，不 await，不读时钟，由 actor 驱动；测试用 `CoreDriver` 在虚拟时间上跑。T022 已经搭好框架：`crates/cupper-host/src/pure/` 里的 `Core` trait、`TimerTable`、`CoreDriver`。

本任务做三件事：
- 第一个真正的纯核心：回合状态机 `TurnMachine`，以及排队消息的 `QueueCore`；
- 驱动接口 `driver::api`，它是 TurnMachine 的输入和输出类型。
  - T043 用进程内替身对接它；
  - T055 在 T050 的 ACP 连接之上实现它。

## 范围

**做**（计划 T040 一行；设计 5.2.6、5.2.16、5.6.2）：
1. **`driver::api`**（5.6.2）：
   - 宿主对驱动发的命令和驱动上报的事件；
   - `AgentError` 的分类。
   - ACP 的类型不出 `driver` 模块：接口里只出现 `cupper-protocol` 的类型和宿主自己的类型。
2. **`TurnMachine`**（5.2.6 回合状态机）：
   - 状态：Idle、Preparing、Prompting、Cancelling、Completed、Cancelled、Failed；
   - 准备阶段 3 s 超时：起始快照完成或超时后才发 prompt；
   - 中断后 10 s 兜底：截止后把回合标为 `cancelled`，`stop_reason: host_timeout`，之后这个 prompt 的响应直接忽略；
   - 在途规则：同一会话同一时刻只有一个在途的 prompt（K1）；
   - 恢复会话时先 close 再 resume。
3. **`QueueCore`**（5.2.6 的发送与队列规则）：
   - 回合进行中时，`mode: queue` 的消息排队；
   - 回合正常结束时，如果队列没有暂停，取出队首开 `origin: queued` 的回合；
   - 回合被取消、失败或中断时，队列暂停（`queue.paused`）；
   - Codex 拒绝审批导致的取消（#556）不暂停队列；
   - `session/queue_resume` 恢复队列；`session/queue_edit` 的 `replace`。
4. 时长登记在 `durations.rs`（`Durations`）里，不写死在核心里。

**不做**：
- 插话 Steer（T140）。`session/send` 的 `mode: steer` 和 `queue_edit` 的 `steer`，本任务只留接口位置。
- 自发回合 Autonomous（T041）。状态机里只留进入和退出的位置。
- 交互请求的跟踪（T042）。
- actor 和事件落盘（T043）；驱动的实现（T055）。

## 验收标准

- [ ] 新增 PROP-queue-fifo：排队消息按到达顺序出队；暂停期间不出队；恢复后接着出队。
- [ ] 新增 PROP-interrupt-10s：中断后 10 s 内 prompt 返回 `cancelled`，回合为 `cancelled`；超过 10 s，回合按 `host_timeout` 结束，迟到的响应被忽略，不会改变已结束的回合。
- [ ] 新增 PROP-turn-single-prompt：任何输入序列下，同一会话同时在途的 prompt 不超过一个。
- [ ] 新增 PROP-prompt-inflight-no-end：prompt 在途时回合不会结束（不发 `turn.completed`），中断截止的情况除外。
- [ ] 全部用 `CoreDriver` 在虚拟时间上驱动，不睡眠，不读系统时钟；性质测试用 proptest。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T040-turn-machine-queue/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头（`prop_queue_fifo_` 等）。

## 交接事项与注意

- **T014 的裁定**（`03-protocol-v1.md` 5.3.11 末尾"T014 实现时的裁定"）：
  - 引用某个回合的事件都写在这个回合的 `turn.completed` 之前，包括被放弃条目的 `item.completed`、它的请求的 `interaction.resolved`、`snapshot.recorded` 的 `phase: end`。TurnMachine 输出的动作顺序要保证这一点。
  - 回合在它的交互请求全部解决之前不结束。请求的跟踪归 T042，TurnMachine 要留出接入点。prompt 已经正常返回、但还有请求挂着时怎么处理，例如标为哪种 `ResolutionReason`，请你在 `notes.md` 里提出方案并在汇报中说明，由编排者裁定。
- **纯核心框架**：适配循环写在 `pure/mod.rs` 的文档里。核心需要 ID 时用 `IdGen::new_id_at(now.wall_ms)`。（T022）
- **集成测试**：`tests/` 下用到 `ManualClock`、`CoreDriver`、`IdGen::seeded` 的，要声明 `required-features = ["test-harness"]`。（T022）
- **驱动**：`driver/` 里已有 T050 的 `transport`、`tap`、`acp`。`driver::api` 是新文件，不要改 T050 的文件；`driver/mod.rs` 只加一行 `mod`。
- **惯例**：错误类型用 thiserror；`lib.rs` 只放 `mod` 声明；时间只经 `Clock`。

## 规模

计划估为 L，上限约 1,500 行。三部分（驱动接口、TurnMachine、QueueCore）加上性质测试，估计会接近上限。**开工前先在 `notes.md` 里按模块写行数估计，测试和测试辅助单独列一项**（计划 1.1 的执行经验）；动手后每完成一个模块统计一次（守则第 8 条）。预计会明显超出时，先停下来汇报，并给出拆分建议，例如把 `driver::api` 和 TurnMachine、QueueCore 分开。

## 参考资料

- 设计：
  - `02-host.md` 5.2.6（会话编排、回合状态机、发送与队列规则）、5.2.16（时间与可测性）；
  - `06-agent-driver.md` 5.6.2（驱动接口）、5.6.9（K1、K9 等）；
  - `03-protocol-v1.md` 5.3.11（事件与 T014 裁定）。
- T022 的代码和汇报（`pure/`、`clock/`、`durations.rs`）；T050 的 `driver/`。
- SP-4a 的会话 actor 原型（只作参考，不迁移）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。其他物料（`notes.md` 等）由执行者在任务分支上提交。
