# T041 汇报

- 执行者：子 agent（Rust 工程师，宿主编排）
- 分支：`feat/T041-autonomous-coalescer`（worktree `.claude/worktrees/T041`，工作区干净）
- 完成日期：2026-10-19
- 范围：按计划 v1.3.8 收窄后的范围。`AutonomousJudge`、组合测试环境、PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable 已划给 T058。

## 做了什么

- **与回合机的衔接**（`orchestrator/turn.rs` +42/−3）：
  - `Input::HoldQueue(bool)`：裁判（T058）用它做出队保护。保持期间，空闲的会话不出队；空闲时用户发的消息照常立即开回合，按裁定只保护自动出队。
  - `TurnSignal { Started, Settled { interrupted }, Completed(TurnEnd) }` 和 `TurnSignal::of(&Action)`：合并器和裁判靠它跟随回合。`interrupted` 取自 `Settle.asks == turn_cancelled`。
  - `Action::Complete` 的文档改为"由合并器写 `turn.completed`"。
- **`durations.rs`**：登记了三个时长：`item_coalesce`（40 ms）、`autonomous_silence`（10 s）、`dequeue_guard`（30 s）。后两个给 T058 用。
- **`orchestrator/grouping.rs`**：`TurnItems` 在条目开始时分配 `group_id`，并维护最终回答（5.3.5）。
  - 连续的工具调用共用一组，组内的思考留在组里；其他条目和交互请求结束当前组。
  - 最终回答是最后一个过程条目之后的全部 `agent_message`；回合里没有过程条目时，取全部 `agent_message`。
- **`orchestrator/coalescer.rs`**：`Coalescer` 纯核心，从 SP-4a 的 `normalize.rs` 迁移，输出新的条目模型。
  - **写入时合并**：
    - 一个 field 的第一个片段立即写出；
    - 之后的片段从第一段缓冲起最多等 40 ms，期间不后移；
    - 写其他事件之前、delta 写满时、定时器到点时都会刷出；
    - 只合并同一条目的同一 field。
  - **条目**：
    - 消息和思考按 ACP 的 message ID 续写；
    - 工具调用支持开始、改名、输出追加（第一次追加时先 `output_state: streaming`）、整段替换（`complete`）和结束（带退出码）；
    - 计划在一个回合内只有一个条目，之后用更新改写；
    - compaction 写成提示条目。
    - `extraction` 按 `missing_required(false)` 给初值。
  - **回合**：
    - 没有打开的回合时，或者回合已经 Settle 时，到达的更新先暂存，下一个回合开始时再写入；
    - Settle 时，文本和计划条目标 `completed`，工具调用标 `abandoned`；被中断的回合里，如果中断停不了命令，`execute` 类工具再加 `may_still_run`；
    - `turn.completed` 由合并器写出，并带上 `final_item_ids`。
  - **晚到输出**：被放弃的工具调用后来又有更新时，在当时的回合里建 `late_tool_output` 条目，`ref` 自包含：原条目 ID、所在回合、标题、命令。每个原工具在每个回合里只有一个晚到条目；原条目的事件不改。
- **`orchestrator/mod.rs`**：模块文档补上了合并器和分组。
- **`notes.md`**：开工前的估计、逐模块统计、停工分析与拆分建议、PROP 的设计、变异检验。

## 关键取舍

- **`now` 和输出放在核心里**：每一步的 `now` 和输出缓冲作为核心的字段，`step` 结束时取走。这样内部函数少了两个参数，签名能放在一行。核心仍然不做 I/O、不读时钟。
- **合并器只有一个缓冲**：同一时刻只缓冲一个条目的一个 field。换条目、换 field，或写任何其他事件之前，都先刷出。这样日志的先后次序和逐条写入完全一致，PROP 才能直接比较折叠后的日志。
- **PROP-coalescer 用两份日志对比**：
  - 同一串随机输入同时送给两个合并器：`a` 正常合并；`b` 在每个输入后都收到一次到点的定时器，相当于逐条写入。
  - 两者用同一种子的 `IdGen` 和同样的虚拟时间，条目 ID 和组 ID 一致。
  - 每一步检查：`a` 的日志加上还在等待的文本，按 `item.delta` 的 `Mergeable` 规则折叠后，与 `b` 的折叠结果相同。这一条同时覆盖了次序、各条目的文本和不跨条目合并。
  - 交互请求之后没有等待的文本，两份日志在同一位置插入标记事件。
  - 等待中的文本，flush 截止时间恰好是第一段缓冲后 40 ms，到点时写出。
  - 结束时，每个 delta 转义后不超过 `MAX_DELTA_ESCAPED`。
- **变异检验**：手工改坏 9 处。
  - 抓到 7 处：不先刷出、不设定时器、到点不刷出、窗口 80 ms、窗口随追加后移、交互请求不刷出、不切分。
  - "不同条目合并在一起"第一版漏掉了：只有两个工具的输出交错时才会出现。我提高了工具输出的权重，让它集中到最近三个调用上，之后连续 3 次都抓到，其余变异体仍全部抓到。
  - "同一条目的不同 field 合并"是等价变异：一个条目只会有一种 field 的 delta。
- **场景测试** `coalescer_settles_open_items_and_writes_late_output_in_the_next_turn`：PROP 只比较日志，不检查语义，所以单独用这个场景覆盖：`abandoned` 加 `may_still_run`、Ending 期间暂存、晚到条目的 `ref` 与退出码、`final_item_ids` 不计入晚到输出。
- 设计决定已由编排者认可，写在 5.3.11 末尾"T041 实现时的裁定"。

## 验证方式

在干净的 worktree 里运行 `cargo xtask ci pr --record docs/tasks/T041-autonomous-coalescer/ci/`，通过（提交 `df9cc81`）：
- rust 995 项；
- protocol 176 项；
- ts 232 项；
- license 1 项。

全部通过，0 失败。摘要 `ci/pr-df9cc81d87c5.json` 已提交，我检查过，没有用户名和本机路径。`cupper-host` 的 clippy `-D warnings` 在全特性、无特性、`test-harness`、`dev-auth` 四种组合下都通过。

对照验收标准（收窄后）：
- [x] **PROP-coalescer**：合并后应用等于逐条应用；其他事件到达时先刷出；定时器到点刷出；不跨条目合并。CI 按编号识别。
- [x] **UNIT-grouping**：工具调用共组，组内思考留在组里；其他条目和请求结束当前组。
- [x] **UNIT-final-items**：最终回答是最后一个过程条目之后的消息；没有过程条目时取全部消息。
- [x] 全部用 `CoreDriver` 在虚拟时间上驱动，性质测试用 proptest，不睡眠。
- [x] 在干净的 worktree 里跑 CI 通过，摘要随任务提交。
- [x] 测试名以编号的 snake_case 开头：`prop_coalescer_…`、`unit_grouping_…`、`unit_final_items_…`。

**规模**：相对 main 共 1,131 行，不含 `docs/`，在约 1,500 行的上限内。
- 实现约 690 行：`coalescer.rs` 569、`grouping.rs` 60、`turn.rs` +42/−3、`durations.rs` 9、`mod.rs` 6。
- 测试约 440 行：`coalescer.rs` 376、`grouping.rs` 66。

## 提交列表

```
06a3ddc docs(task): record the T041 pr CI summary
df9cc81 docs(task): T041 narrowed scope, final line counts and the mutation check
ced86d3 test(host): PROP-coalescer and a settle and late output scenario
c8d7cf9 docs(host): name the coalescer and grouping in the orchestrator's docs
6333f7e Merge branch 'main' into feat/T041-autonomous-coalescer
bf4d9a7 docs(task): T041 line counts, the projected overrun and a split proposal
3a25282 feat(host): add Coalescer, writing agent updates into the turn as items
e2f65f5 feat(host): activity groups and the final answer of a turn
61b4d2b feat(host): hold the queue and signal turns to the other session cores
1611c00 docs(task): T041 line estimates per module
```

## 遗留问题 / 范围外发现

- **T058**：
  - 裁判按 5.3.11"T041 实现时的裁定"实现：
    - 收到 Ending 期间的输出时，先 `HoldQueue(true)`；`turn.completed` 写出后开自发回合；
    - 结束条件和 30 s 出队保护按裁定执行。
  - 合并器已经能暂存 Ending 期间的更新，也能为被放弃的工具建晚到条目。
  - 组合测试环境里，回合机的每个动作先转 `TurnSignal` 给合并器，再给裁判；裁判的 `Start`、`End`、`Hold` 在回合机当前这一步之后，作为输入回送给回合机。
- **T043**：
  - 用 `TurnSignal::of` 把回合机的动作转给合并器；`turn.completed` 由合并器写出，actor 收到 `Action::Complete` 时自己不写。
  - 交互代理写 `interaction.requested` 之前，先给合并器送 `Input::Interject`：刷出缓冲的文本，并结束活动组。
  - 构造合并器时传入能力里的 `interrupt_stops_commands`。
  - 合并器只写内联文本，转 blob 和 `truncated` 归 T043，交接事项里已有。
- **T053**：`NormUpdate::ToolStart` 没有 `cwd`，`execute` 类工具的 `extraction` 因此总是 `partial`。交接事项里已有。
- `Coalescer` 的 `tools` 表在会话期间不清理已完成的调用，以便识别重复的更新。长会话里它会一直增长，每项只有几十字节，目前不是问题。actor 空闲退出时会连同核心一起丢掉。

## 附：中途汇报（拆分前）

- 执行者：子 agent（Rust 工程师，宿主编排）
- 分支：`feat/T041-autonomous-coalescer`（worktree `.claude/worktrees/T041`，工作区干净）
- 日期：2026-10-19
- 状态：**没有做完，停工等编排者决定。** 改动累计 751 行（不含 `docs/`），全部做完预计约 1,600–1,670 行，比约 1,500 行的上限多 7%–11%。

## 做了什么

- **衔接回合机**（`orchestrator/turn.rs` +42/−3）：
  - 新增 `Input::HoldQueue(bool)`，供出队保护使用。保持期间，空闲的会话不出队；空闲时用户发的消息照常立即开回合，因为 5.2.6 只说"出队"。
  - 新增 `TurnSignal { Started, Settled { interrupted }, Completed(TurnEnd) }` 和 `TurnSignal::of(&Action)`。合并器和裁判通过它跟踪回合的开始、Settle 和结束；T043 收到回合机的动作后转发即可。
- **`durations.rs`**：登记了 `item_coalesce`（40 ms）、`autonomous_silence`（10 s）、`dequeue_guard`（30 s）。
- **`orchestrator/grouping.rs`，已完成并测试**：`TurnItems` 负责给每个新条目分配 `group_id`，并维护最终回答（5.3.5）。UNIT-grouping 和 UNIT-final-items 共 4 个用例，全部通过。
- **`orchestrator/coalescer.rs`，实现已提交，PROP-coalescer 还没写**：从 SP-4a 的 `normalize.rs` 迁移，改成纯核心，输出新的条目模型。
  - 文本在写入时合并：第一个片段立即写出，之后等 40 ms；遇到其他事件、delta 写满或定时器到点时提前写出。只合并同一条目的同一 field，所以不会跨条目合并。
  - 条目带 `group_id`。`turn.completed` 由合并器写出，并补上 `final_item_ids`。
  - 没有打开的回合时（包括回合已在收尾），收到的更新先暂存，等下一个回合开始时放进去。
  - Settle 时收尾：
    - 文本条目和计划条目标为 `completed`；
    - 工具调用标为 `abandoned`；被中断的回合里，如果中断停不了命令（Codex），`execute` 类工具再加 `may_still_run`；
    - 被放弃的工具后来又有更新，就在当时的回合里建一个 `late_tool_output` 条目，`ref` 指向原条目，原回合不改。
  - 也处理了工具输出的追加和整段替换、计划的 upsert，以及 compaction 提示。
- 每个提交都通过了 `cargo test -p cupper-host`（T048 的四条 PROP 不受影响），clippy 在全特性、无特性、`test-harness`、`dev-auth` 四种组合下都没有警告。

## 为什么超限

`coalescer.rs` 估 330 行，实际写了 569 行，原因有两个：
- rustfmt 的默认设置（`struct_lit_width` 为 18）会把稍长一点的结构体字面量都展开成多行。协议类型字段多：`ToolCall` 16 个，`TurnCompleted` 8 个，`LateToolOutput` 7 个。
- 我低估了范围。新条目模型下，工具调用的全部更新、计划、compaction 提示、Settle 收尾和晚到输出都得由合并器写出来，不只是合并文本。

我把每一步的 `now` 和输出放进了核心本身，函数签名都压成一行，从 606 行降到 569 行。再压就只能改用元组结构体之类的写法，可读性会变差。

按同样的放大比例重估剩下的部分：

| 剩余部分 | 重估行数 |
|---|---|
| `AutonomousJudge` 实现 | 约 230 |
| PROP-coalescer | 约 170 |
| 自发回合的场景测试（可以省） | 约 70 |
| 组合测试环境，加 PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable | 约 450 |

## 拆分建议（请编排者决定）

- **T041（本分支）**：
  - 回合机的衔接；
  - `grouping.rs`，已完成；
  - `Coalescer` 加 PROP-coalescer：用"合并后"和"逐条写入"两份日志按 `Mergeable` 折叠后比较，检查四条：合并后应用等于逐条应用、遇到其他事件先刷出、定时器到点刷出、不跨条目合并。
  - 合计约 920 行。
- **T041b（新任务）**：
  - `AutonomousJudge`（Q10、30 s 出队保护、Ending 期间的规则）；
  - 组合测试环境：把回合机、裁判、合并器按 T043 的接线组合成一个测试用核心，同时作为 T043 接线的参考；
  - PROP-autonomous-dequeue-guard 和 PROP-turn-snapshot-immutable。后者要靠裁判为晚到输出开自发回合，所以跟裁判放在一起。
  - 约 700–750 行。
- T043 同时依赖这两个任务。

**不拆**：最终约 1,600 行，多出来的主要是测试。

两种方案我都可以在同一个 worktree 里继续做。如果拆分，T041b 需要另开分支，请编排者指定。

## 已定的设计决定（拆分后也适用，汇报和 notes 里都会写明）

- **Ending 期间到达的更新**（裁判部分尚未实现）：
  - 合并器先暂存这些更新；
  - 裁判在 Ending 期间看到 agent 的输出，就先 `HoldQueue(true)`，等这个回合的 `turn.completed` 写出后，立即开一个自发回合承载它们，排在任何排队消息之前。
  - 理由：prompt 已经返回，这些更新是 agent 自发的工作（K9）。放进正在收尾的回合，会破坏"Settle 之后不再有新条目"和快照不可变；放进出队的回合，又会在 agent 还在工作时发出新 prompt（K1）。
- **自发回合何时结束**（计划在裁判里实现）：没有运行中的工具、也没有待处理请求时，以下任一情况发生就结束：
  - 收到带 `cost` 的 `usage_update`；
  - thread 变为 idle；
  - thread 已经是 idle 时工具结束（对应晚到输出）；
  - 静默 10 s，但 thread 已知在运行时不算。
  - 只有静默结束的那一种会保持队列：agent 再静默 30 s 才放开。用户在空闲时发起回合，也会解除保持。
- **晚到条目跨回合**：晚到条目所在的回合先结束时，这个条目标为 `abandoned`，之后的输出在下一个回合里另建一个晚到条目。"每个原工具只有一个"按回合理解，否则就得往已经写下快照的回合里追加。

## 提交列表

```
bf4d9a7 docs(task): T041 line counts, the projected overrun and a split proposal
3a25282 feat(host): add Coalescer, writing agent updates into the turn as items
e2f65f5 feat(host): activity groups and the final answer of a turn
61b4d2b feat(host): hold the queue and signal turns to the other session cores
1611c00 docs(task): T041 line estimates per module
```

## 遗留问题 / 范围外发现

- **T053**：`NormUpdate::ToolStart` 没有 `cwd`，合并器按 `missing_required` 给 `extraction` 初值，`execute` 类工具因此总是 `partial`。提取层需要给出 `cwd`。
- **T043、T053**：输出超过内联上限时转 blob、`truncated` 状态和每秒更新尾部，都需要 I/O，合并器目前没做。`Replace` 一律写成内联文本。需要指定由谁来做。
- **T042 合并时**：
  - T042 改了 `DriverEvent::Ask`（`request` 变为 `AskRequest`）。我的代码和测试不构造 Ask，所以不受影响。
  - 两边都在 `orchestrator/mod.rs` 里加了 `mod` 行，会有一处相邻行冲突，解决起来很简单。
  - 我改了 `turn.rs`，T042 的分支没有改它。
- **出队保护的范围**：设计只要求保护出队，空闲时用户直接发的消息不受保护。它在 Claude 自发工作时同样可能撞上 K1。是否扩大保护范围，请编排者裁定。
