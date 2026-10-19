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

按与 main 分叉点的 `git diff --numstat` 统计，不含 `Cargo.lock` 和 `docs/`。

| 时点 | 模块 | 实现 | 测试 | 累计 |
|---|---|---|---|---|
| ec396ff | `driver/api.rs`（含 `Cargo.toml`、`driver/mod.rs` 各 1 行） | 291 | 22 | 313 |
| 13bf686 | `orchestrator/queue.rs`、`mod.rs`、`lib.rs` | 168 | 105 | 587 |
| e03a930 | `orchestrator/turn.rs`、`durations.rs`（含一个冒烟场景测试） | 751 | 135 | 1,473 |

**超限，停工汇报**（守则第 8 条）。实现部分已经 1,210 行，比估计的 900 行多三分之一。还没写的是四条 PROP 和它们的测试环境（约 350 行），以及其余场景测试（约 100–150 行），预计总量 1,900–2,000 行，明显超出约 1,500 行的上限。

估少的原因：
- `TurnMachine` 的公共类型（`Input`、`Action`、`Reply` 等 12 个类型）约 200 行，估计时没有单列；
- 协议事件的结构体字面量经 rustfmt 展开后很长，`start`、`end` 两个函数就有 100 多行；
- `driver::api` 按 5.6.2 列全了命令、事件和 `NormUpdate`，比估计多 80 行。

### 拆分建议

- **方案 A（推荐，即任务书的建议）**：T040 收窄为 `driver::api` 和 `QueueCore`，到 13bf686 为止（587 行，测试齐全），可以直接审阅合并，先放行只依赖驱动接口的 T053。`TurnMachine` 加四条 PROP 另起一个任务：实现已经写好（e03a930，751 行，跑通了正常回合，另用临时测试走通了中断截止加先 close 再 resume），补测试后约 1,250 行。PROP-queue-fifo 要靠回合结束来出队，所以四条 PROP 都随 `TurnMachine` 走。T041、T043 改为依赖新任务。
- **方案 B**：不拆，在本分支补完测试，总量约 1,950 行。

## 实现中的取舍与待裁定事项

1. **prompt 已经正常返回、但还有交互请求挂着**（任务书点名要提方案）。prompt 返回说明 agent 已经不再等这些请求的回答，回合不能因此一直挂着。方案：回合的结局一定下来，`TurnMachine` 先发 `Action::Settle(TurnEnd)`，由交互代理（T042）把这个回合仍挂着的请求结掉，并对 ACP 应答 `Cancelled`，免得适配器一直等；之后才是结束快照和 `turn.completed`，满足 T014 的顺序裁定。请求的状态和原因：
   - 中断导致的结束：`cancelled`，`reason: turn_cancelled`（已有）；
   - 适配器退出：`expired`，`reason: adapter_exited`（已有）；
   - prompt 正常返回或出错：`cancelled`，**建议给 `ResolutionReason` 新增 `turn_ended`**（开放枚举，附加式改动）。不改协议的替代是用 `withdrawn`：agent 不等回答就结束回合，相当于撤回了请求。请编排者裁定。
2. **中断时的顺序**：先 `Cancel`，再 `Action::CancelAsks`（请求标 `cancelled` 并应答 ACP），再设 10 s 定时器。`Cancelling` 期间新到的请求，T042 应当直接按取消处理。
3. **结束快照**：只有发过 prompt 的回合才拍结束快照；在 `Preparing` 里被中断或连接失败的回合直接结束。起始快照 3 s 超时时写 `snapshot.recorded{phase: start, status: failed, reason: timeout}`，之后迟到的快照结果丢弃。
4. **暂停的解除**：设计写"队列暂停，等用户发新消息或调用 `session/queue_resume`"。我的理解是：空闲时用户发新消息，这条消息立刻开回合，同时解除暂停；它正常结束后，排着的消息接着出队。队列为空时不暂停（没有消息在等，暂停看不出来）。
5. **#556 的判定**：没有经宿主中断、prompt 却返回 `cancelled` 的，认定是 agent 自己取消（Codex 拒绝审批），回合为 `cancelled`，不暂停队列。`Cancelling` 中返回的不论 `stop_reason` 是什么，回合都记为 `cancelled` 并暂停；返回错误则记为 `failed`。
6. **驱动接口的形状**：
   - `Prompt` 带 `PromptTag`，结果原样带回，用来分辨"截止后才到的旧结果"；
   - agent 的请求用 `AskId` 指代，ACP 的 responder 留在 `AgentRuntime` 里，回答走 `DriverCommand::Answer`。这样 `DriverEvent` 是纯数据，纯核心可以直接处理；
   - `Prompt` 的内容直接用协议的 `PromptContent`，由驱动实现（T055）转成 ACP 的块（图片读 blob 转 base64、评论排成文本）；
   - `AttachInfo` 只带一个 `SessionCapabilities`，里面已经有配置项和斜杠命令；
   - 命令是纯数据，`Attach` 的事件发送端和请求的回复通道由 actor 在交给 runtime 时补上。`TurnMachine` 发的是 `Action::Attach(AttachTarget)`，工作区、配置和信任状态由 actor 补齐。
7. **自发回合**只留了进入和退出：`Input::AutonomousStarted`、`AutonomousEnded`。自发回合里中断：发 `Cancel`，回合记 `cancelled`、暂停队列；适配器退出时记 `failed`。不拍快照，30 s 出队保护归 T041。
8. **未覆盖、留给后续**：K6 自动 resume 重发和配置、信任的门控暂停（T044）；Steer（T140，目前 `mode: steer` 按排队处理，回复里带 `fallback: unsupported`）；从数据库恢复回合序号、队列和 agent 会话（T043）；`Ending` 期间到达的更新怎样归属（T041、T043）。
