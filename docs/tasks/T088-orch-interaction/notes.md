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

### 实际规模

按 `git diff --numstat` 统计，不含本笔记和生成的 schema。

| 提交 | 内容 | 增 | 删 |
|---|---|---|---|
| `feat(protocol): interaction/respond may answer not_found` | `x-errors`；5.3.4 一行 | 2 | 2 |
| `feat(host): put the client on calls; audit answers …` | `Call.client`；`AuditEvent::InteractionResponded`；`Gateway::new` 接收审计 channel | 48 | 14 |
| `feat(host): wire the interaction broker into the session` | `core.rs` 176、`actor.rs` 152、`rows.rs` 48、`manager.rs` 47、`mod.rs` 32、性质测试 87、夹具 46 等 | 594 | 178 |
| `test(host): IT-orch-interaction` | `interaction_tests.rs` | 240 | 0 |
| 合计 | | **884** | 194 |

估计约 860 行，实际 884 行。

## 实现要点

- **组合核心**（`core.rs`）：
  - 请求（`DriverEvent::Ask`）的顺序：先送裁判 `Asked`；空闲时裁判开自发回合，回合机写 `turn.started`。然后请求进入暂存表；只要有开着的回合（从 `turn.started` 到它的 Settle），就逐个"合并器 `Interject` → 交互代理 `Ask`"写出，请求带上这个回合的 ID。
  - **回合收尾期间到达的请求（T058 交接，已确认）**：Settle 之后没有开着的回合，请求留在暂存表。裁判因为 `Asked` 置了 `start_next` 并保持队列，所以这个回合 `turn.completed` 之后先开自发回合，暂存的请求写进新回合。裁判的请求计数里有它，请求解决之前不会结束这个回合。性质测试覆盖了收尾期间的请求（默认 256 例里约 26 次暂存）；IT-orch-interaction 的"回合外"用例覆盖空闲时的请求。
  - 回合机的 `CancelAsks`、`Settle` 交给交互代理。Settle 的顺序是：合并器结束开着的条目 → 交互代理解决请求 → 裁判收到各次 `Resolved` → 裁判收到 `Settled`。回合机的 `Complete` 之后，合并器才写 `turn.completed`。
  - 撤回：还在暂存表里的请求直接删掉，对 ACP 应答 `Cancelled`，裁判收到 `Resolved`；已经写出的交给交互代理（`withdrawn`）。
  - 适配器退出：属于这个实例、还没写出的请求直接删掉（不应答，实例已经没了）；已经写出的由交互代理标 `expired`。
  - 还没有 attach 时到达的请求，没有实例可以记，直接应答 `Cancelled`。实际上 agent 只能在 attach 之后发请求。
  - `Settle` 在核心处理之后也照常输出，性质测试据此看到"结局已定"；actor 不处理它。
- **actor**：
  - `interaction/respond` 带上 `resolved_by`（principal 和连接自报的客户端种类、名字）以及会话当前的档位。档位按 `permission_mode` 的当前值和能力里的 `risk` 换算，未知时为 `None`。
  - 回答成功并提交之后，CLI 和 test_harness 的回答发出审计记录 `interaction.responded`。
  - `interaction/get` 从会话视图取：待处理的请求总在视图里（窗口里的回合，或 `detached`）；已经淘汰出窗口、又已解决的请求回 `not_found`。重启后从 `interactions` 表补，归 T089。
  - `reply_to` 只接受本会话里状态为 `resolved` 的请求；否则回 invalid params，不新增错误码。客户端只在拒绝成功之后才发这条消息，所以出错说明客户端有 bug。
  - 每批事件里有请求写出或解决时，提交之后用 `IndexNotes::pending` 告诉索引：待处理的个数和最早一个的类别。
- **行**：`interaction.requested` 插入 `interactions`；`request_json` 存整个 `interaction.requested`，T089 重启后可以据此恢复。`adapter_instance` 取核心记下的当前实例。`interaction.resolved` 更新状态、`outcome_json`、`resolved_by`（JSON）、`reason`、`resolved_at`。
- **网关**：
  - `Call` 带上连接在 `initialize` 里自报的 `ClientIdentity`；
  - `Gateway::new` 改为接收外面建好的审计发送端，宿主组装时同一个发送端交给 `SessionDeps.audit`；
  - `supervisor` 现在自己建这个 channel，接收端仍然留给 T059 落盘。
- **性质测试**：
  - 请求改成真实的 `DriverEvent::Ask`；`Op::Resolve` 三次里两次由 test_harness 回答（`dismissed`），其余撤回。还没写出的请求，只能撤回。
  - 模型按 `interaction.requested`、`interaction.resolved` 事件记下待处理的请求。新加一条检查：请求只写进正在进行、还没 Settle 的回合。
  - 只在 attach 之后发请求。
  - 生成器的权重没有变，T058 的种子仍然对应原来的操作序列。
  - 默认 256 例里请求约 60 个；解决方式覆盖了回答、撤回、中断取消、回合结束取消、适配器退出。`PROPTEST_CASES=20000` 各跑 3 次通过。

## 给其他任务的接入点

- **T055（会话管理器的组装）**：`SessionDeps` 多了 `audit`、`index` 两项。`audit` 用 `Host::start` 里交给 `Gateway::new` 的同一个发送端（`supervisor` 里的 `records`）；`index` 在 T034 之前用 `NoIndex`。
- **T034**：实现 `IndexNotes`（`pending(session, count, oldest)`），换掉 `NoIndex`。
- **T059**：审计记录多了 `InteractionResponded { session_id, request_id, outcome, principal }`，落盘时记为 `interaction.responded`。
- **T089**：
  - 载入时用 `interactions` 行的 `request_json`（整个 `interaction.requested`）和状态，经 `Input::Ended` 交给交互代理；
  - `interaction/get` 对淘汰出视图的请求也可以改从表里读。
- **T053、T055**：`DriverEvent::Ask` 带上 `toolCallId` 之后，在 `SessionCore::write_held` 里把 `item_id` 填上。
