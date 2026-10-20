# T053 工作笔记

## 开工前的核对

- Claude 的 18 份录制里，适配器发来的帧有这些：`session/update` 的 `agent_message_chunk`、`tool_call`、`tool_call_update`、`usage_update`、`available_commands_update`、`session_info_update`、`compaction_update`、`config_option_update`、`current_mode_update`、`plan_update`、`user_message_chunk`（`session/load` 回放），以及 `_auth/status_update`、`session/request_permission`（12 次）、`elicitation/create`（1 次）、`$/cancel_request`（1 次）、各类响应（124 次）。SDK 3.3.0 加上已开的 `unstable_plan_operations` 能解析以上全部更新类型；不认识的更新类型解析失败，落到 K27 的兜底处理器，正好是"只进原始帧、不产生事件"。
- Claude 的命令参数是流式补全的：`tool_call` 出现时 `rawInput` 为空、标题是 "Terminal"，命令文本在随后的 `tool_call_update` 里。所以 `NormUpdate::ToolUpdate` 要能带命令和路径，合并器也要跟着应用。
- 工具调用里没有 cwd，`ToolStart` 的 `cwd` 取会话的工作目录，提取层需要会话上下文（cwd、最近一次 `plan_update` 的 Markdown、当前的配置项）。
- 合并器在工具开始时按 `missing_required(false)` 定 `extraction`，但它不保存完整的 `ToolCall`；T017 交接的"结束时 `missing_required(true)` 不为空就降为 `partial`"要在合并器里记下已有的必填字段。
- `compaction_update` 在录制里出现过，但 SP-1 的客户端声明了 `session.compaction`；宿主不声明时适配器还推不推，录制回答不了，要等兼容剧本（T064/T066）核对。

## 规模估计（开工前）

按格式化后的写法估。T052 的经验：格式库的实现比估计多约 30%，其余持平。金样例是数据，不计入。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `driver/api.rs` | `Ask` 加 `tool_call_id`，`ToolStart` 加 `cwd`，`ToolUpdate` 加命令和路径；金样例输出用的 `Serialize` | 40 | — |
| 合并器 | 应用 `cwd` 和 `ToolUpdate` 的新字段；结束时按 `missing_required(true)` 降为 `partial`；改现有用例里的构造 | 40 | 65 |
| `driver/claude/extract.rs` | 会话更新（上面 11 种）、工具字段（命令、路径、kind 映射与兜底、状态取 K19 原值、"Exit code N" 去行、退出码、diff）、prompt 结果与错误分类、登录状态 | 415 | — |
| 交互请求 | permission 与 plan_confirm 的分类、`PermissionRequest` 的构造、计划正文、form 模式的 elicitation | 120 | — |
| `driver/claude/options.rs` | 选项对照表（按工具区分 `allow-with-updates`、`exit-plan-*` 的 `mode_after`/`tier_after`）、按 kind 兜底、未知 kind、持久选项的前缀与 `prefix_risk` | 170 | 80 |
| 配置项与档位 | `configOptions` → `ConfigOption`（按 category 映射、档位表、表外取值按 `full_access`） | 90 | 40 |
| 能力提取 | `initialize` 和会话响应 → `SessionCapabilities` | 100 | 60 |
| A 层运行器 | 读录制、按段跟踪请求 ID 与方法、经 K19 改写和 SDK 解析后调提取层、规范化（delta 折叠等）、金样例读写与 `UPDATE_GOLDEN`、每份录制一个用例、"每份录制都有金样例" | 285 | — |
| 检查与 GOLD-A 用例 | "取值都在表里"（GOLD-A-option-map-claude）、GOLD-A-tool-fields-claude、GOLD-A-unknown-enums | — | 160 |
| xtask | CI 里设置了 `UPDATE_GOLDEN` 就失败 | 10 | 10 |
| 合计 | | 1,270 | 415 |

合计约 **1,690 行**，超出约 1,500 行的上限约 13%；按 T052 的偏差，实际可能在 1,800–1,900 行。按 brief 和守则第 8 条，开工前停下来汇报。

### 拆分建议

brief 的拆法（第一部分：提取层、运行器与两项检查；第二部分：选项对照表、配置项与能力提取）有两处依赖要调整：

- 提取层交出权限请求时，选项必须带 `effect` 和 `requires`。第一部分先用 5.3.6 里"按 kind 兜底"和"kind 未知"两行（`allow_always` 一律持久、需要原生确认），这是保守的；第二部分换成按工具区分的对照表，重新生成 s04、s04b、s05 等金样例。
- "取值都在表里"要等表存在，放到第二部分；"每份录制都有金样例"留在第一部分。

| 部分 | 内容 | 估计 |
|---|---|---|
| 第一部分（T055 依赖） | api 改动、合并器、提取层、交互请求（选项按 kind 兜底）、配置项按 category 透传、A 层运行器、"每份录制都有金样例"、GOLD-A-tool-fields-claude、GOLD-A-unknown-enums、xtask | 约 1,150 |
| 第二部分 | Claude 的选项对照表与前缀风险、档位表、能力提取、"取值都在表里"（GOLD-A-option-map-claude）、UNIT-option-map-claude、UNIT-capabilities、UNIT-capabilities-plan，重新生成受影响的金样例 | 约 600 |

## 编排者裁定（2026-10-20）

拆分：T053 做 api 改动、合并器、提取层、交互请求（选项按 kind 兜底）、配置项按 category 透传、能力提取、A 层运行器与"每份录制都有金样例"、xtask 的 `UPDATE_GOLDEN` 检查；上限约 1,400 行，硬停止线 1,500。按工具区分的选项对照表、档位表和"取值都在表里"归 T095。

## 进度统计

不含金样例（`testdata/golden/`，18 个文件）和本笔记。

| 提交 | 内容 | 行数 | 累计 |
|---|---|---|---|
| 1 | `api.rs`（`Ask.tool_call_id`、`ToolStart.cwd`、`ToolUpdate` 的命令和路径、`Serialize`）；合并器保存每个工具调用的字段、结束时定 `extraction`（测试 52 行） | +137 −26 | 137 |
| 2 | 提取层 `driver/claude/`：`mod.rs` 368、`ask.rs` 108、`capabilities.rs` 146；开 SDK 的 `unstable_end_turn_token_usage` | +623 −1 | 760 |
| 3 | A 层运行器 `gold_a.rs` 478（运行器约 360，tool-fields 与 unknown-enums 约 120）、能力的两个单测 77、`k19::rewrite` 改为 `pub(crate)`、`ids.toml` 追加 10 个编号 | +567 −1 | 1,327 |
| 4 | xtask：设置了 `UPDATE_GOLDEN` 就拒绝运行 | +18 | 1,345 |

合计 +1,345 −28，在 1,400 行的上限以内。

## 实现里的取舍

- **提取层的形状**：`ClaudeSession` 每个会话一份，持有工作目录、最近一次 Markdown 计划和当前配置项；`update()` 把一条 `session/update` 变成零到多个事件（`tool_call` 带 diff 时先 `ToolStart` 再一条带 diff 的 `ToolUpdate`）。权限请求、form 模式的 elicitation、prompt 结果、错误分类和能力是各自的函数，`AskId` 和 `PromptTag` 由运行时（以及运行器）分配。
- **工具字段**：`ToolStart` 的 `cwd` 只给 `execute` 类，是会话工作目录的绝对路径；`paths` 和 diff 的路径在工作目录内时写成相对路径。没有新信息的 `tool_call_update`（只有 `_meta.claudeCode.toolResponse` 之类）不产生事件。非命令工具的文本内容（Read 的文件内容等）作为整段输出。diff 的行数统计去掉首尾相同的行后计数，`hunks` 不填（回合的改动以 git 为准）。
- **K19 的原值**：未知的 `stopReason` 原样下发；未知的工具状态不改变状态；未知的选项 kind 原样下发，`effect.scope` 为 `unknown`、`persistent`、`requires: privileged`。工具的 `kind` 不在 K19 的字段里，SDK 自己把不认识的值变成 `other`。
- **用量**：开了 SDK 的 `unstable_end_turn_token_usage` 特性，`PromptDone` 才能带上 Claude 在 prompt 响应里给的 token 数；`Cargo.lock` 不变。
- **能力**：`fork` 需要 SDK 的另一个 unstable 特性，P1 才做，先给 `false`；`thinking` 按适配器支持给 `true`（录制里 haiku 没有产生思考）；`file_ref` 给 `true`（`resource_link` 是 ACP 的基线）。
- **配置项**：只映射单选项，按 category 定 `id`（`mode` → `permission_mode`，`model`，`thought_level` → `reasoning_effort`，其余用 agent 的原 ID）；在档位表（T095）之前，`permission_mode` 的每个取值的 `risk` 都是 `full_access`，其余 `normal`。
- **金样例的规范化**：同一条消息（或连续的思考）的文本折叠成一条；对象的键排好序（工作区里的 serde_json 开了 `preserve_order`，所以显式调用 `sort_all_objects`）；ID、路径、时间都保留录制里的值。登录状态只记 `kind` 和套餐名（K16）。
- **`UPDATE_GOLDEN`**：`cargo xtask ci` 的任何作业在环境里看到它就拒绝运行。

## 没做的和留下的

- **T088 的交接**（`SessionCore::write_held` 填 `item_id`）：这个分支从 main 开出时 T088 还没合并（`write_held` 不在分支上），没法做。`DriverEvent::Ask` 已经带上 `tool_call_id`，合并 main 之后可以接。
- **compaction**：录制里有 `compaction_update`，提取层照常处理；宿主不声明 `session.compaction` 时适配器推不推，留给兼容剧本（T064、T066）。
- **命令列表很大**：金样例约 1 MB，大部分是每次 `available_commands_update` 的完整列表；去重是运行时的事（设计 5.6.2），提取层逐帧照出。
