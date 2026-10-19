# T011 工作笔记

## 拆分计划与行数估计（开工前）

### 估算方法

按 T010 在 rustfmt 默认风格下的实际密度估算：

| 写法 | 行数 |
|---|---|
| 带一行文档的必填字段 | 约 2 行 |
| 可选字段（`#[serde(default, skip_serializing_if = ...)]`） | 约 3 行 |
| 结构体本身（derive、文档、括号、空行） | 约 5 行 |
| `open_enum!` | 约 6 行，每个取值另加 1 行 |

T010 的教训是低估了格式化后的行数和测试，所以这次把测试单独列一栏。

### 按模块估计

| 模块（新文件） | 内容 | 类型数（结构体 / 联合 / 枚举） | 代码估计 |
|---|---|---|---|
| `open_enum.rs` 扩展 + `decode.rs` | `open_union!` 宏（见下文"技术方案"第 1 条）、严格与宽容两种解码、`kind` 作标签时的兜底类型 | — | 200 |
| `domain.rs` | 公共领域类型：`Content`、`BlobRef`、`PromptContent` 的 5 种块（含 `review_comment`、`text_blob`）、`AgentKind`、`PrincipalKind`、`ClientKind`、`ClientRole`、`Tier`、`ResolvedBy`、`Cost`；`ids` 增加 `BlobHash`、`IntentId`、`GroupId` | 9 / 2 / 6 | 165 |
| `index.rs` | 索引流的 10 种事件：`project.upserted`（`trust_state`）、`project.removed`、`session.upserted`（`SessionSummary`，含 `pending_kind`）、`session.removed`、`agents.updated`（`AgentStatus` 及 CLI、适配器、运行时、登录、配置目录的子结构，git 和 git-lfs 版本）、`host.updated`、`clients.updated`、`elevation.requested`/`resolved`、`notification.intent` | 18 / 0 / 16 | 320 |
| `capabilities.rs` | `SessionCapabilities`（含 `turn.interrupt_stops_commands`）、`ConfigOption`、斜杠命令、提取层能力 | 11 / 0 / 6 | 150 |
| `session.rs` | 会话流中条目以外的事件：`session.configured`（`project_config`、`confirmation_required`）、`session.capabilities`、`workspace.*` 3 种、`turn.started`/`completed`（时间字段、`final_item_ids`、用量）、`item.started`/`delta`/`updated`/`completed` 的外壳、`queue.updated`（条目带 `content`、`reply_to`）、`snapshot.recorded`（`stat`、`reason`）、`usage.updated` | 17 / 0 / 14 | 290 |
| `item.rs` | 条目联合（`kind` 作标签，8 种）：`user_message`（`reply_to`）、`agent_message`、`thought`、`tool_call`（`ToolCall`，17 个字段，含 `query`、`url`、`parent_item_id`、`output_state`、`output_tail`、`output_bytes`、`diffs`）、`late_tool_output`、`plan`、`notice`、`error`；`item.updated`/`completed` 共用的 `ItemPatch`（约 19 个可选字段）；按 `tool_kind` 的必填字段表（`ToolCall::missing_required`） | 10 / 1 / 11 | 300 |
| `interaction.rs` | `interaction.requested`/`resolved`；请求联合（`permission`、`question`、`plan_confirm`、`auth_url`）；`Option`、`Effect`（`unknown`、`tier_after`、`rule`）、`requires`；回答联合（`selected`、`answer`、`dismissed`、`opened`）；`request_digest` | 12 / 2 / 9 | 290 |
| `event.rs` | `EventBody`（24 种事件，邻接标签 `type`/`data`）、所属流的判定、`item.delta` 的 `Mergeable` 与 `x-mergeable`、`EVENT_VERSION`、`fallback` 的填法 | 1 / 1 / 1 | 120 |
| `error.rs` 修改 | `ErrorData.resolved_by` 改为具名字段（交接事项） | — | 10 |
| **代码合计** | | | **约 1,845** |

| 测试（UNIT-proto-events） | 估计 |
|---|---|
| 每种事件和每个联合成员的往返样例（约 45 条 JSON，加上测试框架） | 110 |
| 未知类型、未知枚举值、未知联合成员；残缺数据在严格和宽容两种模式下的结果 | 70 |
| `open_union!` 宏本身 | 40 |
| `Mergeable`：按 `(item_id, field)` 合并、上限、其他事件不合并 | 35 |
| `request_digest`；`x-mergeable` 只标在 `item.delta`；所属流的判定；`resolved_by` | 55 |
| **测试合计** | **约 310** |

**总计约 2,150 行（按 ±10% 算，在 1,950 到 2,400 行之间）**，明显超过 1,500 行的上限。按任务书的要求，先停下来汇报，不开始写代码。

### 建议的拆分（两个任务，依次进行）

| 任务 | 范围 | 估计 |
|---|---|---|
| **T011a 事件基础、领域类型与索引流** | `open_union!` 和严格/宽容解码；`EventBody` 的骨架（先放索引流的变体）和所属流的判定；`domain.rs`；索引流的全部事件；`SessionCapabilities`（`CliCompat` 与 `AgentStatus` 共用；`agents/capabilities` 方法也会返回它）；`fallback.summary` 的填法；`ErrorData.resolved_by` | 代码约 905，测试约 150，**合计约 1,055** |
| **T011b 会话流：条目、ToolCall 与交互请求** | `session.rs`、`item.rs`、`interaction.rs`；`EventBody` 补上会话流的变体；`item.delta` 的 `Mergeable` 和 `x-mergeable`；`request_digest` | 代码约 940，测试约 160，**合计约 1,100** |

- 依赖：T011b 依赖 T011a 的宏、解码函数和公共类型。
- T014 的 `IndexView` 在 T011a 合并后就可以开工，`SessionView` 要等 T011b。
- 两个任务合起来正好覆盖 5.12.5 第 2 组里属于事件和领域类型的各项。对照表在两个任务的汇报里各自给出自己负责的部分。

## 技术方案（两个任务共用）

1. **开放联合要区分"未知"和"残缺"**。
   - 已用一次性的小程序实测（serde 1.0.229、schemars 1.2.3，程序没有入库）：派生的 `#[serde(tag = "type")]` 加上末尾的 `#[serde(untagged)] Unknown(..)` 时，类型已知但数据残缺的值（如 `{"type":"permission","title":1}`）会**静默落到 `Unknown`**，不报错。用这种写法，严格模式无从实现。
   - 方案：新增 `open_union!` 宏。
     - `Serialize` 和 `JsonSchema` 照旧派生。schemars 对这种写法产出 `anyOf`，兜底成员是 `$ref`，不会出现 `oneOf` 同时匹配两个成员的问题（同一次实测确认）。
     - `Deserialize` 由宏生成：标签已知时，按对应的变体严格解析，错误原样传出；标签未知时才落到 `Unknown`，并原样保留数据。
     - 宏支持内部标签（`type`，或条目用的 `kind`）和邻接标签（事件的 `type`/`data`），也支持没有字段的成员（如 `dismissed`）。宏同时给出 `TAGS` 列表和 `tag()` 方法。
2. **严格模式开关**：`Strictness::{Strict, Tolerant}`，作用于 `Event<EventBody>` 和单独的 `EventBody`（快照和历史里的 `ops`）。
   - 严格模式：任何一层出现"类型已知但数据残缺"都直接报错，用于 CI 的金样例。
   - 宽容模式：事件的 `type` 已知、解析却失败时，把**整个事件**降级成 `EventBody::Unknown`，同时返回错误供调用方记日志（ADR-04 修订第 4 条："降级成未知事件"）。嵌套的联合遇到未知标签时照常落到各自的 `Unknown`，不降级整个事件。
3. **一个 `EventBody` 覆盖两条流**：`stream/events`、`stream/snapshot`、`history/page` 都用同一个事件类型，生成的 TS 也简单。`EventBody::stream_kind()` 给出每种事件属于哪条流，有测试覆盖。
4. **`fallback.summary` 的填法（交接事项）**：建议定为"最后兜底的英文短句"，不用语义码。
   - 理由：只有不认识这个 `type` 的旧客户端才会用到 `fallback`。新类型带来的新语义码，旧客户端同样没有翻译，下发语义码对它没有用。
   - 规则：
     - `kind` 是开放枚举，取客户端已经会画的占位形状，如 `item`（消息流里的一行）、`hidden`（不显示）；
     - `summary` 是给开发者看的英文，最多 120 个字符，不含 agent 给的文本和路径，与 `error.message` 的定位相同；
     - 界面显示本地化的通用占位，例如"此版本无法显示这类内容"，`summary` 只放在 tooltip 或诊断信息里。
   - 在 T011a 里写进 `envelope` 的文档，并写一条测试。
5. **公共类型的归属**：`PrincipalKind`、`ClientKind`、`ClientRole` 既用在事件里（`actionable_by`、`resolved_by`、`clients.updated`），也用在 T012 的 `initialize` 里，所以放在 `domain.rs`。`Scope` 沿用 `rpc` 里已有的定义。

## 规划中发现的设计空缺（按下列建议实现，并在汇报中列出）

| # | 位置 | 问题 | 建议 |
|---|---|---|---|
| 1 | `notification.intent.intent_id`、`group_id` | 5.4.4 没有给这两种 ID 定前缀 | 新增 `IntentId`（`n_` + UUIDv7）、`GroupId`（`g_` + UUIDv7）。两个前缀都还没有被占用 |
| 2 | blob 的 `hash` | 5.4.4 规定是 64 位十六进制、不带前缀，`ids` 里还没有这个类型 | 新增 `BlobHash`，带 `x-id`：内容里有时间等每次运行都不同的值时，哈希也会变 |
| 3 | `late_tool_output.output` | 5.3.5 写作必填的 `Content`，但 `item.started` 时还没有输出（输出走 `field: output` 的 delta） | 改成可选，与 `ToolCall.output?` 一致 |
| 4 | `Option.effect.ends_turn` | 实际有三种状态：会、不会、未知（kind 未知的选项；Codex 的 `revise_plan` 未实测，按"可能结束回合"提示） | 用 `Option<bool>`，不出现表示未知 |
| 5 | `item.updated`、`item.completed` 里的"…要更新的字段"、"…最终字段" | 没有给出具体的形状 | 共用一个展开的 `ItemPatch`：各类条目可以更新的字段全部写成可选字段，不带 `kind`（与 5.3.5 一致）；同名字段在各类条目里的类型相同 |
| 6 | `workspace.failed {code}` | `code` 的类型没有定 | 写成 `{code: ErrorCode, reason?: ErrorReason}`，与错误对象的写法一致 |
| 7 | `turn.completed.error_code`、`SessionSummary.error_code`、`error` 条目的 `code` | 取值没有定 | 一个开放枚举 `FailureCode`：取 5.6.2 中 `AgentError` 的分类（`not_logged_in`、`cli_too_old`、`session_not_found`、`process_exited`、`restartable`、`model_unsupported`、`quota_exhausted`、`other`），加上宿主侧的 `adapter_exited`、`host_restarted` 等 |
| 8 | `AgentStatus` | 5.3.5、5.6.2、5.2 的运行时安装器各自写了一部分，都是文字描述，没有给出结构 | 写成具体结构：`{agent, state, cli?: {path, version?, compat, install}, adapter: {version, runtime, failure?}, auth: {state, kind?, plan?}, rss_bytes?, config_dir}`；登录状态只有类型和套餐名，不含邮箱和组织 |
| 9 | 用量里的 `cost` | 是数字还是带币种的结构，没有定 | 按 ACP `usage_update` 的形状写成 `{amount, currency}` |
| 10 | `session.configured.config` 的值、提问回答的 `values` | 类型没有定 | 配置项的值用字符串（ACP 的配置项是 select 类型）；回答的值用带类型的 `AnswerValue`（字符串、数字、布尔、字符串数组）。数字用 `serde_json::Number`，保证往返时整数不会变成浮点数 |
| 11 | `queue.updated` 条目的 `created_by` | 类型没有定 | 用 `PrincipalId` |
