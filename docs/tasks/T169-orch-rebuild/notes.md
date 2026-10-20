# T169 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）
- 日期：2026-10-20

## 读到的现状（开工前）

- **重建**：检查点的 `view_v` 不符时，`streams` 只用最近 20 个回合快照，再重放检查点之后的事件。检查点之前的 `session.configured`、`session.capabilities`、`queue.updated` 不会回来，与日志是否修剪无关。T089 的载入遇到视图里没有配置时回内部错误。
- **准备阶段崩溃**：本地工作区在创建时就写了 `workspace.ready`。第一条消息先等信任检查，通过后才开第一个回合，回合开始时清空 `initial_prompt_json`。崩溃发生在这之间时，行里还留着第一条消息，没有任何回合。另有一种情况：检查没通过，第一条消息进了排队，`initial_prompt_json` 却没清空。这时如果载入时再把它重开一次，就会重复。
- **没结束的条目**：协调器在回合结束时，把文本和计划记为 `completed`，工具调用记为 `abandoned`；中断且 agent 的中断不停命令时，执行类工具标 `may_still_run`。补写照这个取值，与有序退出时宿主自己写的一致，不需要新的协议取值。
- **会话层特权审计**：
  - `session/create` 的档位检查在 `accept` 里；
  - `interaction/respond` 的判定在交互代理里，actor 只看到成功或失败；
  - `session/set_config` 在 M1a 还没实现（actor 回 `unsupported`），这一处只能等 M1b，在汇报里说明；
  - 格式用 `AuditEvent::Privileged`，动作和细节取 `policy::describe` 带上会话事实后的结果，经 actor 已有的审计通道发出。

## 开工前的规模估计

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 重建 | 视图缺什么就从行和能力缓存补什么：`session.configured`（行与项目信任）、`session.capabilities`（缓存）、`queue.updated`（行，版本照旧）；替换"缺配置回内部错误" | 55 | 35 |
| 准备阶段崩溃 | 载入时把 `initial_prompt_json` 交回机器；宿主启动时主动载入这些会话；第一条消息进了排队时同一事务清空 `initial_prompt_json`，避免重复 | 55 | 30 |
| 没结束的条目 | 载入时收集运行中回合里没结束的条目，核心在请求过期和 `turn.completed` 之前补写 `item.completed` | 30 | 15 |
| 会话层特权审计 | `session/create` 切到 `full_access`；`interaction/respond` 选特权或完全访问的选项；成功后各记一条 | 40 | 50 |
| **合计** | | **约 180** | **约 130** |

合计约 **310 行**，与任务书的估计相同，在约 380 的停止线以内。
