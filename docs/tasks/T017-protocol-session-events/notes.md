# T017 工作笔记

## 行数估计（开工前）

### 估算方法

T011 的估计偏低约 40%，原因是文档注释和开放枚举比预想的多。这次直接按 T011 `index.rs` 的实际密度折算：15 个结构体、16 个开放枚举、约 60 个字段，共 342 行。由此得到：

| 写法 | 行数 |
|---|---|
| 结构体本身（derive、文档、括号、空行） | 约 4.5 行 |
| 字段（必填约 2 行，可选约 3 行） | 平均约 2.3 行 |
| `open_enum!` | 约 6 行，每个取值另加 1 行，带文档的取值再加 1 行 |

测试按 T011 的实际情况单独估算：T011 的测试估计 150 行，实际写了 475 行。

### 按模块估计

| 模块 | 内容 | 类型数（结构体 / 联合 / 枚举） | 代码 | 测试 |
|---|---|---|---|---|
| `session.rs`（新） | 会话流里条目和交互请求以外的事件：`session.configured`（`project_config`、`confirmation_required`）、`workspace.*` 3 种、`turn.started`/`completed`（时间字段、`final_item_ids`、用量、`error_code`）、`queue.updated`（条目带 `content`、`reply_to`、`created_by`）、`snapshot.recorded`（`stat`、`reason`）、`usage.updated`；`DiffStat`、`ExcludedPath` 供 T013 的 `git/diff` 复用 | 16 / 0 / 11 | 320 | 20 |
| `item.rs`（新） | `item.started`/`delta`/`updated`/`completed`；条目联合（`kind` 作标签，8 种）；`ToolCall`（16 个字段）及按 `tool_kind` 的 `missing_required`；`late_tool_output`；`plan`、`notice`、`error`；共用的 `ItemPatch`（约 20 个可选字段） | 16 / 1 / 11 | 470 | 60 |
| `interaction.rs`（新） | `interaction.requested`/`resolved`；请求联合（`permission`、`question`、`plan_confirm`、`auth_url`）；选项与 `effect`（`unknown`、`tier_after`、`rule`，`ends_turn` 为 `Option<bool>`）；回答联合与 `AnswerValue`；`request_digest` | 14 / 2 / 6 | 330 | 50 |
| `event.rs` 修改 | `EventBody` 加 16 个会话流变体、`stream_kind()` 分类、`Mergeable`、`x-mergeable` | — | 75 | 200 |
| `lib.rs` | 3 个 `mod` 声明 | — | 3 | — |
| **合计** | | | **约 1,200** | **约 330** |

测试（UNIT-proto-events-session）的 330 行：每种事件、每个条目和请求成员的往返样例约 120 行；未知类型、未知成员、未知枚举值约 40 行；残缺数据在严格和宽容两种模式下的结果约 40 行；按 `(item_id, field)` 合并及上限约 40 行；`x-mergeable` 只在 `item.delta` 上约 15 行；`request_digest` 约 35 行；`missing_required` 约 40 行。

**总计约 1,530 行，按 ±10% 算在 1,380 到 1,680 行之间**，贴着 1,500 行的上限，但不算明显超出，所以开工。控制办法：

- 文档注释只写字段名看不出含义的地方；测试样例写紧凑；
- 每完成一个模块统计一次；按已完成模块的实际偏差推算，总数预计超过约 1,650 行时，停下来汇报。

T011 拆分时对本任务的估计是约 1,100 行。多出来的部分主要是 `ItemPatch`、`ToolCall` 的可选字段（每个约 3 行），以及按 T011 实际情况上调的测试。

## 实际行数

按守则第 8 条，每完成一个模块统计一次（`git diff --shortstat main..HEAD -- crates/cupper-protocol`）：

| 完成的模块 | 新增 | 其中测试 | 估计（代码 + 测试） | 累计新增 |
|---|---|---|---|---|
| `session.rs` | 312 | 20 | 340 | 313（含 `lib.rs` 1 行） |
| `item.rs` | 545 | 123 | 530 | 859 |
| `interaction.rs` | 387 | 89 | 380 | 1,247 |
| `event.rs` 修改 | 397 / 删 27 | 335 / 删 26 | 275 | **1,644 / 删 27** |

- 前三个模块完成时累计 1,247 行，与估计的 1,250 行基本一致，按剩下的 `event.rs` 推算约 1,520 行，没有到 1,650 行的停止线，于是继续。
- `event.rs` 比估计多 120 行，全部在测试：会话流的样例表（每种事件、8 种条目、4 种请求、4 种回答，各带有无可选字段的变体）约 115 行，比估计的多一倍。
- 最终新增 1,644 行、删除 27 行，代码 1,077 行（估计 1,200），测试 567 行（估计 330）。比 1,500 行的上限多约 10%，超出部分都是测试。

## 实现时的取舍

**形状**

- `item.started` 把条目联合展开在外壳字段旁边：`{item_id, turn_id?, started_at, group_id?, kind, ...}`。`group_id` 放在外壳上，因为组内的 `thought` 也要带它，不只是 `tool_call`。schema 是外壳的属性加上 9 个成员的 `anyOf`（8 种条目和兜底）。
- `duration_ms` 放在 `item.completed` 上，各类条目都必填，不作为 `ToolCall` 的字段：只有结束时才知道。与 `turn.completed` 的写法一致，思考条目也能显示用时。
- `ItemPatch` 有 20 个可选字段，覆盖 `tool_call`、`late_tool_output`、`plan`、`notice` 可以更新的字段；出现的字段整体替换，不出现的保持不变，没有"清空"的写法。`user_message`、`error` 和各条目的固定字段（`ref`、`notice.code`）不在里面。有一条测试检查各类条目的可更新字段都能按同名同类型解析为 `ItemPatch`。
- `late_tool_output` 另带 `output_tail?`、`output_bytes?`：它的输出按普通工具输出的规则内联或转为 blob（5.2.6），截断时的这两个字段也会经 `ItemPatch` 更新到它上面。
- 权限请求的 `tool.command` 是 `Content`，不是字符串：第 1 组第 6 条要求超长的命令改为 blob，不截断。`tool.paths` 仍是路径列表，超出预算时怎么办设计没有写（见"遗留问题"）。
- 选项的 `requires` 直接用 `Scope`，取值 `approve`、`privileged`。
- `effect.scope` 和 `prefix_risk` 的线上取值 `unknown` 是已知取值，变体名分别是 `EffectScope::Unclassified`、`PrefixRisk::Unparsed`，因为 `open_enum!` 已经占用了 `Unknown` 这个变体名。有测试确认 `"unknown"` 解析为已知变体，不落进兜底。
- `request_digest`：`turn_id` 不出现时，从对象里省略这个键，不写 `null`，与线上的写法一致。
- `notice.params` 必填，写成带类型的 `NoticeParams {attempt?, max?, paths?}`，没有参数的码就是 `{}`。哪个码用哪些参数写在 `NoticeCode` 的文档里。
- `session.configured.config` 是 `BTreeMap<String, String>`，键是能力描述里配置项的 `id`。
- `AnswerValue` 是不带标签的联合：布尔、数字（`serde_json::Number`）、字符串、字符串数组，`null` 和对象都不接受。
- `effect.rule.location` 是字符串，不是 `WirePath`：适配器给的写入位置是 `localSettings` 这类语义值。
- 供 T012、T013 复用：`InteractionOutcome`（`interaction/respond.outcome`）、`InteractionStatus`（含 `pending`，供 `interaction/get`）；`DiffStat`、`ExcludedPath`、`SnapshotStatus`（`git/diff` 的 `stat` 和 `snapshot`）。

**设计只列了字段或写了"等"的取值**

| 类型 | 取值 |
|---|---|
| `Guidance` | `retry`、`run_login_in_terminal`、`upgrade_cli`、`install_cli`、`quota_exhausted`、`change_model`、`restart_agent`、`none` |
| `StopReason` | ACP 的 `end_turn`、`max_tokens`、`max_turn_requests`、`refusal`、`cancelled`，加宿主的 `host_timeout` |
| `SnapshotReason` | `timeout`、`unsafe_repo_config`、`not_a_repo`、`git_failed` |
| `ExclusionReason` | `too_large`、`nested_repo`、`large_untracked_dir`、`filter_unsupported` |
| `ResolutionReason`（`interaction.resolved.reason`） | `withdrawn`、`turn_cancelled`、`adapter_exited`、`host_restarted` |
| `PlanPriority` | ACP 的 `high`、`medium`、`low` |
| `NoticeCode` | 5.3.5 列出的 9 个 |
| `workspace.preparing.progress` | 百分比，0–100 的整数 |
