# T140 汇报

- 执行者：子 agent（Rust 工程师，宿主会话编排与驱动）
- 分支：`feat/T140-steer`
- 完成日期：2026-11-04

## 做了什么

1. **协议**：附加 `FallbackReason::steer_pending`，补了 `session/send` 的样例，已重新 gen。schema-compat 为 additive，0 breaking。
2. **驱动**（`driver/api.rs`、`driver/runtime/mod.rs`）：
   - runtime 执行 `DriverCommand::Steer`：内容按 prompt 的规则转成 ACP 块，发 `_session/steering`，固定带 `_meta.steering.idleBehavior: promptRequired`。
   - 应答映射成 `SteerOutcome`：`injected`、`startedNewTurn`、`promptRequired`；不认识的值按 `Unsupported`。结果以 `DriverEvent::SteerDone` / `SteerFailed` 交给会话，错误按 agent 分类（`judged`）。
   - 请求经 SDK 的 `on_receiving_result` 回调等应答，不丢弃在途请求（K28）；回调里只做 channel 发送。
   - Claude 和 Codex 走同一路径。
3. **纯核心**（`orchestrator/turn.rs`，不做 I/O，也不 await）：
   - **前提**：`steer` 依次判"能力里有 steer"（attach 时从 `turn.steer.supported` 记下）、"`expected_turn` 等于当前回合"、"处于 `Prompting`"、"没有插话在途"。不满足就排队，原因分别是 `unsupported`、`turn_mismatch`、`not_prompting`、`steer_pending`，不发驱动命令。`Preparing`、`Cancelling`、`Autonomous`、`Ending` 都不算 `Prompting`。空闲时发送照旧开新回合，不受 mode 影响。
   - **满足时**：发 `Steer`，经 Clock 设 5 s 定时（新时长键 `steer_timeout`，取自 `limits.steer_timeout_ms`）。回复延后到有结果时，用 actor 已有的延后回复机制。
   - **结果**：
     - `Injected`：在当前回合写一条 `steered: true` 的用户消息，回复 `steered` 和回合 ID；
     - `StartedNewTurn`：原回合照常等自己的 PromptDone；完成后先开 `origin: steer_new_turn` 的回合，再出队。新回合里有一条 steered 的用户消息，没有 prompt，由 AutonomousJudge 按 autonomous 判定结束。回复是 `steered` 和新回合 ID；
     - `PromptRequired`、`Unsupported`、`SteerFailed`、超时：排队，原因是 `prompt_required`、`unsupported`、`agent_error`、`timeout`。
   - **时序**（按你定的方案）：
     - 回合在插话在途时结束：照常 Settle、拍结束快照，`turn.completed` 推迟到插话有结果之后，最多 5 s；
     - 中断不取消在途的插话；
     - 适配器退出和 HostExit 都按 `agent_error` 当场转为排队（HostExit 同时丢掉待开的 steer 回合）；
     - 超时之后晚到的 `Injected`：那一条还在队列里、目标回合也还没完成，就把它从队列取出（`queue.updated`），写成目标回合里 steered 的用户消息；晚到的 `StartedNewTurn` 取出后开 `steer_new_turn` 回合（不回复）；已经出队的只记日志（`turn::Action::Log`，actor 用 tracing 写出）。
   - `QueueCore::take` 按 `queue_id` 取出一条。AutonomousJudge 对 `steer_new_turn` 与 `autonomous` 同样处理。
4. **actor**：传 `expected_turn_id`，映射 `steered` 和七个原因。`mode: queue` 的路径没有改动。
5. **CLI**：`cupper send <session> <text> --steer`。发 `mode: steer`，`expected_turn_id` 取会话视图里正在运行的回合（见"设计偏差"）。输出 disposition，转为排队时附上原因；`--json` 输出 `SessionSendResult`。
6. **假 agent**：`--fault no_steering`（`initialize` 里 `steering.supported: false`），以及脚本命令 `steer_prompt_required`、`steer_error`。
7. **notes.md**：开工前的估算、时序方案、你的几次决定（拆出 T243、`steer_pending`、硬停止线放到 880）。

## 关键取舍

- **结果走驱动事件，不另开回复的 future**：每个会话同时最多一个插话在途，结果不需要对号，晚到的按"晚到"判；actor 也不用再开一个 channel。适配器退出时由核心在 `AdapterExited` 时判为 `agent_error`，不依赖 SDK 是否回错误。
- **`StartedNewTurn` 不替原回合判定结束**：原回合以它自己的 PromptDone 结束。按 SP-1 s06，只有 agent 已经空闲时才会出现 `startedNewTurn`，原回合的响应马上就到。
- **IT-K11 的"不在 Prompting"取 `Cancelling`**：`hang` 加中断，再把中断兜底设为 1 s，结果是确定的；另外补一步：空闲时的 `steer` 开普通回合。两步都不发 steering。用自发回合做的话，Codex 的线程状态会让它提前结束，结果不稳定。

## 验证方式

- **单元测试**（turn.rs，4 个）：
  - 前提和七个原因；
  - Injected；
  - prompt 返回之后 `turn.completed` 等插话结果；
  - 中断之后等到插话超时；
  - StartedNewTurn 开新回合，并按 autonomous 结束；
  - 晚到的 Injected 取出排队消息；已出队的只记日志；
  - 晚到的 StartedNewTurn；
  - 适配器退出。
- **PROP-steer-only-prompting**（props.rs，随机的插话加上按时或晚到的应答）断言两点：只在 `Prompting`、回合对得上、没有插话在途时才发 `Steer`；在途期间不写 `turn.completed`。我分别把两条规则改坏试过，属性测试都会失败。原有四个属性测试照常通过，`mode: queue` 的行为不变。
- **本机检查**（都是单份）：
  - host lib 525 个通过、3 个忽略；
  - driver_k 21 个全部通过；lifecycle 的 it_cli_commands 通过；
  - cupper-cli 27 个、cupper-protocol 100 个通过；
  - fmt、clippy（-D warnings）、trace-ids（451 个编号）通过。
- **验收标准**：
  - [x] PROP-steer-only-prompting、IT-K10-steer-explicit-only、IT-K11-steer-idle-queued、IT-degrade-steering、IT-cli-commands 的新增一条都通过。runner 上各连跑 10 次 × 4 份（macos-15），日志里的通过数都是 40：
    - `--test driver_k`，filter `steer`（三个 IT）：https://github.com/boris-w/cupper/actions/runs/41809879467 ，40 passed, 0 failed；
    - `--lib`，filter `prop_steer_only_prompting`：https://github.com/boris-w/cupper/actions/runs/41809913125 ，40 passed, 0 failed；
    - `--test lifecycle`，filter `it_cli_commands`：https://github.com/boris-w/cupper/actions/runs/41810044357 ，40 passed, 0 failed。artifact 里的 binaries.txt 是 lifecycle 测试二进制，40 次都 ok。
  - [x] `mode: queue` 改动前后行为不变：原有的 turn、props、session 测试全部通过。
  - [x] schema-compat 0 breaking（additive）；trace-ids 通过。
  - [x] PR #82 https://github.com/boris-w/cupper/pull/82 的 8 个检查全部通过，ci 运行在 https://github.com/boris-w/cupper/actions/runs/41809862898 （5350069f）。
- **规模**：`cargo xtask size` 为 +862 −73，在放宽后的硬停止线 880 以内。

## 提交列表

```
5350069f test(host): IT-K10-steer-explicit-only, IT-K11-steer-idle-queued, IT-degrade-steering
5f6271d2 feat(cli): cupper send --steer
7aeafe81 test(orchestrator): PROP-steer-only-prompting
450eb017 feat(orchestrator): steer the prompting turn, or queue with the reason
94ac8071 feat(driver): steer through _session/steering
4493545a test(fake-agent): turn steering off, and answer steers prompt_required or with an error
56bb0ff3 feat(protocol): the steer_pending fallback reason
523fdf24 docs(T140): the estimate, the steer's timing, and what main decided
a99a5f8e Merge main into feat/T140-steer
```

## 设计偏差 / 补充

- **5.2.6 "排队、Steer、中断"**：在 `mode: steer` 那一行补上：
  - 同一会话同时只有一个插话在途，再来的 `steer` 排队，原因 `steer_pending`；
  - 插话在途时回合结束，`turn.completed` 推迟到插话有结果之后（最多 5 s），中断不取消插话；
  - 适配器退出、宿主强制退出按 `agent_error`；
  - 晚到应答的处理：取出还在队列里的那一条；已出队的只记日志；
  - `StartedNewTurn` 在原回合完成后开 `steer_new_turn` 回合，没有 prompt，由自发回合的规则结束。
- **5.6.2 `Steer`**：结果不是"请求的回复"，而是 `DriverEvent::SteerDone` / `SteerFailed`，和 `Prompt` 一致；5 s 由会话侧计时。
- **5.2.14 CLI 命令表**：`send --steer` 的 `expected_turn_id` 取会话视图里正在运行的回合（`SessionView::running_turn`），因为 `session/get` 的结果没有回合 ID。这一处你已同意。
- **03 协议文档**：`fallback_reason` 加上 `steer_pending`。
- **时长**：新增键 `steer_timeout`（5 s，取自 `limits.steer_timeout_ms`）。

## 遗留问题 / 范围外发现

- **原回合的 prompt 一直不返回**：如果 `StartedNewTurn` 之后原回合的 prompt 迟迟不返回（K1 那类挂起），这条 `steer` 的回复就一直等着。概率很低：`startedNewTurn` 只出现在 agent 已经空闲时。
- **新回合的早期输出归到原回合**：`StartedNewTurn` 之后、原回合的 PromptDone 到达之前，agent 新回合的输出会记在原回合里。这是现有的"回合运行时的输出属于它"规则。
- **适配器在开新回合之前退出**：待开的 steer 回合照样会开，然后按 10 s 静默结束。
- **范围外**：界面部分（Enter 默认插话、"已转为排队"的提示等）在 T242；`session/queue_edit` 的插话仍回 `unsupported`，在 T149；计划和思考的能力降级在 T243。
