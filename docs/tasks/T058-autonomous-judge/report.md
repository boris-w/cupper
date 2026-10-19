# T058 汇报

- 执行者：子 agent（Rust 工程师，宿主编排）
- 分支：`feat/T058-autonomous-judge`（worktree `.claude/worktrees/T041`，工作区干净）
- 完成日期：2026-10-19

## 做了什么

- **`orchestrator/autonomous.rs`：`AutonomousJudge` 纯核心**，按 T041 的 notes 和 5.3.11 末尾"T041 实现时的裁定"实现。
  - **输入**：
    - `Turn(TurnSignal)`；
    - `Driver(DriverEvent)`：全部驱动事件；
    - `Asked`：交互代理写出请求之前送入；
    - `Resolved`：每次解决请求之后送入；
    - 两个定时器：静默 10 s、出队保护 30 s。
  - **输出**：`Start`、`End`、`Hold(bool)`，以及定时器请求。`Action::turn_input()` 把前三个换成回合机的 `AutonomousStarted`、`AutonomousEnded`、`HoldQueue`。
  - **规则**：
    - **输出**指更新和请求。没有回合时，开一个自发回合（K9；回合外的请求也一样）。回合收尾期间到达的输出，先 `Hold(true)`，等 `turn.completed` 写出后立即开自发回合，排在排队消息之前。
    - **结束**：没有运行中的工具，也没有待处理的请求时，下面任一情况就结束：
      - 收到带 `cost` 的 `usage_update`；
      - thread 变为 idle；
      - thread 已是 idle 时有工具结束（晚到输出）；
      - 静默 10 s，thread 已知在运行时不算。
    - **出队保护**：只有静默结束的回合才保持队列，直到 agent 再静默 30 s。期间 thread 变为 active 或上游重试，都会让这 30 s 重新开始。用户发起的回合结束时，保护随之解除。
    - **运行中的工具**：Settle 时清空，晚到输出会把工具重新记为运行中。
    - **适配器退出**：thread 状态回到未知，10 s 重新计时。这一条是性质测试找出来的。
  - 两个场景测试：
    - 静默结束后保持 30 s，期间的活动会重新计时；
    - 收尾期间的输出先保持队列，回合结束后开自发回合；工具运行时不结束；thread 为 idle 时工具一结束，回合就结束。
- **`orchestrator/session_props.rs`：组合测试核心 `Session`**，按 T043 的接线方式组合回合机、裁判和合并器：
  - 驱动事件依次交给裁判、回合机，如果是更新，最后交给合并器；
  - 回合机的动作按顺序处理：`TurnSignal` 立即交给合并器；`Complete` 本身不写事件，`turn.completed` 由合并器写；
  - 回合机这一步结束后，再把信号交给裁判，裁判的动作作为回合机的输入回送；
  - 请求先送裁判（`Asked`），再送合并器（`Interject`）；
  - 交互代理的替身在 Settle 时解决这个回合的全部请求。

  `World` 模拟 agent、快照端口和连接，当场回应每个动作，并核对写出的日志。两条 PROP 共用这套环境，各自只断言自己的规则。
- `orchestrator/mod.rs` 加了两行 `mod`。
- `notes.md`：行数统计、实现上的决定、接线说明、性质测试的设计、覆盖度、变异检验。

## 关键取舍

- **裁判不解析 `DriverEvent::Ask`**：请求改由 `Asked` 和 `Resolved` 两个输入告知，这样不依赖 T042 的 `AskRequest` 类型。T043 在交互代理写出请求之前送 `Asked`，每次解决之后送 `Resolved`。
- **输出与活动分开**：
  - 只有输出（更新、请求）会开回合；
  - thread 变为 active、上游重试只算活动：重新计 10 s，或在保护期内重新计 30 s；
  - 标题、配置、命令列表、用量是元数据，不算活动。
- **删掉一处多余的放开**：
  - 第一版在用户回合开始时就放开队列；
  - 变异检验显示它是等价变异：这个回合进行时本来就不出队，结束时裁判会放开；
  - 所以删掉了。"用户发起回合会解除保护"改由回合结束时放开来实现，性质测试检查这一条。
- **模型里补了两项检查**，否则有的变异体抓不到：
  - 空闲且队列在等时，只有静默结束后的 30 s 内才允许不出队；
  - 收到 `cost`、thread 变为 idle，或 thread 为 idle 时工具结束，空闲的自发回合必须当场结束。没有这一条，忽略 `cost` 的变异会被静默规则掩盖。
- **覆盖度**：用临时计数器跑 256 次：
  - 自发回合 855 个，其中静默结束 175 个；
  - 静默结束后，排队消息等满保护期才开始的有 91 次；
  - 晚到条目 581 个；
  - 收尾期间到达输出 47 次。
- **变异检验**：手工改坏 17 处。
  - 性质测试抓到 15 处，都是由负责那条规则的检查抓到的，明细见 notes。例如：
    - 不保持队列、不开自发回合、回合机忽略 `HoldQueue`，由出队保护的检查抓到；
    - 合并器在收尾期间不暂存、晚到输出写回原条目、每次都新建晚到条目，由快照不可变的检查抓到。
  - "保护期间的活动不重新计时"由裁判的场景测试抓到。
  - 剩下一处是上面说的等价变异。

## 验证方式

`cargo xtask ci pr --record docs/tasks/T058-autonomous-judge/ci/` 在干净的 worktree 里通过（提交 `182c467`）：

| 部分 | 通过 | 失败 |
|---|---|---|
| rust | 1,011 | 0 |
| protocol | 176 | 0 |
| ts | 232 | 0 |
| license | 1 | 0 |

摘要 `ci/pr-182c46705608.json` 已提交，我检查过，里面没有用户名和本机路径，CI 也按编号识别出了两条 PROP。`cupper-host` 在四种特性组合下 clippy `-D warnings` 都通过：全特性、无特性、`test-harness`、`dev-auth`。

对照验收标准：
- [x] **PROP-autonomous-dequeue-guard**，检查以下各条：
  - 静默结束后，排队消息要等结束时刻和最后一次活动都过去 30 s 才开始；
  - 等不满 30 s 时不放开，等满之后必须放开；
  - 收尾期间到达的输出，先进入自发回合；
  - 空闲且没有阻碍时，自发回合 10 s 内结束；"agent 已完成"的信号一到就结束；
  - thread 在运行时不按静默结束；工具运行或请求待处理时不结束。
- [x] **PROP-turn-snapshot-immutable**，检查以下各条：
  - `turn.completed` 之后，没有任何事件指向这个回合；
  - 已结束的条目不再变化；
  - 条目只在进行中的回合里、并且在它 Settle 之前开始；
  - 回合的条目都在 `turn.completed` 之前结束；
  - 晚到条目指向在已完成回合里被放弃的调用，每个调用在每个回合里只有一个。
- [x] 全部测试用 `CoreDriver` 在虚拟时间上驱动，性质测试用 proptest，不睡眠。
- [x] 在干净的 worktree 里跑 CI 通过，摘要随任务提交。
- [x] 测试名以编号的 snake_case 开头：`prop_autonomous_dequeue_guard_…`、`prop_turn_snapshot_immutable_…`。

**规模**：相对分叉点 `d501afb` 共 1,167 行，不含 `docs/`。估计约 750 行，上限约 1,500 行。
- 实现约 300 行（含模块文档）；
- 测试约 864 行：`session_props.rs` 721 行，裁判的场景测试 143 行。

组合测试环境比估计多了约 270 行：模型要同时扮演 agent、快照端口、连接和交互代理的替身，后来又补了两项检查。

## 提交列表

```
1c2b80b docs(task): record the T058 pr CI summary
182c467 docs(task): fix the T058 mutation tally
d4af081 docs(task): T058 line counts, decisions, the combined core and the mutation check
28ab349 refactor(host): drop the judge's redundant release when a user turn starts
ce0f26d test(host): check that the queue moves and that done signals end a turn at once
396666b test(host): combine the session's cores and check the dequeue guard and immutable turns
5e333fd feat(host): add AutonomousJudge, deciding autonomous turns and the dequeue guard
38a4287 docs(task): T058 line estimates per module
```

## 遗留问题 / 范围外发现

- **T043 的接线**：照 `session_props.rs` 里的 `Session` 接，次序要求和 `turn.completed` 由谁写见上文"做了什么"。另外几点：
  - 收到 `Asked` 而开出的自发回合，要在交互代理写出请求之前开好，这样请求带上 `turn_id`。
  - 回合收尾期间到达的请求，T042 的交互代理怎么处理，组合环境里没有建模，替身只计数。请 T043 接线时确认：这类请求放进随后开的自发回合，裁判会等它解决后才结束这个回合。
- **`DriverEvent::RunState`**：裁判把 thread 的状态当作"最近一次报告"来用。只有 Codex 会报告，Claude 的状态一直是未知，所以只按静默规则判断。T110（Codex 提取层）提供 `threadStatus` 时，记得要有 idle。
- **裁判的 `running` 集合**：如果某个工具结束之后，agent 又发来不带结束状态的更新，这个工具会被重新记为运行中，回合就不会因为静默而结束，要等工具再次结束、thread 变为 idle、中断或适配器退出。这在 ACP 里不应出现，如果出现就是提取层的问题，录制金样例时可以留意。
- 宿主任务的行数估计偏低，原因是 rustfmt 会展开结构体字面量，组合测试环境又要模拟很多端口。之后给同类任务估规模时，建议实现按草估的 1.7 倍、组合性质测试按 1.5 倍计算。
