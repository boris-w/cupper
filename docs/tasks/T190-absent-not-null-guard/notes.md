# T190 工作笔记

## 开工前的核对

- 任务书；`handoff-notes.md` 第一列含 T190 的行（T016：变换在 `schema.rs` 的 `absent_not_null`，`CliPath` 是值本身可为 `null` 的具名定义）。
- 能拿到的样例值：`protocol/fixtures/` 的 `events.json`（事件，按 `Event` 解码再编码）、`methods.json`（每个方法的参数和结果，按各自的类型往返，`tests/fixtures.rs` 已有类型表）、`views.json`（Rust 视图写出的快照操作和视图，本身就是 Rust 的序列化结果）。
- 视图的 proptest 生成器（`view/props.rs`）是库内的 `pub(super)`，集成测试拿不到。

## 做法

- 新的集成测试 `tests/absent_not_null.rs`：样例按 Rust 类型往返后，沿导出的 schema（`schema::document()`，与 `v1.json` 同源）逐层走：遇到 `$ref` 进入定义，`anyOf` 选第一个对得上的分支（按类型、标签常量），对象逐个属性、数组逐个元素、映射逐个值。非必填属性的值是 `null`、而属性自己的类型不允许 `null` 时，记一次违反。`Unknown…` 定义不往下走。JSON-RPC 信封不在范围内（只检查 `params` 和 `result`）。
- 覆盖面：用不带 `absent_not_null` 的生成器再导出一次定义，"带 `Option` 的属性"就是其中非必填且可为 `null` 的属性；样例里出现过非 `null` 值的算覆盖到。

## 规模估计（开工前）

| 模块 | 估计 |
|---|---|
| 测试：读样例、往返（复用 `fixtures.rs` 的类型表要另写一份，集成测试之间不能共用） | 70 |
| 沿 schema 走、判 `null`、记覆盖 | 70 |
| 覆盖面：未变换的定义里找 `Option` 属性、输出未覆盖的 | 35 |
| 合计 | 约 **175** |

停止线 200，硬停止线 240。

## 按提交统计

| 提交 | `cargo xtask size` |
|---|---|
| 守卫（`tests/fixtures.rs`：类型表改成可传入检查函数、沿 schema 走、覆盖面） | +288 −14 |

写完才统计，已超过原来的停止线（200/240）；编排者裁定按方案一接受，停止线改为 310、硬停止线 330（计划 v1.3.61）。补上两处覆盖面的修正后仍是 +288 −14。

## 负面样例（不提交）

去掉 `AuthElevateResult.expires_at` 的 `skip_serializing_if`：守卫失败，报 `auth/elevate result.expires_at: null on optional AuthElevateResult.expires_at`。改回后通过。

## 覆盖面

"带 `Option` 的属性"：用不带 `absent_not_null` 的生成器再导出一次定义（方法目录、`Event`、两个视图、`RpcError`），其中非必填且可为 `null` 的属性，共 **220** 个。样例里出现过（有值，或者是允许的 `null`）的算覆盖到：**135 个（61%）**。没覆盖到的 85 个，按定义：

| 定义 | 没覆盖到 |
|---|---|
| `ItemUpdated` | 15 |
| `ItemCompleted` | 15 |
| `ToolCall` | 11 |
| `SettingsPatch` | 10 |
| `ErrorData` | 8 |
| `LateToolOutput` | 5 |
| `ProjectTrustInfoResult` | 2 |
| `Plan` | 2 |
| `NoticeParams` | 2 |
| `ErrorItem` | 2 |
| `TrustScan` | 1 |
| `ToolRef` | 1 |
| `ToolDiff` | 1 |
| `SteerOutcome` | 1 |
| `SlashCommand` | 1 |
| `RpcError` | 1 |
| `QueueMove` | 1 |
| `QuestionField` | 1 |
| `PlanEntry` | 1 |
| `InteractionGetResult` | 1 |
| `InitialConfig` | 1 |
| `ConfigValue` | 1 |
| `Choice` | 1 |

逐个属性：

- `Choice.description`
- `ConfigValue.description`
- `ErrorData.agent_code`
- `ErrorData.agent_message`
- `ErrorData.capability`
- `ErrorData.reason`
- `ErrorData.resolved_by`
- `ErrorData.retry_after_ms`
- `ErrorData.scope`
- `ErrorData.supported`
- `ErrorItem.agent_code`
- `ErrorItem.raw`
- `InitialConfig.reasoning_effort`
- `InteractionGetResult.reason`
- `ItemCompleted.command`
- `ItemCompleted.cwd`
- `ItemCompleted.diffs`
- `ItemCompleted.entries`
- `ItemCompleted.extraction`
- `ItemCompleted.markdown`
- `ItemCompleted.output_bytes`
- `ItemCompleted.output_tail`
- `ItemCompleted.parent_item_id`
- `ItemCompleted.paths`
- `ItemCompleted.query`
- `ItemCompleted.signal`
- `ItemCompleted.source`
- `ItemCompleted.tool_kind`
- `ItemCompleted.url`
- `ItemUpdated.command`
- `ItemUpdated.cwd`
- `ItemUpdated.diffs`
- `ItemUpdated.entries`
- `ItemUpdated.exit_code`
- `ItemUpdated.extraction`
- `ItemUpdated.markdown`
- `ItemUpdated.output`
- `ItemUpdated.parent_item_id`
- `ItemUpdated.paths`
- `ItemUpdated.query`
- `ItemUpdated.signal`
- `ItemUpdated.source`
- `ItemUpdated.tool_kind`
- `ItemUpdated.url`
- `LateToolOutput.exit_code`
- `LateToolOutput.output`
- `LateToolOutput.output_bytes`
- `LateToolOutput.output_tail`
- `LateToolOutput.signal`
- `NoticeParams.paths`
- `NoticeParams.reason`
- `Plan.entries`
- `Plan.markdown`
- `PlanEntry.priority`
- `ProjectTrustInfoResult.diff_omitted`
- `ProjectTrustInfoResult.findings_omitted`
- `QuestionField.choices`
- `QueueMove.before_queue_id`
- `RpcError.data`
- `SettingsPatch.agents.codex.cli_path`
- `SettingsPatch.agents.env_passthrough`
- `SettingsPatch.apps.editor`
- `SettingsPatch.apps.terminal`
- `SettingsPatch.diagnostics.verbose`
- `SettingsPatch.host.idle_exit_minutes`
- `SettingsPatch.notifications.needs_action`
- `SettingsPatch.notifications.turn_completed`
- `SettingsPatch.sessions.default_agent`
- `SettingsPatch.worktrees.root`
- `SlashCommand.input_hint`
- `SteerOutcome.turn_id`
- `ToolCall.command`
- `ToolCall.cwd`
- `ToolCall.diffs`
- `ToolCall.exit_code`
- `ToolCall.output`
- `ToolCall.output_bytes`
- `ToolCall.output_tail`
- `ToolCall.parent_item_id`
- `ToolCall.query`
- `ToolCall.signal`
- `ToolCall.url`
- `ToolDiff.hunks`
- `ToolRef.command`
- `TrustScan.findings_omitted`

- 现有的生成方法补不上：视图的 proptest 生成器（`view/props.rs`）是库内的 `pub(super)`，集成测试拿不到；类型没有"默认值加所有可选字段都填满"的值。编排者裁定：没覆盖到的属性归新任务 T193（按 schema 生成"所有可选字段都填满"的实例），排在 M1b 第一次附加式改契约的 T151 之前，T190 不做。

## `ci pr --record`

前两次各有一个与本任务无关的宿主测试在机器负载很高时失败（本任务只改了 `cupper-protocol` 的集成测试）：
- `trust::tests::unit_trust_scan_forking_directory_links_hit_the_entry_limit`：用墙钟断言扫描在 10 s 内结束，那次用了 23.4 s（负载约 16）；
- `it_lifecycle_host_exit_and_sigterm_exit_in_order`："Sigterm: no host/shutdown"（负载约 27）。

两个单独重跑都通过。第三次等负载降到 10 以下再跑，全部通过，摘要已提交。`license` 作业那次跳过了 `fetch-db`（作业本身通过）。
