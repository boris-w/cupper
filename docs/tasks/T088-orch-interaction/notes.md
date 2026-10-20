# T088 工作笔记

- 执行者：子 agent（Rust 工程师，宿主编排；T043 的执行者）

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释。草估之后按 T043 的实测比例放大：T043 的实现草估 885 行，实际 1,130 行（×1.28）；测试草估 400 行，实际 479 行（×1.2）。按计划 1.1 的系数（实现 ×1.7、测试 ×1.5）推算的数也列在后面，以便对照。

| 模块 | 内容 | 草估 |
|---|---|---|
| `session/core.rs` | 交互代理替换请求计数；请求先送裁判（`Asked`）、再送合并器（`Interject`），然后由交互代理写出；没有开着的回合（收尾期间）时请求先暂存，下一个回合开始后写出；`CancelAsks`、`Settle`、适配器退出、撤回、回答交给交互代理，每次解决送裁判 `Resolved`；记下当前的适配器实例 | 110 |
| `interaction/mod.rs` | 不存在的请求回 `not_found`；查询已结束请求的接口（`reply_to` 核对用） | 10 |
| `session/actor.rs` | `interaction/respond`（`Respond.tier` 按当前配置和能力换算）、`interaction/get`（视图里的请求）；`reply_to` 核对后写进 `SendRequest`；审计 `interaction.responded`；请求出现和解决后通知索引 | 120 |
| `session/rows.rs` | `interactions` 行：写出时插入，解决时更新 | 30 |
| `session/manager.rs`、`mod.rs` | `facts.option`（会话视图里的请求和选项）；`SessionDeps` 加审计 channel 和索引接口；索引接口的 trait 与空实现 | 50 |
| 网关 | `Call` 带上客户端身份（`resolved_by` 要客户端种类和名字）；审计记录加 `InteractionResponded`；`Gateway::new` 改为接收外面建好的审计 channel，宿主组装时同一个发送端交给会话 | 35 |
| 协议 | `interaction/respond` 的 `x-errors` 加 `not_found`（生成物不计）；5.3.4 一行 | 2 |
| `session_props.rs` | 性质测试改用真实的交互代理：请求是真的 `DriverEvent::Ask`，解决是撤回或测试身份的回答；模型按事件记下待处理的请求 | 45 |
| 实现小计 | | **约 400**（×1.28 ≈ 510；按 ×1.7 ≈ 680） |
| 测试：IT-orch-interaction | 复用 IT-orch-lifecycle 的夹具；回合内审批（带 `turn_id`、ACP 收到选项、`resolved_by`）、先到先得、四种拒绝、`not_found`、审计；回合外的请求开自发回合且解决前不结束；中断取消挂着的请求、之后才写 `turn.completed`；撤回；`reply_to` 的接受与拒绝 | **约 290**（×1.2 ≈ 350；按 ×1.5 ≈ 435） |
| **合计** | | **约 860**（按计划 1.1 的系数约 1,115） |

按 T043 的实测比例推算约 860 行，低于约 1,000 行的停止线，开工。每完成一个模块统计一次；按已完成部分的实际比例推算，总数预计超过约 1,000 行时停下来汇报。
