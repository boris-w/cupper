# T140 Steer：宿主、驱动与 CLI

- 派发日期：2026-11-04
- 执行者：子 agent（Rust 工程师，宿主会话编排与驱动）
- 分支：`feat/T140-steer`
- 依赖：T111（已合并）。T149、T242 依赖本任务

## 背景

计划原来把 Steer 的宿主、驱动、CLI 和界面放在一个 L 任务里，理由是宿主侧不能没有使用者。开工前拆分（计划 v1.3.88）：本任务做宿主、驱动和 `cupper send --steer`。宿主侧由 CLI 用上，不是没人用的代码。界面另立 T242。

现在已有的部分：

- **协议**：
  - `SendMode::Steer` 和 `expected_turn_id`；
  - `Disposition::Steered`，`FallbackReason` 的六个值；
  - `origin: steer_new_turn`，用户消息的 `steered`；
  - 能力里的 `turn.steer {supported, side_effects}`；
  - `limits.steer_timeout_ms = 5000`；
  - `ErrorReason::Steer` 超时。
- **宿主**：
  - `driver::api` 的 `DriverCommand::Steer` 和 `SteerOutcome`；
  - `driver/acp.rs` 的 `SteeringRequest`（`_session/steering`）；
  - 两种 agent 的能力都从 `initialize` 的 `_meta.steering.supported` 读出。
  - 但 `driver/runtime` 把 `Steer` 命令直接丢掉，`TurnMachine` 把 `mode: steer` 一律按排队处理，并回 `fallback_reason: unsupported`。
- **假 agent**：已经有 `steer_ok`、`steer_new_turn`、`steer_hang` 三条脚本命令。
- **CLI**：没有 `--steer`。

## 范围

**做**（计划 T140 一行；设计 5.2.6"排队、Steer、中断"、5.5.3、5.6.2 的 `Steer`、5.6.9 的 K10 和 K11、5.6.10、NFR-7）：

1. **纯核心的 Steer 判定**（`TurnMachine`，不做 I/O，也不 await）。回合进行中收到 `mode: steer` 时：
   - **三项前提**：能力里有 `steer`；`expected_turn_id` 等于当前回合；当前回合处于 `Prompting`。不满足时直接转为排队，`fallback_reason` 分别是 `unsupported`、`turn_mismatch`、`not_prompting`，不发驱动命令。`Preparing`、`Autonomous`、`Cancelling` 都不算 `Prompting`（K11）。
   - **三项都满足时**：发驱动的 `Steer`，并经 `Clock` 设 5 s 的定时。
   - **驱动的结果**：
     - `Injected`：在当前回合写一条 `steered: true` 的用户消息，回 `disposition: steered` 和这个回合的 `turn_id`；
     - `StartedNewTurn`：开一个 `origin: steer_new_turn` 的回合，回 `steered` 和新回合的 `turn_id`。原回合怎么结束，以设计和 SP-1 s06 为准；
     - `PromptRequired`、`Unsupported`、超时、`AgentError`：转为排队，回 `queued`，并带对应的 `fallback_reason`。
   - **开工前定下的时序**（2026-11-04，执行者在 `notes.md` 里提出，`main` 同意）：
     1. 超时之后才到的应答：超时时已回的 `queued` 不再改。晚到的 `Injected`：排队的那一条还在队列里、目标回合也没完成，就把它取出，写成这个回合里 `steered: true` 的用户消息；已经出队的只记日志。晚到的 `StartedNewTurn`：还在队列里就取出，目标回合结束后开 `steer_new_turn` 回合。其余结果不处理。
     2. 等应答的 5 s 里回合结束或被中断：回合照常 Settle、拍结束快照，`turn.completed` 推迟到插话有结果之后（最多 5 s）；`Injected` 的消息写在它之前，其余结果转为排队；中断不取消在途的插话；适配器退出按 `agent_error`。
     3. 插话在途时又来一条 `steer`：转为排队，原因是附加的 `FallbackReason::steer_pending`（协议附加，补样例）。
   - 空闲时发送仍按"开新回合"处理，不受 `mode` 影响。
2. **驱动**：`driver/runtime` 执行 `DriverCommand::Steer`：
   - 发 `_session/steering`，总是带 `idleBehavior: promptRequired`；
   - 把应答映射成 `SteerOutcome`，错误按 `AgentError` 分类；
   - 遵守 5.6.10 的两条 SDK 约束（K27、K28）；ACP 回调里只做 channel 发送。
   - Claude 和 Codex 两种形态都要走通。
3. **CLI**：`cupper send <session> <text> --steer`：
   - 发 `mode: steer`，`expected_turn_id` 取 `session/get` 里正在运行的回合；
   - 输出 disposition，转为排队时一并输出原因；带 `--json` 时输出结构化结果。
   - IT-cli-commands 补上这一条。
4. **用例**（都已登记）：
   - **PROP-steer-only-prompting**：纯核心的性质测试，只有在 `Prompting` 并且回合对得上时，才会发出 `Steer`；
   - **IT-K10-steer-explicit-only**：Claude 形态。回合运行中用 `mode: queue` 发送，宿主从不发 steering；只有明确要求 `steer` 时才发；
   - **IT-K11-steer-idle-queued**：Codex 形态。回合不在 `Prompting` 时发 `steer`，结果是排队，驱动收不到 steering；
   - **IT-degrade-steering**：假 agent 关掉 steering 时，能力里是 `supported: false`，`steer` 转为排队，原因是 `unsupported`。
   - IT-degrade-plan、IT-degrade-thinking 在开工前拆到 T243（计划 v1.3.88）。
   - 另外覆盖 `Injected`、`StartedNewTurn`、`PromptRequired`、超时（`steer_hang`，时间经 `Clock` 推进）、出错这几条路径，上面三条时序，以及 `fallback_reason` 的七个值。
   - 假 agent 需要的开关（关掉 steering，以及 `prompt_required` 的应答）按现有脚本命令的写法补上。
5. 事件经会话 actor 写，每条流只有一个写者；时间只经 `Clock`。
6. 协议照 ADR-04，契约已冻结，只能附加，`schema-compat` 0 breaking。除 `steer_pending` 之外还要附加时，先告诉 `main`。

**不做**：
- 界面：Enter 默认插话、⌘Enter 排队，"已转为排队"的提示，偏好"跟进消息默认排队"，第一次插话时的副作用提示，"拒绝并附一条消息"的插话路径，E2E-composer-steer-default。这些都归 T242。
- 计划与思考的能力降级（IT-degrade-plan、IT-degrade-thinking）：归 T243。
- `session/queue_edit`：归 T149。

## 验收标准

- [ ] 上面四个编号的用例、IT-cli-commands 的新增一条都通过。IT 在 runner 上连跑 10 次，由 `main` 触发 test-repeat。
- [ ] 改动前后，`mode: queue` 的行为不变。
- [ ] `schema-compat` 0 breaking；trace-ids 检查通过。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：650；硬停止线：880（2026-11-04 开工前估约 760，拆出 T243 后约 630；原为 600、700）。做到 +730 −64 时越过 720 停下报告，余下的 CLI `--steer` 和三个进程级用例约 140 行，不再拆，硬停止线由 720 放到 880；测试只按可读性整理（表驱动、共用助手），不为压行数合并断言。

## 参考资料

- 设计：
  - 5.2.6"排队、Steer、中断"和回合状态机；5.2.16（纯核心与 `Clock`）；
  - 5.5.3 时序图；
  - 5.6.2 `Steer`；5.6.7 能力缓存；5.6.9 K9–K11；5.6.10；
  - 5.3.4 `session/send`；5.3.11 `fallback_reason`；
  - 5.2.14 CLI 命令表。
- 决策和待定问题：ADR-07（v1.4 修订）、Q9。
- 卷宗：T003（SP-1 s06 的实测，steering 的位置）、T048（`TurnMachine`）、T111（Codex 形态）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 子进程一律经 `cupper-sys::spawn`；时间经 `Clock`；每条事件流只有一个写者。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
