# T040 执行笔记

## 开工前的行数估计（2026-10-19）

不含 `Cargo.lock`。上限约 1,500 行。

| 模块 | 内容 | 估计 |
|---|---|---|
| `driver/api.rs` | 5.6.2 的命令、请求的回复、事件（含 `NormUpdate`）、`AgentError` 与分类到 `FailureCode` 的映射 | 230 |
| `durations.rs` | 登记准备超时 3 s、中断兜底 10 s | 12 |
| `orchestrator/mod.rs` | 模块声明与说明 | 10 |
| `orchestrator/queue.rs` | `QueueCore`：入队、出队、暂停、恢复、`queue_edit`（`replace`，`steer` 只留位置） | 170 |
| `orchestrator/turn.rs` | `TurnMachine`：状态、连接与先 close 再 resume、快照与 3 s、中断与 10 s、结束顺序、出队与暂停 | 480 |
| 测试：`QueueCore` 单元测试 | 版本号、暂停、编辑的各种错误 | 60 |
| 测试：`TurnMachine` 场景测试 | 正常回合、准备超时、中断兜底加迟到响应加 close/resume、#556、连接失败、适配器退出 | 220 |
| 测试：性质测试与测试辅助 | 模拟 agent、快照端口、连接的环境；四条 PROP；收尾到静止 | 330 |
| 合计 | | 约 1,510 |

估计贴着上限。实现部分约 900 行，测试约 610 行。若做完 `TurnMachine` 时实现部分已经明显超过 950 行，就停下来汇报，按任务书的建议把 `driver::api` 拆出去。

## 进度统计

（每完成一个模块记一次）
