# T058 执行笔记

## 开工前的行数估计

按模块估，不含 `docs/`。T041 的经验是：rustfmt 会把稍长的结构体字面量展开，实际行数约是草估的 1.7 倍，下面已经按这个比例放大。上限约 1,500 行。

| 模块 | 内容 | 估计 |
|---|---|---|
| `orchestrator/autonomous.rs` | `AutonomousJudge`：何时开自发回合、何时结束、出队保护；Ending 期间的输出 | 230 |
| 测试：`autonomous.rs` | 场景：静默结束与 30 s 保护；Ending 期间的输出先保持队列，结束后开自发回合 | 70 |
| 测试：`orchestrator/session_props.rs` | 组合测试核心（回合机、裁判、合并器，按 T043 的接线转发），加 agent 和端口的模型；PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable | 450 |
| **合计** | | **约 750** |

## 进度统计

按与 main（`d501afb`）的 `git diff --numstat` 累计，不含 `docs/`。

| 提交 | 内容 | 累计 |
|---|---|---|
