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
