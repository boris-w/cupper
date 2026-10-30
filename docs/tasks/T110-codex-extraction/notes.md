# T110 工作笔记

## 开工前的核对

录制：`testdata/recordings/codex-acp/2.2.2/` 下两个 CLI 版本，0.159.2 有 14 份，0.160.1 有 16 份（多出 `s02b-plan`、`s10x-model-unsupported`），header 的 `usage` 都含 `gold-a`。GOLD-A 的用例按场景名匹配，同名场景的两份录制由同一个用例覆盖（同 T053）。

读了 codex-acp 2.2.2 的产物（npm 下载到临时目录，校验了 integrity，不入库），与录制对照：

- **运行状态**：`thread/status/changed` 原样放进 `session_info_update._meta.codex.threadStatus`，`type` 有 `active`（带 `activeFlags`：`waitingOnApproval`、`waitingOnUserInput`）、`idle`、`systemError`、`notLoaded`。
- **错误**：客户端没有声明 AIR 的 `sessionFailure` 时，会重试的错误是 `_meta.codex.error {message: "Reconnecting... n/5", willRetry: true, ...}`；当前回合里不再重试的错误写成一段**不带 `messageId`** 的 `agent_message_chunk`（`readableServiceErrorMessage` 之后加两个换行），线程同时变为 `systemError`，`stopReason` 仍是 `end_turn`（s10x）。`usageLimitExceeded` 另外让 prompt 以 `-32603` 失败，`data.codexErrorInfo` 为 `usageLimitExceeded`。其他回合的错误是 `_meta.codex.error` 加 `willRetry: false`。
- **JSON-RPC 错误**：未登录 `-32000`；app-server 被杀 `1001`，`data.restartable`（崩溃保护触发后为 false）；会话不存在 `-32002` "Session not found: <id>"。`1001` 的 `message` 在换行之后附带 app-server 的 stderr（配置路径、MCP 服务器名），只取第一行。
- **命令**：`exec_command` 的 `commandActions` 只有一个读、搜索或列目录动作时不开终端：`kind: read` 或 `search`，标题是 "List files"、"Read file '<path>'"、"Search for ..."，读文件带 `locations`，**没有 `rawInput`**；结束时输出在 `content` 文本里，退出码在 `rawOutput.exit_code`。其余命令开终端：`kind: execute`，标题是去掉 `sh -c`、`zsh -lc` 外壳的命令，`rawInput {command, cwd}`，`_meta.terminal_info.cwd`；客户端声明了 `_meta.terminal_output`，输出逐段放在 `_meta.terminal_output.data`（每段都是新增内容，没流过的输出在结束时补一次），退出码在 `_meta.terminal_exit.exit_code`（被拒绝时为 null）；没有输出的命令结束时只有 `rawOutput.exit_code`（s06 的 `sleep 10`）。
- **文件修改**：`kind: edit`，标题 "Editing files"，没有 `locations`，每个 hunk 一个 `diff`，带 `_meta.kind`（`add`、`update`、`delete`；删除时 `newText` 为空）。审批请求的工具调用带 `locations`。模型用 shell 运行 `apply_patch` 时是普通的 `execute`，没有 diff（K24）。
- **计划确认**：`toolCallId` 为 `plan-review:<计划 ID>`、`kind: switch_mode`、`rawInput.plan`；选 `implement_plan` 后适配器给这个 ID 发一条 `tool_call_update {status: completed, rawOutput: "User approved the plan."}`，之前没有对应的 `tool_call`。
- **权限选项**：录制里出现的只有 `allow_once`、`allow_for_session`（文件修改）、`accept_execpolicy_amendment`（命令）、`cancel`、`implement_plan`、`revise_plan`，与 5.3.6 的表一致。源码里另有 `decline`（reject_once，不结束回合）、命令上的 `allow_for_session`、网络策略、权限档案、MCP 审批等选项，录制里没有，按 kind 兜底。持久选项的原文是 "Yes, and don't ask again for commands that start with \`<前缀>\`"。
- **提问**：`request_user_input` 的每个问题是一个 `oneOf` 字段（`isOther` 时多一个 "None of the above"）加一个 `<id>_note` 文本字段；答案里的 note 以 `user_note: ` 前缀追加到该问题的答案后面，与选了哪个选项无关。
- **配置项**：`mode`（category `mode`）、`collaboration_mode`（category `collaboration_mode`）、`model`、`reasoning_effort`（category `thought_level`）；模式 4 个取值与 5.6.5 一致（Q25）。旧的 `models`、`modes` 字段不读。
- **标题**：`session_info_update.title`，先是 prompt 截断的标题，回合结束后是 AI 标题（可能在下一个回合里才到）。
- **录制里 prompt 之外的更新**：只有 s07 取消之后补发的命令输出和 `completed`，以及 s10x 回合后的 AI 标题。`threadStatus: active` 从没在 prompt 之外出现过；取消之后线程立即 `idle`，命令仍在运行。

## 规模

### 开工前的估计

停止线 1000、硬停止线 1200，不含金样例和本笔记。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `driver/codex/mod.rs` | `CodexSession`：逐帧映射、`threadStatus`、`_meta.codex.error`、文本形式的错误、prompt 结果、错误分类 | 300 | — |
| `driver/codex/ask.rs` | 权限请求与计划确认的分类；提问沿用 Claude 的 `elicitation` | 40 | — |
| `driver/codex/options.rs` | 选项对照表、场景、前缀 | 80 | 70 |
| `driver/codex/capabilities.rs` | 能力、档位表 | 65 | 40 |
| A 层运行器 | 改为两种适配器共用（`git mv` 到 `driver/gold_a/`，只计改动） | 70 | — |
| Codex 的 A 层用例 | 场景表、匹配检查、工具字段、"取值都在表里"、运行状态与错误 | — | 200 |
| `tests/ids.toml` | T110 段追加其余录制的编号 | 20 | — |
| 小计 | | 575 | 310 |

另估 Claude 模块里与 agent 无关的部分搬到 `driver/` 下约 250 行，合计约 1,100，越过停止线，先报给编排者。

### 裁定

- 补充裁定一：选共用模块（方案 A），停止线 1150、硬停止线 1250；搬动单独成一个 refactor 提交，放在 Codex 的提交之前，Claude 的 21 份金样例逐字不变。
- 补充裁定二：搬动提交实际 +571 −447（我把共用的更新分支、elicitation、prompt 结果漏算了），按净增 +124 计入；Codex 部分单独算，停止线 750、硬停止线 850。
- 补充裁定三：Codex 部分第一版 +1,068，停止线改为 950、硬停止线 1050；不砍覆盖，收紧以不损可读性为限，分三个提交。

### 搬动提交 5c27a66 搬了什么

都是原样搬动，加上让 Claude 改用它们的胶水；`git diff --color-moved` 可以对照。

| 现在的位置 | 原来的位置 | 块 |
|---|---|---|
| `driver/extract.rs` 的 `Session` | `claude/mod.rs` 的 `ClaudeSession` | 每个会话记的工作目录、最近的 Markdown 计划、配置项；多了一个档位表字段 `tiers`。`set_config`、`config`、`last_plan`、`paths` 随之搬过去，新增 `cwd()` 访问器 |
| `Session::update` | `ClaudeSession::update` | 除 `tool_call`、`tool_call_update` 外的各分支：消息、思考、`plan`、`plan_update`、命令列表、`current_mode_update`、`config_option_update`、标题、用量、压缩 |
| `extract.rs` 的自由函数 | `claude/mod.rs` | `prompt_done`、`tool_update`、`tool_kind`、`tool_status`、`command`、`text`、`texts`、`content_text`、`wire_path`、`stat`、`wire`；模块文档里关于 K19、工具 kind、未知更新、回放的几条 |
| 同上 | `claude/capabilities.rs` | `config_options`、`tier`（档位表改为参数）、`set_mode`、`commands` |
| 同上 | `claude/ask.rs` | `elicitation` |
| `driver/options.rs` | `claude/options.rs` | `option()` 里与表无关的部分（改为 `options::option`，查表用的 `row` 作为参数，另加"拒绝是否结束回合"）、`kind`、`effect`、`by_kind`、持久选项的规则（`rule`）、`interpreter` |
| `driver/gold_a/mod.rs` | `claude/gold_a.rs` | `git mv`，只改了 import |

胶水：`ClaudeSession` 改为包着 `Session` 的新类型，Claude 的 `config_options`、`tier` 变成传入自己档位表的一行包装；运行时改从 `extract` 取 `prompt_done` 和 `elicitation`。

## 进度统计

| 提交 | 内容 | 行数 | 累计 |
|---|---|---|---|
| 5c27a66 | refactor：搬动 agent 无关的代码 | +571 −447（按净增 +124 计） | 124 |

Codex 部分（停止线 950、硬停止线 1050），从 5c27a66 起算，不含金样例和本笔记：

| 提交 | 内容 | 行数 | 累计 |
|---|---|---|---|
| 96b8428 | s05 录制里的临时路径（`testdata/` 不计；`spikes/` 下的两份原件 +72 −72，是数据） | +0 | 0 |
| 07a890d | Codex 的选项对照表、档位表、能力；`collaboration_mode` 映射为 `plan_mode` | +284 | 284 |
| 3cf5f16 | Codex 的提取层与交互请求 | +351 −2 | 635 |
| e329f74 | A 层运行器改为两种适配器共用；30 份金样例；Codex 的用例；`tests/ids.toml` | +390 −61 | 1,025 |

Codex 部分合计 +1,025 −63，在硬停止线 1050 以内（补充裁定三之后按现状提交，编排者同意）。第一版写完是 +1,068，收紧的地方：运行器的分派集中到一张描述表，内联录制改为逐行原文，diff 改为每个 hunk 一条，命令的 cwd 只读 `rawInput.cwd`，能力测试写紧凑。比开工前的估计多，主要是 rustfmt 展开之后的行数和测试。`cargo xtask size` 按总数报告（含搬动提交和 `spikes/` 的数据），会超过任务书上的线，这是预期的。

## 取舍

- **结构**：`driver/codex/` 照 Claude 分成 `mod.rs`（逐帧提取）、`ask.rs`（交互请求）、`options.rs`（选项对照表）、`capabilities.rs`（能力与档位表）。两边相同的更新交给 `extract::Session`，Codex 只处理工具调用、`session_info_update._meta.codex` 和不带 `messageId` 的消息。
- **运行状态**：`threadStatus` 的 `active` 带两个标记，`idle`、`notLoaded` 记为不活跃，`systemError` 记为不活跃加 `system_error`；不认识的 `type` 不出事件。T058 要求的 idle 由此提供。
- **上游重试**（K18）：`_meta.codex.error` 只在 `willRetry` 为真时出 `UpstreamRetry`，次数取消息末尾的 "n/m"，取不到时为 0/0，仍然出事件，让裁判知道 agent 还在忙。`willRetry` 为假的那种是别的回合的错误（源码：`turnId` 不是当前回合时才这样发），不出事件。
- **命令**：`execute` 的命令文本用标题（适配器已去掉 `sh -c`、`zsh -lc` 外壳），标题为空时退回 `rawInput.command`；cwd 取 `rawInput.cwd`，与 `_meta.terminal_info.cwd` 是同一个值，都没有时用会话的工作目录。输出：`_meta.terminal_output.data` 每段都是新增内容，记为 `Append`；读取类命令结束时的 `content` 文本记为 `Replace`。退出码先看 `_meta.terminal_exit`，再看 `rawOutput.exit_code`（读取类命令和没有输出的命令）。`kind: read` 的 "List files" 没有路径，合并器会把它记为 `partial`。
- **diff**：每个 hunk 一条 `ToolDiff`，与能力里的 `per_hunk` 一致，也和 Claude 一样不合并；`_meta.kind` 为 `delete` 时是删除，`add` 或没有旧文本时是新增，其余是修改。工具调用本身没有 `locations` 时，路径取 diff 的文件（去掉相邻的重复）。
- **工具状态**：Codex 的工具调用一出现就是 `in_progress`，参数完整，`tool_call` 的状态不另出事件（同 Claude）。
- **计划确认**：`plan-review:` 的请求按 5.6.4 归为 `plan_confirm`，计划正文取 `rawInput.plan`，没有时用最近一次 `plan_update`。选 `implement_plan` 后适配器给这个 ID 发的 `tool_call_update` 不出事件：之前没有对应的 `tool_call`，合并器会为它新建一个 `other` 条目，而这个确认已经是一个交互请求了。
- **JSON-RPC 错误**：`-32000` 为 `not_logged_in`；`1001` 看 `data.restartable`，真为 `restartable`，否则（崩溃保护触发）为 `process_exited`；"Session not found" 为 `session_not_found`；`data.codexErrorInfo` 为 `usageLimitExceeded` 时为 `quota_exhausted`（源码里有，录制里没有）；SDK 的 `incoming_transport_closed` 为 `adapter_exited`。`message` 只取第一行：`1001` 的消息在换行后附带 app-server 的 stderr，其中有配置文件路径和 MCP 服务器名。
- **选项**：表只收 5.3.6 的 Codex 各行；`decline`、命令上的 `allow_for_session`、网络和权限档案类的选项按 kind 兜底，`reject_once`、`reject_always` 兜底时也记为结束回合（5.3.6"Codex 视为 ends_turn"）。持久选项的前缀取原文反引号里的部分，取不到时取命令的第一个词。`implement_plan` 不带 `mode_after`：它切换的是协作模式，不是权限模式，档位不变。
- **配置项**：category `collaboration_mode` 映射为 cupper 的 `plan_mode`（5.6.5），写在共用的 `config_options` 里；Claude 没有这个 category，金样例不变。
- **能力**：`interrupt_stops_commands: false`（K3）、`reject_ends_turn: true`（K2）、`steer.side_effects: false`（SP-1 s06）、`output_streaming: true`、`diff: per_hunk`、`plan: entries`（`update_plan` 可用时有条目；SP-1 的环境里它不可用）、`run_state: agent_reported`。

## K17：文本形式的错误

做法：线程报 `systemError` 之后、下一次报 `active` 之前，不带 `messageId` 的 `agent_message_chunk` 不当作消息，记为这个 prompt 的错误文本；prompt 返回时（Codex 仍说 `end_turn`）改为 `PromptFailed`，`form: text`。文本是 OpenAI 的错误对象时取 `error.message`，否则取原文；含 "model" 和 "not supported" 时分为 `model_unsupported`，其余为 `other`。宿主现有的回合状态机据此以 `failed` 结束回合并带上 `error_code`；`error` 条目宿主还不写，交给 T112。

哪些帧会触发：
- 只有"`systemError` 之后"加"不带 `messageId`"两个条件同时满足的 chunk。
- 源码（2.2.2）里不带 `messageId` 的 chunk 都是适配器自己写的：当前回合不再重试的错误（`createErrorEvent`）、配置警告 "Config warning: …"、警告 "Warning: …"、客户端不支持 notices 和 compaction 时的 "*Context compacted…*"。模型的文本一律带它所在条目的 ID（`createTextEvent` 传 `itemId`）。
- 录制里只有 s10x 出现过 `systemError`，紧接着就是错误文本，之后是标题和 `end_turn`。

会不会误判：
- Codex 在 `systemError` 之后又正常输出：模型的文本带 `messageId`，照常是消息（内联用例里的 "model text"）。
- 下一个回合开始时线程报 `active`，状态清掉，之后不带 ID 的警告文本照常是消息（内联用例里的 "Warning: slow"）。
- 剩下的误判空间：`systemError` 之后、下一次 `active` 之前到达的警告或压缩提示，会被并进错误文本。回合已经失败，这只影响错误文本的内容，不影响回合的结局。
- 反过来，如果先来错误文本、后报 `systemError`，文本会当作普通消息，回合以 `completed` 结束。两条消息来自 app-server 的两个通知（`error` 和 `thread/status/changed`），适配器按到达的顺序转发；录制里是先报 `systemError`。T113 的兼容运行如果见到相反的顺序，再改成先缓存不带 ID 的文本。
- prompt 以 JSON-RPC 错误返回时（例如 `usageLimitExceeded` 同时发了文本又让 prompt 以 `-32603` 失败），以 JSON-RPC 错误为准；记下的文本在下一次 `active` 时清掉，不会带到下一个 prompt。

金样例：GOLD-A-codex-s10x（`prompt_failed`，`model_unsupported`，没有 `agent_text`），另有一份内联录制覆盖上面两种不误判的情形。

## Q31：要不要声明 JetBrains AIR

结论：**不声明**，最终回答按宿主规则（5.3.5）判定。

`phase` 只发给声明了 `clientCapabilities._meta.jetbrains.air` 的客户端（T102）。声明的代价：
1. **命令列表带 `skillPath`**：每一项带上用户 skill 和插件的本机路径。提取层的 `commands` 不读 `_meta`，进不了协议，但会进原始帧（`raw.db`）、诊断包和以后的录制；`tools/sanitize` 现在不处理 `skillPath`（T102 遗留第 1 条）。
2. **模式选项带 `kind`**：standard、auto_review、full_access，和档位一一对应。这一条本身无害，以后还可以用来给表外的新模式定档，但不足以抵消其他代价。
3. **工具调用改用 AIR 的渲染**：源码里 AIR 客户端不再走 `applyStandardFields`：命令审批请求的标题、kind、locations 不同；读取类命令的输出改放 `rawOutput` 而不是 `content`，退出码不再放进 `rawOutput.exit_code`；MCP 进度不发；提问表单去掉 "None of the above"，改用 AIR 的自定义答案标记。T102 只核对过 `exec_command`。SP-1、T102 的录制和这里的全部金样例都建立在非 AIR 的形状上，声明之后要用真实 CLI 重新录制、重新核对（T113）。
4. AIR 是 JetBrains 的私有扩展，适配器对它的处理在各版本之间变化很大，升级适配器时多一份核对。

好处有限：Codex 的过程说明（`commentary`）基本都在工具调用之前，宿主规则"最后一个过程条目之后的消息才是最终回答"已经把它们排除在外；只有模型在最后一个工具调用之后先说一段过程、再给最终回答时，宿主规则会把两段都算作最终回答。等兼容运行（T113）见到这种情形影响显示时，再考虑请上游在非 AIR 客户端也透出 `phase`，而不是声明 AIR。

## Q25

模式取值 4 个：`read-only`、`workspace-write` 为 `normal`，`agent` 为 `elevated`，`agent-full-access` 为 `full_access`，表外的取值按 `full_access`。30 份录制里的模式取值、`currentValue` 都在表里（GOLD-A-option-map-codex）。

## 交接事项的结论

- **T042，`isOther` 和 `<id>_note`**：不合成一题，保持现在按 JSON Schema 类型映射的两个字段。源码里 note 不只是"其他"的答案：无论选了哪一项，note 都以 `user_note: ` 前缀追加到这个问题的答案后面。合成为带 `allow_custom` 的一题，就没法在选项之外再附一句备注。现状下选 "None of the above" 再填 note 就能表达"其他"，字段的标题和说明都是 agent 给的原文（CP-4）。
- **T058，裁判与 threadStatus 的 idle**：提取层对 `idle`（和 `notLoaded`）发 `RunState { active: false }`。另一条"工具结束之后又来不带结束状态的更新"：GOLD-A-tool-fields-codex 检查了全部 30 份录制，工具调用以 `completed` 或 `failed` 结束之后都没有再来更新。s07 取消后补发的输出都在最后的 `completed` 之前。
- **T058 修复，空闲时的 thread active 要不要挡住出队**：现在不需要。30 份录制里，prompt 之外从没出现过 `threadStatus: active`。取消之后线程立即 `idle`，命令仍在运行（s07），所以 `active` 也覆盖不了 K3 的晚到输出。从源码看，线程只在跑回合时才是 `active`；prompt 之外的回合只可能来自空闲时插话（宿主不这样做，K11）或 Codex 自己的后台任务，它们一有输出就会开自发回合挡住出队，剩下的空隙只在"报 active"到"第一条输出"之间。如果 T113 的真实运行里见到 prompt 之外的 `active`，再在裁判里把它当作输出处理（T112 的范围）。
- **T055，第二种 agent 要自己的 `ActorKind`**：本任务不拉起 Codex 的 `AgentRuntime`，转给 T111。
- **T002，其余录制的编号**：在 T110 段追加了 GOLD-A-codex-s01、s02b、s03、s07b、s08、s09、s10、s11、s12。两个 CLI 版本里同名场景的录制由同一个用例覆盖。
- **T102，Q31、Q25**：见上两节。`revise_plan` 按实测记为 `plan_revise`、结束回合。

## 范围外

- K24 的"条目 `extraction: partial`"：`extraction` 由合并器按 `tool_kind` 的必填字段判定，用 shell 运行 `apply_patch` 的条目是 `execute`，命令、cwd、退出码都有，所以是 `full`。提取层只能做到"没有 diff"（GOLD-A-codex-s02、s05）。要标 `partial`，得在合并器里识别 `apply_patch`，或者改 5.6.9 K24 的写法，交编排者定。
- cupper 的 `initialize` 没有声明 `session.notices` 和 `session.compaction`（SP-1 的客户端声明了），所以真实运行时 Codex 的压缩提示、配置警告会以消息文本出现，而不是 `compaction_update` 或 `notice`。这影响显示，不影响本任务的提取逻辑，留给 T111、T113 核对。

## 录制里没脱敏的临时路径

Codex s05 的最后一句回答带一个 Markdown 链接，指向本机的每用户临时目录（`/var/folders/<两段随机名>/T/cupper-sp1.<随机>/repos/codex-local/color.txt`）。路径被切成了约 35 个 `agent_message_chunk`，逐帧扫描扫不出来；金样例把同一条消息折叠成一行之后，`pii-scan` 报出了 "per-user temporary path"。用户名没有出现，但这是本机标识。

经编排者同意，改了全部 4 个副本：`"(/"` 那一块改为 `"(<tmp>"`，其后到 `"/repos"` 之前的 35 块改为空串，帧数和 `seq` 不变，折叠后是 `(<tmp>/repos/codex-local/color.txt)`，与其余录制的写法一致。

| 文件 | 改动的帧（seq） |
|---|---|
| `testdata/recordings/codex-acp/2.2.2/0.159.2/s05-questions.jsonl` | 77–112 |
| `testdata/recordings/codex-acp/2.2.2/0.160.1/s05-questions.jsonl` | 80–115 |
| `spikes/SP-1-adapter-behavior/recordings/codex-local/s05-questions.jsonl` | 77–112 |
| `spikes/SP-1-adapter-behavior/recordings/codex-bundled/s05-questions.jsonl` | 80–115 |

找副本的办法：把 `testdata/`、`spikes/`、`docs/` 下所有 JSONL 里同一条消息的文本折叠起来，再搜本机路径的前缀（`/var/folders/`、`/private/var/`、`/Users/`、`/home/`、`/tmp/`），只有这 4 个文件命中；`docs/tasks/SP-1-adapter-behavior/data/summary.json` 里没有这条消息。改完重新生成了两份 s05 金样例，用 `tools/sanitize` 的 `scan` 和主仓库的敏感词清单扫了 `testdata/`（全部录制和金样例）和 `spikes/SP-1-adapter-behavior/recordings/`，共 181 个文件，都干净。
