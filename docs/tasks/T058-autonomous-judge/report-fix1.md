# T058 修复第 1 轮汇报

- 执行者：子 agent（Rust 工程师，宿主编排；T043 的执行者）
- 分支：`fix/T058-dequeue-guard`（从 main 开出）
- 完成日期：2026-10-20

## 做了什么

- **复现**：种子 `cc 7390a05e…` 写进 `crates/cupper-host/proptest-regressions/orchestrator/session_props.txt` 后每次都失败。我又直接重放了缩减后的操作 `[Send, Send, Interrupt, Text(0), Advance(25000), Advance(25000), Thread(true)]`，逐步打出回合机、裁判、队列和模型的状态。逐步表写在 `notes.md` 的"修复第 1 轮"一节，要点如下：
  - 0 s：回合 t1 在 Preparing 时被中断，以 `cancelled` 结束，排队的那条消息因此暂停（`turn_cancelled`）；
  - 0 s：`Text(0)` 开了自发回合 t2；
  - 10 s：t2 静默结束，裁判保持队列，出队保护定时到 40 s；
  - 40 s：已静默 30 s，裁判放开，但队列还在暂停，没有出队；
  - 50 s：`Thread(true)`，这是放开之后的活动，裁判不再保持；
  - 50 s：收尾时 `queue_resume` 解除暂停，排队消息立刻开回合。模型要求最后一次活动（50 s）之后再静默 30 s，于是断言失败。
- **根因：性质测试的模型比设计严**，裁判和接线都没有错：
  - 设计 5.2.6 写的是"结束后再静默 30 s 才允许出队"，T041 的裁定是"agent 再静默 30 s 才放开出队"。裁判在 40 s 放开，符合规定。
  - 模型的 `guard` 在出队保护放开之后仍然保留，直到用户回合开始或下一次自发回合结束才清掉。所以放开之后再有活动，模型还要求再静默 30 s。
  - 队列没暂停时，保护一放开就出队，两者没有差别。只有"保护期间队列被暂停、放开之后才有活动、再 `queue_resume`"这条路径会撞上，所以默认 256 例很少碰到。
  - `SessionCore` 的组合顺序与这串操作无关，每一步都符合各自的规则。
- **修复**（`session_props.rs`，增 9 行、删 2 行）：
  - 每个操作之前，如果已经过了"静默结束时刻和最后一次活动中较晚的那个再加 30 s"，就清掉 `guard`，保护期满之后的活动不再算进出队保护。
  - 模型清掉 `guard` 的时刻不会早于裁判放开的时刻。
  - 裁判、`SessionCore`、actor 的代码没有改，它们不依赖这个错误的假设。
- **种子文件入库**：只放了任务书给的那一行；`.gitignore` 没有忽略 `proptest-regressions/`。

## 关键取舍

- **修模型，还是修裁判**：
  - 按模型原来的理解，"静默结束之后、用户回合之前，任何一次出队都要等最后一次活动过去 30 s"，就得让裁判在放开之后还能重新保持。这会改变 5.2.6 规定的行为。
  - 修模型只是让测试与设计一致，产品行为不变。所以我判断这不属于任务书说的"修哪边都会改变规定的行为"，没有停下来等裁定。两种理解和各自的依据都写进了 `notes.md`。如果你倾向更严的那种，再告诉我。
- **变异检查留下的种子没有入库**：检查时产生了两个种子，它们来自改坏的代码，放进种子文件会让人误以为曾经有反例。检查做完后代码已还原。

## 验证方式

- **种子**：修复前稳定失败，修复后通过。
- **加大用例数**：两条组合性质（PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable）都用 `PROPTEST_CASES=20000` 跑：
  - 随机种子 7 次；
  - `PROPTEST_RNG_SEED` 固定为 1、2、3 各 1 次；
  - 每次约 44 s，全部通过，没有新反例。
- **变异检查**（修好的模型；检查完已还原）：
  - "静默结束后不设出队保护"：2,000 例内抓到；
  - "保持期间的活动不重新计 30 s"：20,000 例内抓到，缩减为 `[Text(0), Send, Advance(25000), Thread(true), Resolve]`。T058 时这个变异只有场景测试能抓到。
- **测试名**：没有改，仍以用例编号开头。
- **CI**：`cargo xtask ci pr --record docs/tasks/T058-autonomous-judge/ci/` 在干净的 worktree 里通过，跑之前确认没有未提交的种子文件。结果：
  - rust 1,165 个通过；
  - protocol 185 个通过；
  - ts 314 个通过；
  - license、testdata-scan 通过。

  摘要 `ci/pr-5479241fe097.json` 已提交。
- **规模**：代码与测试改动增 16 行（其中种子文件 7 行）、删 2 行；笔记 61 行。

## 提交列表

```
7411e1d docs(task): T058 fix round 1 CI summary
5479241 docs(task): T058 fix round 1, the dequeue guard counterexample
00875c5 test(host): end the dequeue guard model's hold after its 30 s of quiet
```

## 遗留问题 / 范围外发现

- **Codex 的一个空隙（只是观察，没有改）**：出队保护放开之后，空闲时 Codex 报 thread active，裁判不保持也不开回合，所以这时 `queue_resume` 或排队消息会把 prompt 发给一个 thread 仍在运行的 Codex 会话。
  - 这与 5.2.6 一致（threadStatus "只用于显示运行状态和界定自发回合"），也与 T058"thread 变为 active 只算活动、不开回合"的决定一致。
  - Claude 没有这个问题：它再工作一定会有输出，而输出会开自发回合，挡住出队。
  - 是否要让空闲时的 Codex thread active 也挡住出队，可以在 T110（Codex 接入）时一并考虑。
- worktree 保留着，可以接着派 T088。