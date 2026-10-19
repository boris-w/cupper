# T041 执行笔记

## 开工前的行数估计

按模块估，不含 `Cargo.lock` 和 `docs/`。任务书的上限约 1,500 行。

| 模块 | 内容 | 估计 |
|---|---|---|
| `durations.rs` | 登记 40 ms 合并窗口、10 s 静默、30 s 出队保护 | 10 |
| `orchestrator/turn.rs` | `Input::HoldQueue`（出队保护）；`TurnSignal`（其他核心从回合机的动作里得知回合的开始、Settle、结束） | 40 |
| `orchestrator/grouping.rs` | 一个回合的活动组与最终回答（`group_id`、`final_item_ids`） | 60 |
| `orchestrator/coalescer.rs` | `Coalescer`：`NormUpdate` 写成新条目模型的事件；40 ms 写入合并；回合外暂存；Settle 时收尾（`abandoned`、`may_still_run`）；晚到输出；写 `turn.completed` | 330 |
| `orchestrator/autonomous.rs` | `AutonomousJudge`：自发回合的开始、结束、出队保护；Ending 期间的更新 | 190 |
| 实现小计 | | 630 |
| 测试：`grouping.rs` | UNIT-grouping、UNIT-final-items | 90 |
| 测试：`coalescer.rs` | PROP-coalescer（合并后与逐条写入的对比） | 140 |
| 测试：`autonomous.rs` | 场景：静默结束与 30 s 保护、Ending 期间的更新 | 60 |
| 测试：`session_props.rs` | 三个核心按 T043 的接线组合成一个测试核心，加 agent 和端口的模型；PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable | 370 |
| 测试小计 | | 660 |
| **合计** | | **约 1,290** |

余量约 200 行。性质测试的环境最难估，写完它就统计一次；预计超过 1,500 行时停工汇报。

## 进度统计

按与分叉点的 `git diff --numstat` 累计，不含 `Cargo.lock` 和 `docs/`。

| 提交 | 内容 | 累计 |
|---|---|---|
| 回合机的衔接 | `HoldQueue`、`TurnSignal`、三个时长（turn.rs +42/−3，durations.rs +9） | 54 |
| `grouping.rs` | `TurnItems` 与 UNIT-grouping、UNIT-final-items（实现 60，测试 66） | 181 |
| `coalescer.rs` | `Coalescer` 实现，还没有 PROP-coalescer | 751 |
| 合入 main 之后 | 以下改按与 main 的差计（合并提交 `6333f7e` 带进来的 T042、T027 等不算） | 751 |
| 模块文档 | `orchestrator/mod.rs` 补上合并器和分组 | 755 |
| PROP-coalescer | 性质测试和一个 Settle 与晚到输出的场景测试（测试 376） | 1,131 |

## 停工：预计超出上限（守则第 8 条）

`coalescer.rs` 估 330 行，实际 569 行（注释和空行约 60 行）。超出主要有两个来源：
- rustfmt 默认的 `struct_lit_width` 是 18，稍长一点的结构体字面量就展开成多行。协议类型字段多（`ToolCall` 16 个、`TurnCompleted` 8 个、`LateToolOutput` 7 个），每处都要一整块。
- 我低估了范围：除了合并文本，还要做工具调用的全部更新（输出追加与整段替换、`streaming` 状态、结束、退出码）、计划的 upsert、compaction 提示、按 Settle 收尾，以及晚到输出。这些都要用新的条目模型写出来。

我已经把每一步的 `now` 和输出放进核心本身，函数签名都压到一行，从 606 行降到 569 行。再压就要改用元组结构体之类的写法，可读性会变差。

按同样的放大比例，重新估剩下的部分：

| 剩余部分 | 原估计 | 重估 |
|---|---|---|
| `AutonomousJudge` 实现 | 190 | 约 230 |
| PROP-coalescer | 140 | 约 170 |
| 自发回合的场景测试 | 60 | 约 70（可以省掉，由组合性质测试覆盖） |
| 组合测试环境，加 PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable | 370 | 约 450 |
| 合计 | 760 | 约 850–920 |

全部做完约 1,600–1,670 行，超过约 1,500 行的上限 7%–11%。所以在这里停下，由编排者决定。

### 拆分建议

- **T041（本分支）**：回合机的衔接（`HoldQueue`、`TurnSignal`）、`grouping.rs`（UNIT-grouping、UNIT-final-items，已完成）、`Coalescer` 加 PROP-coalescer。合计约 920 行。
- **T041b（新任务）**：`AutonomousJudge`；组合测试环境，把回合机、裁判和合并器按 T043 的接线组合成一个测试用核心；PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable。约 700–750 行。
  - PROP-turn-snapshot-immutable 要靠裁判为晚到输出开自发回合，所以和裁判放在一起。
  - T043 同时依赖 T041 和 T041b。

如果不拆，我估计最终约 1,600 行，主要多在测试上。

## 设计决定

编排者已全部认可，写进了 `03-protocol-v1.md` 5.3.11 末尾"T041 实现时的裁定"。出队保护的范围维持设计原意：只保护自动出队。

- **出队保护放在回合机里**：新增 `Input::HoldQueue(bool)`。保持期间，空闲的会话不出队；空闲时用户发的消息照常立即开回合，并解除保护（5.2.6 只说"出队"）。由裁判发出，回合机只负责照办。
- **`TurnSignal`**：从回合机的动作里读出回合的开始、Settle 和结束，交给合并器和裁判。T043 用 `TurnSignal::of(&action)` 转发即可。
- **`turn.completed` 由合并器写**：`Action::Complete(TurnEnd)` 经 `TurnSignal::Completed` 交给合并器，合并器补上 `final_item_ids` 后写出。
- **Ending 期间到达的更新**（裁判部分在 T058 实现）：
  - 合并器先暂存，回合结束后放进下一个回合；
  - 裁判见到 Ending 期间的输出，就先保持队列，等这个回合的 `turn.completed` 写出后，立即开一个自发回合承载这些更新，排在任何排队消息之前；
  - 理由：prompt 已经返回，这些是 agent 自发的工作（K9）。放进正在收尾的回合，会破坏"Settle 之后不再有新条目"；放进出队的回合，又会在 agent 还在工作时发出新 prompt（K1）。
- **晚到输出跨回合**：晚到输出的条目所在的回合如果在条目结束前就结束了，这个条目照常标为 `abandoned`。之后的输出在下一个回合里另建一个晚到条目。"每个原工具只有一个"按回合理解，否则就要往已经写下快照的回合里追加。
- **大输出**：合并器把 `Replace` 写成内联的 `Content::Text`。超过内联上限时转 blob、`truncated` 状态和每秒更新尾部，都需要 I/O，不在本任务范围；在汇报里提给 T043、T053。
- **`extraction`**：合并器按 `missing_required(false)` 给出初值。`NormUpdate::ToolStart` 没有 `cwd`，`execute` 类工具因此总是 `partial`；在汇报里提给 T053。

## 收窄后的范围（计划 v1.3.8）

编排者裁定拆分：本任务只做与回合机的衔接（`HoldQueue`、`TurnSignal`）、`grouping.rs`、`Coalescer` 和 PROP-coalescer。验收是 PROP-coalescer、UNIT-grouping、UNIT-final-items。`AutonomousJudge`、组合测试环境、PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable 另立为 T058。

**最终行数**：相对 main 共 1,131 行，不含 `docs/`。
- 实现约 690 行：`coalescer.rs` 569、`grouping.rs` 60、`turn.rs` +42/−3、`durations.rs` 9、`mod.rs` 6。
- 测试约 440 行：`coalescer.rs` 376、`grouping.rs` 66。

## PROP-coalescer 的设计

- 同一串随机输入同时送给两个合并器，各自记一份日志：
  - `a` 正常合并；
  - `b` 在每个输入之后都收到一次到点的定时器，因此每个片段立即写出，相当于逐条写入。
  - 两者用同一个种子的 `IdGen` 和同样推进的虚拟时间，条目 ID 和组 ID 一致。
- 随机输入覆盖：
  - 消息片段（3 个消息 ID 加无 ID）和思考；
  - 工具调用的开始（包括已有调用的再次开始）、输出追加、整段替换和结束；
  - 计划；
  - 交互请求，此后两份日志都追加一个标记事件；
  - Settle（中断或不中断）、下一个回合，Ending 期间的更新会被暂存；Settle 之后的工具输出走晚到输出；
  - 推进时间：0–40 ms 或 40–100 ms。
- 文本有时很长（转义后可达 24,000 字节），用来覆盖 delta 写满后的切分。工具输出集中在最近三个调用上，这样不同条目的输出会交错出现。
- 每一步之后检查：
  - **合并后应用等于逐条应用**：`a` 的日志加上还在等待的文本，按 `item.delta` 的 `Mergeable` 规则折叠后，与 `b` 的折叠结果完全相同。事件的先后、每个条目的文本都要一致；把文本写进别的条目，会在这里被发现（不跨条目合并）。
  - **其他事件到达时先刷出**：交互请求之后，没有等待的文本；标记事件夹在两份日志的同一位置，次序不对也会被上一条发现。
  - **定时器到点刷出**：有文本在等待时，flush 定时器的截止时间恰好是第一段缓冲文本到达后 40 ms，等待期间不后移。推进时间时写出的 delta 恰好发生在这个截止时间。
  - 结束时没有等待的文本，每个 delta 转义后不超过 `MAX_DELTA_ESCAPED`。

### 变异检验

手工改坏 8 处，逐个运行性质测试：

| 变异 | 结果 |
|---|---|
| 写其他事件前不刷出 | 抓到 |
| 不设定时器 | 抓到 |
| 定时器到期时不刷出 | 抓到 |
| 窗口改成 80 ms | 抓到 |
| 每次追加都把窗口后移 | 抓到 |
| 交互请求不刷出 | 抓到 |
| 不按 `MAX_DELTA_ESCAPED` 切分 | 抓到 |
| 不同条目的同一 field 合并在一起 | 第一版生成器漏掉了 |
| 同一条目的不同 field 合并在一起 | 抓不到，是等价变异：一个条目只会有一种 field 的 delta |

- "不同条目合并"只在两个工具的输出交错、中间没有别的事件时才会出现，第一版生成器很少产生这种序列。
- 我把工具输出的权重从 4 提到 8，输出集中到最近三个调用上，之后连跑 3 次都抓到了，其余变异体也仍然全部抓到。
