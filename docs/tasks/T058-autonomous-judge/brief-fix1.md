# T058 修复第 1 轮：静默结束后的出队保护有反例

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主编排）。T043 的执行者接手，因为组合核心 `SessionCore` 是 T043 从 T058 的测试环境提成正式代码的
- 分支：`fix/T058-dequeue-guard`
- 依赖：T058、T043（都已合并）。T088 要在组合核心上接交互代理，本修复排在 T088 之前

## 背景

T037 跑 CI 时随机撞上一个性质测试失败，编排者在 main 上和 T043 之后的分支上都稳定复现了：

- 用例：`orchestrator::session_props::prop_autonomous_dequeue_guard_queue_waits_while_the_agent_works`（PROP-autonomous-dequeue-guard）；
- 失败的断言：`Guard: after a turn ended on silence, the queue waits for 30 s of quiet`；
- 种子（缩减后的操作序列）：

  ```
  cc 7390a05ebe580707816e5bab38bbf67b0501e1d444018d9994dbaf6d6edc9900 # shrinks to ops = [Send, Send, Interrupt, Text(0), Advance(25000), Advance(25000), Thread(true)]
  ```

- 复现：把这一行写进 `crates/cupper-host/proptest-regressions/orchestrator/session_props.txt`，再运行 `cargo test -p cupper-host --lib orchestrator::session_props`，每次都失败。

这个问题在 T043 之前就有，T043 把 `Session` 原样提成了 `orchestrator/session/core.rs` 的 `SessionCore`，所以产品代码里也带着它。修好之前，CI 可能随机失败。

## 范围

**做**：
1. **先定位根因**，三种可能：
   - 裁判（`AutonomousJudge`）对"静默结束"或出队保护的判定错了；
   - 组合接线（`SessionCore` 里回合机、裁判、合并器、队列之间的顺序）错了；
   - 性质本身写错了，即模型对 5.2.6、Q10 的理解与设计不符。

   把这串操作逐步走一遍：每一步回合机、裁判、队列的状态，哪一步让队列提前出队。写进 `notes.md`。
2. **在根因所在的位置修**。修产品代码时，同时看 `SessionCore` 和 actor 有没有别的地方依赖同样的错误假设。
3. **种子文件入库**：`proptest-regressions/` 定为入库（proptest 的推荐做法，编排者已裁定）。把上面这个种子文件随修复一起提交。确认 `.gitignore` 没有忽略它。
4. **加大用例数再找一遍**：用 `PROPTEST_CASES` 调大两条组合性质（PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable）的用例数，例如 2 万，换几个种子各跑几次。找到新的反例同样处理，也写进 `notes.md`。

**不做**：
- T088 的交互接线；
- 改动与这个问题无关的接线。

**需要先停下来问编排者的情况**：根因是性质与设计的理解不一致，而修性质或修代码都要改变 5.2.6 / Q10 规定的行为。这时写清楚两种理解各自的依据，汇报后等裁定，不要自己选。

## 验收标准

- [ ] 根因写进 `notes.md`，包括逐步状态和修复的理由。
- [ ] 种子文件已入库，这个种子通过。
- [ ] 加大用例数之后，两条组合性质各跑 5 次以上都通过，写明用例数和次数。
- [ ] 测试名不变，仍以用例编号开头。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T058-autonomous-judge/ci/` 通过，摘要随任务提交。跑之前确认工作树里没有 proptest 新写出、尚未提交的种子文件，否则 protocol 作业会报"工作树被改动"。

## 参考资料

- 设计：`02-host.md` 5.2.6（自发回合、30 s 出队保护）；`03-protocol-v1.md` 5.3.11 末尾"T041 实现时的裁定"；PRD 的 Q10。
- `docs/tasks/T058-autonomous-judge/`：`brief.md`、`notes.md`、`review.md`。
- `docs/tasks/T043-session-actor/review.md`（编排者的复现记录）；`docs/tasks/T037-host-lifecycle/report.md`（遗留问题第 1 条）。
- `crates/cupper-host/src/orchestrator/session_props.rs`、`orchestrator/session/core.rs`、`orchestrator/autonomous.rs`（裁判）。

## 工作方式

在原来的 T043 worktree 里，从最新的 main 开新分支：`git checkout -b fix/T058-dequeue-guard main`。T043 的分支已经合并，不要再往它上面提交。

## 规模

S，预计 400 行以内（含测试）。预计会超出时先停下来汇报。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report-fix1.md`。不要自己创建汇报文件。汇报里不要写本机的绝对路径。`notes.md` 由执行者在任务分支上提交，追加一节"修复第 1 轮"即可。
