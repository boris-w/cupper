# T058 会话纯核心 II-b：自发回合判定与组合

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主编排）。与 T041 是同一位执行者；裁判的设计在 T041 的 `notes.md` 里。
- 分支：`feat/T058-autonomous-judge`
- 依赖：T041（已合并）。本任务由 T041 拆分而来（计划 v1.3.8）。T043（会话 actor，关键路径）依赖它。

## 背景

T041 已经做好三部分：与回合机的衔接（`HoldQueue`、`TurnSignal`）、`Coalescer`、分组与最终回答。本任务补上：
- 判定自发回合何时开始和结束、能否出队的裁判 `AutonomousJudge`（Q10）；
- 一个组合测试环境：把回合机、裁判和合并器按 T043 的接线方式组合成测试用的核心，用来验证两条跨核心的性质，同时作为 T043 接线的参考。

## 范围

**做**（计划 T058 一行；设计 5.2.6；5.3.11 末尾"T041 实现时的裁定"）：
1. **`AutonomousJudge`**：
   - **何时开自发回合**：没有进行中的回合时，收到回合外的更新就开（K9）。
   - **Ending 期间的输出**：先 `HoldQueue(true)`；等这个回合的 `turn.completed` 写出后，立即开自发回合来承载这些输出，排在任何排队消息之前。
   - **何时结束**：没有运行中的工具、也没有待处理的请求时，下面任一情况发生就结束：
     - 收到带 `cost` 的 `usage_update`；
     - thread 变为 idle；
     - thread 已经是 idle 时有工具结束；
     - 静默 10 s，但 thread 已知在运行时不算。
   - **静默结束之后的出队保护**：agent 再静默 30 s，才放开出队；用户在空闲时发起回合，也会解除保护。
   - **回合外的交互请求**一律放进自发回合（T014 的裁定）。
   - 用到的时长 `autonomous_silence`、`dequeue_guard` 已经由 T041 登记。
2. **组合测试环境**：
   - 回合机的每个动作，先转成 `TurnSignal` 交给合并器，再交给裁判；
   - 裁判发出的 `Start`、`End`、`Hold`，在回合机当前这一步结束之后，作为输入回送给回合机。

**不做**：actor 的接线（T043）。

## 验收标准

- [ ] PROP-autonomous-dequeue-guard（计划已登记）：
  - 静默结束之后的出队保护按上面的规则生效和解除；
  - agent 还在工作时，不出队。
- [ ] PROP-turn-snapshot-immutable（计划已登记）：
  - 回合快照写下后不再改变；
  - 晚到输出只在当时的回合里出现，必要时由裁判开自发回合来承载。
- [ ] 全部测试用 `CoreDriver` 在虚拟时间上驱动，性质测试用 proptest，不睡眠。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T058-autonomous-judge/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项

- **合并器和组合环境**（T041）：
  - 合并器已经能暂存 Ending 期间的更新，也能为被放弃的工具建晚到条目；
  - 组合环境里的转发顺序见上面"范围"第 2 条。
- **设计裁定**：5.3.11 末尾"T041 实现时的裁定"和"T014 实现时的裁定"。
- **纯核心框架**（T022）：用 `CoreDriver`；核心需要 ID 时用 `IdGen::new_id_at(now.wall_ms)`。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 规模

估为 M，约 700–750 行，见 T041 的推算。上限约 1,500 行。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

## 参考资料

- 设计：`02-host.md` 5.2.6、5.2.16；`03-protocol-v1.md` 5.3.5、5.3.11；`06-agent-driver.md` 5.6.9（K1、K9、K12）；`13-open-issues.md` 的 Q10。
- T041 的代码和 `notes.md`；T048 的 `orchestrator/turn.rs`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
