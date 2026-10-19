# T017 汇报

- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T017-protocol-session-events`
- 完成日期：2026-10-19

**结论**：范围内的内容都做完了，验收标准逐条满足。在干净的 worktree 里跑 `cargo xtask ci pr --record` 通过，UNIT-proto-events-session 和 UNIT-proto-events-index 都是 pass。

**规模超了，请编排者知悉**：`crates/cupper-protocol` 新增 1,644 行、删除 27 行，比 1,500 行的上限多约 10%。

- 代码 1,077 行，低于估计的 1,200 行。
- 测试 567 行，估计是 330 行。超出的部分全在测试。
- 前三个模块写完时累计 1,247 行，和估计一致。按估计推算总数约 1,520 行，没到 `notes.md` 里定的 1,650 行停止线，所以继续做。
- 最后的 `event.rs` 比估计多了约 120 行，主要是会话流的样例表：每种事件、8 种条目、4 种请求、4 种回答，各带有无可选字段两种写法。
- 每个模块的行数见 `notes.md` 的"实际行数"。

## 做了什么

- **开工前**：在 `notes.md` 里写了模块级的行数估计，约 1,530 行，并定下 1,650 行的停止线。之后每完成一个模块统计一次。
- **`session.rs`（新）**：条目和交互请求以外的会话流事件。
  - `session.configured`：含 `project_config`、`confirmation_required {reason, config_digest}`。
  - `workspace.preparing`、`workspace.ready`、`workspace.failed`，其中 `workspace.failed` 是 `{code: ErrorCode, reason?}`。
  - `turn.started`：`turn_no`、`origin`、`started_at`。
  - `turn.completed`：`ended_at`、`duration_ms`、用量、`error_code: FailureCode`、`final_item_ids`。
  - `queue.updated`：条目带 `content`、`reply_to`、`created_by: PrincipalId`。
  - `snapshot.recorded`（`stat`、`reason`、`excluded`）、`usage.updated`。
- **`item.rs`（新）**：
  - `item.started`：把条目联合（`open_union!` 的 `in tag "kind"` 形式，8 种条目）展开在外壳字段旁边。
  - `item.delta`：`field` 取 `text` 或 `output`。
  - `item.updated` 和 `item.completed`：共用 `ItemPatch`，20 个可选字段。`item.completed` 另有 `ended_at`、`duration_ms`、`may_still_run`。
  - `ToolCall`：全部字段，加上按 `tool_kind` 检查必填字段的 `missing_required(ended)`。
  - `late_tool_output`：`output` 可选。
  - `plan`、`notice`（`NoticeCode` 加 `NoticeParams`）、`error`（`FailureCode` 加 `Guidance`）。
- **`interaction.rs`（新）**：
  - `interaction.requested`、`interaction.resolved`。
  - 请求联合：`permission`、`question`、`plan_confirm`、`auth_url`。
  - `InteractionOption` 和 `OptionEffect`：含 `unknown`、`tier_after`、`rule`、`mode_after`，`ends_turn` 是 `Option<bool>`。
  - 回答联合：`selected`、`answer`、`dismissed`、`opened`；`answer` 的值是 `AnswerValue`。
  - `request_digest`。
- **`event.rs`**：
  - `EventBody` 加了 16 个会话流变体，`stream_kind()` 都归到 `Session`。
  - 只有 `item.delta` 带 `x-mergeable`。
  - 为 `EventBody` 实现了 `Mergeable`：同一个 `(item_id, field)` 的相邻 delta 合并，合并后的转义字节数不超过上限。
  - T011 的两个索引流测试改成调用共用的辅助函数（`covers`、`rejected_or_degraded`），行为不变。

## 关键取舍

**形状上的决定**（`notes.md` 有完整说明）

- **`item.started` 的写法**是 `{item_id, turn_id?, started_at, group_id?, kind, ...}`。
  - `group_id` 放在外壳上，因为组内的 `thought` 也要带它，不只是 `tool_call`。
  - schema 是外壳的属性加上 9 个成员的 `anyOf`。
- **`duration_ms` 放在 `item.completed` 上**，所有条目都必填，不作为 `ToolCall` 的字段：用时只有结束时才知道。这和 `turn.completed` 的写法一致。
- **`ItemPatch` 的规则**：出现的字段整体替换，不出现的保持不变，没有"清空"的写法。
  - 有一条测试检查：各类条目的可更新字段都能按同名、同类型解析成 `ItemPatch`（5.3.11 第 5 条）。
- **`late_tool_output` 另加了 `output_tail?`、`output_bytes?`**：它的输出按普通工具输出的规则处理，截断时这两个字段也会经 `ItemPatch` 更新到它上面。
- **权限请求的 `tool.command` 用 `Content`，不用字符串**：第 1 组第 6 条要求超长的命令改为 blob、不截断。
- **选项的 `requires` 直接用 `Scope`**，取值 `approve` 或 `privileged`。
- **线上取值 `unknown` 是已知取值**：`effect.scope` 和 `prefix_risk` 的 `unknown` 分别对应变体 `EffectScope::Unclassified`、`PrefixRisk::Unparsed`，因为 `open_enum!` 已经占用了 `Unknown` 这个变体名。有测试确认 `"unknown"` 解析为已知变体，不会落进兜底。
- **`request_digest` 在 `turn_id` 缺省时省略这个键**，不写 `null`，与线上的写法一致。
- **`notice.params` 必填**，类型是 `NoticeParams {attempt?, max?, paths?}`；没有参数的码写 `{}`。

**设计只列了字段、或写了"等"的地方，我补的取值**（建议编排者写进 5.3.11）：

| 类型 | 取值 |
|---|---|
| `Guidance` | `retry`、`run_login_in_terminal`、`upgrade_cli`、`install_cli`、`quota_exhausted`、`change_model`、`restart_agent`、`none` |
| `StopReason` | ACP 的 `end_turn`、`max_tokens`、`max_turn_requests`、`refusal`、`cancelled`，加宿主的 `host_timeout` |
| `SnapshotReason` | `timeout`、`unsafe_repo_config`、`not_a_repo`、`git_failed` |
| `ExclusionReason` | `too_large`、`nested_repo`、`large_untracked_dir`、`filter_unsupported` |
| `ResolutionReason`（`interaction.resolved.reason`） | `withdrawn`、`turn_cancelled`、`adapter_exited`、`host_restarted` |
| `PlanPriority` | `high`、`medium`、`low`（ACP） |
| `workspace.preparing.progress` | 0–100 的百分比 |
| `effect.rule.location` | 字符串，不用 `WirePath`：适配器给的是 `localSettings` 这类语义值 |

## 验证方式

| 验收标准 | 结果 |
|---|---|
| UNIT-proto-events-session | 通过，共 13 个测试，见下面的覆盖清单 |
| 5.12.5 第 2 组的对照表 | 见下表 |
| 编译到 `aarch64-apple-ios-sim` | debug 和 release 都通过 |
| `cargo xtask ci pr --record` | 在干净的 worktree 里于 `585508d` 运行，通过：rust 237 项、ts 64 项、license 1 项。摘要是 `docs/tasks/T017-protocol-session-events/ci/pr-585508d917a8.json`，不含路径和用户名 |
| 测试命名 | 新增的测试都以 `unit_proto_events_session_` 开头 |

UNIT-proto-events-session 的 13 个测试覆盖：

- **往返**：会话流每种事件，带信封和不带信封各一次。
- **未知**：未知的条目 kind、请求类型、回答类型，以及未知的枚举值。
- **残缺数据**：7 类，包括嵌套联合里的残缺。严格模式报错；宽容模式把整个事件降级成 `Unknown`，数据原样保留。
- **合并**：按 `(item_id, field)` 合并、上限按转义字节计算、其他事件不合并。
- **`x-mergeable`**：只出现在 `item.delta` 上。
- **`request_digest`**：与手写的 JCS 串的 sha-256 对比；`turn_id` 缺省时省略这个键；内容一改摘要就变。
- **`missing_required`**：覆盖各个 `tool_kind`。
- **`AnswerValue`**：整数往返后不变成浮点数。
- **其他**：`ItemPatch` 的字段同名同类型；各开放枚举都接受未知值。

另外：`cargo clippy --all-targets -D warnings` 和 `RUSTDOCFLAGS="-D warnings" cargo doc` 都通过，根配置没有改动，也没有新增依赖。

**5.12.5 中本任务负责的部分**

| 项 | 定义在 |
|---|---|
| 第 1 组 #4：`item.delta` 的 `field: text\|output`，合并键 `(item_id, field)`；`output_state` 的语义 | `item::ItemDelta.field: DeltaField`；`impl Mergeable for EventBody`；`item::OutputState` 的文档 |
| 第 1 组 #5：`request_digest` 用完整的 256 位、按 JCS 规范化 | `interaction::request_digest`（经 `canonical::digest`） |
| 第 1 组 #6：要批准的内容不截断，超长改 blob 并设 `content_truncated` | `PermissionRequest.content_truncated`；`PermissionTool.command: Option<Content>`；`detail: Option<Content>` |
| 第 2 组：时间字段 | `TurnStarted.started_at`、`TurnCompleted.ended_at`（另有 `duration_ms`）、`ItemStarted.started_at`、`ItemCompleted.ended_at`（另有 `duration_ms`），都是 `TimestampMs`/`DurationMs` |
| 第 2 组：`final_item_ids` 及判定规则；活动组的规则 | `TurnCompleted.final_item_ids: Vec<ItemId>`，规则写在字段文档里；`ItemStarted.group_id: Option<GroupId>`。判定由宿主实现 |
| 第 2 组：`ToolCall` 的 `query`、`url`、`parent_item_id`、`output_state`、`output_tail`、`output_bytes`，按 `tool_kind` 的必填字段表 | `item::ToolCall` 的同名字段；`ToolCall::missing_required` |
| 第 2 组：`late_tool_output`、`item.completed.may_still_run` | `Item::LateToolOutput(LateToolOutput {ref: ToolRef, output?, ...})`；`ItemCompleted.may_still_run`（只在为 true 时出现） |
| 第 2 组：`session.configured.project_config` | `SessionConfigured.project_config: ProjectConfig {loaded, ignored}` |
| 第 2 组：`session.configured.confirmation_required` | `ConfirmationRequired {reason: full_access_unconfirmed\|tier_above_setter, config_digest}`；另有队列暂停原因 `PauseReason::ConfigUnconfirmed` 和 `NoticeCode::ConfigUnconfirmed` |
| 第 2 组：`snapshot.recorded` 的 `stat`、`reason` | `SnapshotRecorded.stat: Option<DiffStat>`、`.reason: Option<SnapshotReason>` |
| 第 2 组：`Option.effect` 的 `unknown`、`tier_after`、`rule` | `OptionEffect.scope`（`"unknown"` 即 `Unclassified`）、`.tier_after: Option<Tier>`、`.rule: Option<EffectRule>`；`ends_turn: Option<bool>` |
| 第 2 组：新的 notice code | `item::NoticeCode`，共 9 个 |
| 第 2 组：注解 `x-id`、`x-time`、`x-path` | 新字段一律用 `ids` 的类型、`TimestampMs`/`DurationMs`、`WirePath` |
| 第 2 组 v1.4：`queue.updated` 条目的 `content` | `QueueEntry.content: PromptContent` |
| 第 2 组 v1.5：`user_message.reply_to`、排队条目的 `reply_to` | `UserMessage.reply_to`、`QueueEntry.reply_to`，都是 `Option<RequestId>` |

不在本任务、已由别处负责的部分：

- T011：`interrupt_stops_commands`、`trust_state`、`pending_kind`；
- T012、T013：各方法，包括 `project/trust`、`session/confirm_config`、`queue_edit`、`session/send.reply_to`、`git/diff` 的 `caveats`、`ui/navigate`、`host/status.pid`；
- T014：视图的序列化格式；
- T010：`conflict/config_unconfirmed` 等错误 reason（已有）。

两边合起来覆盖第 2 组全部。

## 提交列表

```
0a5bdc4 docs(task): record the T017 pr CI summary
585508d docs(task): record the T017 implementation notes
f8b9200 feat(protocol): add the session stream to EventBody and merge item.delta
d9d6920 feat(protocol): add interaction requests, option effects and request_digest
cb371bb feat(protocol): add the item union, ToolCall and ItemPatch
408baf7 feat(protocol): add the session stream's configuration, turn and queue data
a2a55c3 docs(task): add the T017 module plan and line estimate
```

## 遗留问题 / 范围外发现

1. **规模**：1,644 行，比上限多约 10%，超出全部在测试。
2. **需要编排者裁定的设计空缺**：
   - **权限请求的 `tool.paths` 超出预算时怎么办**，设计没写。
     - 建议：完整列表放进 `detail`（blob），`paths` 只保留前若干项，并设 `content_truncated`。
   - **`user_message` 的"（文本走 delta）"含义不清**：是宿主把长文本拆成 delta 下发，还是 `content` 始终完整？
     - 我目前只在文档里写了"文本也可能经 `item.delta` 到达"。建议在 5.3.5 写明。
   - **`ItemPatch` 没有"清空字段"的写法**：例如输出完成后 `output_tail` 不会被清掉，客户端按 `output_state` 判断即可。
     - 建议把这条规则写进 5.3.11。
3. **交接给后续任务**：
   - **T014**：
     - `apply` 的规则：`ItemPatch` 里出现的字段整体替换，不出现的保持不变。
     - `late_by_ref` 的键：线上是 `ref.item_id`，Rust 里是 `LateToolOutput.of.item_id`。
     - 工具调用的用时来自 `item.completed.duration_ms`。
     - 快照里的长文本按 `MAX_DELTA_ESCAPED` 拆成 `item.delta`。
   - **T012**：
     - `interaction/respond.outcome` 复用 `InteractionOutcome`；`interaction/get` 的状态复用 `InteractionStatus`（含 `pending`）。
     - 宿主核对摘要用 `request_digest`。它返回 `Result`，只在出现超过 2^53 的整数时失败。
   - **T013**：`git/diff` 复用 `DiffStat`、`ExcludedPath`、`SnapshotStatus`。
   - **T015**：
     - `ItemStarted` 的 schema 是自身属性加 `anyOf`（9 个成员）；`ItemUpdated` 展开 `ItemPatch` 后有 21 个属性。
     - `AnswerValue` 是不带标签的联合，schema 是 `anyOf`。
     - `Item`、`InteractionRequest`、`InteractionOutcome` 的兜底成员是 `UnknownMember`，后处理时用 `.loose()`。
   - **宿主的提取层**：`ToolCall::missing_required(ended)` 不为空时，把 `extraction` 降为 `partial`。
4. **会话流的"未知事件类型"没有单独写测试**：未知类型的处理和流无关，T011 的索引流测试（`project.pinned`）已经覆盖。本任务覆盖的是嵌套联合里的未知成员和未知枚举值。
