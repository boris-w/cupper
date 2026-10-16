# SP-1 工作笔记

记录实测的环境、做法、每个场景看到的具体行为，以及中途的试错。结论和结果矩阵在汇报里。录制文件都在 `spikes/SP-1-adapter-behavior/recordings/<adapter>/`，下文简写为 `<adapter>/<文件名>`；提取出来的数字在同目录的 `data/summary.json`。

## 1. 环境与版本（2026-10-16）

| 项目 | 版本 |
|---|---|
| macOS | 26.4（arm64） |
| Node | v24.13.0 |
| `@agentclientprotocol/claude-agent-acp` | 0.91.0（当天 npm `latest`） |
| 其中的 `@anthropic-ai/claude-agent-sdk` / `@agentclientprotocol/sdk` | 0.3.300 / 1.8.0 |
| 本机 Claude Code（`CLAUDE_CODE_EXECUTABLE`） | 2.1.296 |
| `@agentclientprotocol/codex-acp` | 2.2.2（当天 npm `latest`） |
| codex-acp 自带的 `@openai/codex` | 0.160.1（下文记作 codex-bundled） |
| 本机 codex（`CODEX_PATH`） | codex-cli 0.159.2（下文记作 codex-local，比自带的旧一个小版本） |

两点本机环境的特殊情况，会影响结论的适用范围：

- **本机 Codex 没有用 ChatGPT 登录**，而是在 `~/.codex/config.toml` 里配置了一个自定义的 Responses API 网关（`model_provider = "custom"`）。所以 Codex 侧的认证状态显示为 `gateway`，模型目录也是网关提供的。网关在测试期间经常返回 "servers are currently overloaded"，于是录制里有不少重连事件，Codex 回合普遍比 Claude 慢。
- 两个 CLI 都会加载用户自己的配置：Claude 会话会拉起用户配置的 MCP 服务器和 hooks；Codex 会话会拉起用户配置的 MCP 服务器、插件和 `notify` 程序。录制和内存数据里都带着这些进程（脱敏后统一叫 `user-configured tool or MCP server`）。产品里如果"使用本机 CLI"，看到的就是同样的情况。

## 2. 做法

- 客户端是手写的换行分隔 JSON-RPC（TypeScript，Node 24 直接运行，不需要依赖）。没有用 SDK 的原因见 `spikes/SP-1-adapter-behavior/README.md`。
- 客户端声明的能力：elicitation（form + url）、unstable 的 `plan`、`session.configOptions.boolean`、`session.notices`、`session.compaction`、`auth.terminal`、`_meta.terminal_output`；`fs` 和 `terminal` 声明为不支持。
- 每个适配器配置（claude、codex-bundled、codex-local）各用一个新建的临时 git 仓库作为 `cwd`，仓库放在系统临时目录里，不在任何已有仓库中。
- 模型和推理强度：Claude 用 `haiku`、effort `low`；Codex 用 `gpt-5.6-luna`、`low`。权限模式：Claude 用默认的 `default`（Manual）和 `plan`；Codex 用 `workspace-write` 和 `read-only`，两者都比 codex-acp 的默认模式 `agent`（Auto review）严格。客户端每次发 prompt 前都会检查模式和模型，不满足就拒绝发送（录制里的 `safety-check` 标注）。
- 审批只回答 `allow_once` 或 `reject_once`。
- 场景编号与 brief 一致（s01–s12），另外补了几个：`s02b` 明确要求使用计划工具，`s02c` 和 `s02d` 是 Claude 的补充实验，`s04b` 拒绝命令审批，`s07b` CLI 进程在空闲时被杀后的恢复。codex-local 没有跑 `s02b`，因为 bundled 已经确认 `update_plan` 在这个环境下不可用，再跑一遍只是多花一个回合。

### 调用次数（大约）

按发出的 `session/prompt` 加上 steering 启动的新回合统计：

| 适配器 | 回合数 | 说明 |
|---|---|---|
| claude | 31（30 个 prompt 加 1 次 steering 启动的回合） | 含作废的第一次 s07（长 `sleep` 被 Claude Code 转到后台，改用循环后重跑）；s10 的 1 个 prompt 因未登录没有调用模型 |
| codex-bundled | 约 25（录制里有 21 个 prompt 加 1 次 steering 启动的回合） | 另有 3 个不在录制里的回合：第一次用 `gpt-5.4-mini` 时被中途停掉的 s03 和 s04（会话还留在 Codex 历史里），以及模型没有执行命令、作废的第一次 s04b |
| codex-local | 约 21（录制里有 19 个 prompt 加 1 次 steering 启动的回合） | 另有作废的第一次 s04b |

另外还有 CLI 自己的后台调用，录制里看不到，例如 Claude 生成会话标题、Codex 用 `gpt-5.6-luna` 生成标题，以及 Codex 遇到网关过载时的自动重试（单个回合最多录到 5 次重连）。总量是每个适配器几十次小模型调用。除了 s02d 为了观察思考流特意把 effort 调到 high 的那一个回合，其余回合都用的是最便宜的模型和最低档推理强度。

## 3. 逐个场景

### s01 能力、配置项和斜杠命令下发的时机、预建会话

录制：`*/s01-init-session.jsonl`

- **`initialize`**：
  - Claude 能力：`loadSession`；`promptCapabilities` 是 image 和 embeddedContext；`mcpCapabilities` 是 http 和 sse；`auth.logout`；`providers`；`sessionCapabilities` 有 list、resume、close、delete、fork、additionalDirectories。另外 `agentCapabilities._meta.claudeCode.promptQueueing: true`，以及顶层 `_meta.steering.supported: true`。客户端声明 `auth.terminal` 后，`authMethods` 是两个 terminal 方法（Claude 订阅、Anthropic Console）。
  - Codex 能力：同样一组，另外多了 `sessionCapabilities.subagents`；`mcpCapabilities` 是 `acp: false, http: true, sse: false`；顶层 `_meta.steering.supported: true`，没有 promptQueueing 标记。`authMethods` 是 api-key、chat-gpt、chat-gpt-device-code。
  - 两边都会在 `initialize` 响应之后，主动推一个 `_auth/status_update`。Claude 推的是 `{kind: "account", label: "Claude Max", account: {plan, email, organization}}`，Codex（自定义网关）推的是 `{kind: "gateway"}`。这个通知会带上**账号邮箱和组织**，宿主转发或记录时要注意（录制里已替换成占位符）。
- **配置项、模式和命令什么时候给出**：都在 **`session/new` 的响应里**一次给全（`configOptions` 和旧的 `modes`；Codex 还带旧的 `models`），不需要等第一个回合。`available_commands_update` 在响应之后 2–5 ms 到达。Claude 在每个回合开始时还会再推一次 `available_commands_update`。
  - Claude 的配置项：`mode`（5 个选项，含 `auto` 和 `bypassPermissions`）、`model`、`effort`（分类是 `thought_level`）。
  - Codex 的配置项：`mode`（4 个选项，含 `agent-full-access`）、`collaboration_mode`（default 或 plan）、`model`、`reasoning_effort`（分类是 `thought_level`）。
  - 两边都没有出现 boolean 类型的配置项。当前模型不支持 Fast mode，所以 Fast mode 这个选项没有出现。
  - 初始值来自用户自己的 CLI 配置（例如用户默认的大模型和高推理强度），所以新会话的默认值可能很贵。
- **预建会话会不会留下空会话**：两边都不会。
  - 新建会话后不发 prompt，Claude 不写 transcript（`~/.claude/projects/...` 里没有对应的 `.jsonl`），Codex 也不写 rollout 文件。
  - 在同一个进程里调用 `session/list`，以及重启适配器后再调用，都看不到这个会话。
  - 但预建并不是免费的：Claude 每个会话会立刻启动一个 CLI 进程（会话建立约 0.7–0.9 秒，内存见 s12）；Codex 会为每个线程拉起用户配置的 MCP 服务器。
  - 例外：Claude 会为每个会话建一个 `~/.claude/session-env/<id>/` 目录，空会话也有。

### s02 普通回合

录制：`*/s02-normal-turn.jsonl`；补充实验：`*/s02b-plan.jsonl`、`claude/s02c-plan-tasks-enabled.jsonl`、`claude/s02d-thinking.jsonl`

- **消息流**：两边都是 `agent_message_chunk`，带 `messageId`。
- **思考流**：
  - Codex 有 `agent_thought_chunk`（推理摘要，例如 "**Planning tool usage**"）。
  - Claude 在 haiku 下，不管 effort 是 low 还是 high，都**没有出现 `agent_thought_chunk`**（s02d）。适配器源码支持把 thinking 转成 thought chunk，没有出现的原因未核实。
- **工具调用**：
  - Claude 先发 `tool_call`（`status: pending`，标题是占位的 "Terminal"、"Preparing file…"），然后随着参数流式补全，发多次 `tool_call_update` 补上 `title` 和 `rawInput`，最后是 `completed` 或 `failed`。每个 update 都在 `_meta.claudeCode` 里带 `toolName`，完成时还带 `toolResponse`（原始结果）。
  - Codex 的 `tool_call` 出现时就已经是 `in_progress`，参数完整，之后直接是 `completed` 或 `failed`。
- **diff 的形状**：
  - Claude：每次 Edit 一个 `diff`。审批前的那次 update 里，`oldText`/`newText` 就是 `old_string`/`new_string`；完成后的那次是**带上下文的片段**，不是整个文件。`_meta.claudeCode.toolResponse` 里另外有 `structuredPatch`（hunk 列表）和 `originalFile`。新建文件时 `oldText: null`。
  - Codex：**每个 hunk 一个 `diff`**（一次改两处，就是两个 diff 条目），每个条目带 `_meta.kind`（add 或 update）。新建文件时 `oldText: null`。有时模型会改用 shell 运行 `apply_patch`，这时显示为 `execute` 类型的命令，**完全没有 diff**（codex-local/s05、codex-bundled/s05）。
  - 结论：两边都不给整个文件，也都可能拿不到 diff，回合级的变更视图只能靠宿主自己做 git diff。
- **计划**：
  - Claude 的 TodoWrite 在 SDK 会话里不可用（模型自己说的）。Task 系列工具（TaskCreate、TaskUpdate）也不可用，ToolSearch 找不到，加上 `CLAUDE_CODE_ENABLE_TASKS=1` 也一样（s02c）。所以**这次没有拿到 Claude 的 `plan` 更新**。
  - Codex 的 `update_plan` 在这个模型和目录下也不可用（模型说的，s02b）。
  - 两边能稳定拿到的只有 plan 模式下的 `plan_update`（unstable，Markdown 格式，见 s05）。
- **`usage_update`**：
  - Claude：每次调用模型后都发 `{used, size}`，回合结束时再发一次带 `cost: {amount, currency}` 的。`_meta` 里有 `_claude/model`，以及 `_claude/rateLimit`（5 小时和 7 天窗口的使用率、重置时间、overage 状态）。
  - Codex：只有 `{used, size}`。
  - `session/prompt` 的响应里两边都有 `usage`（Claude 有 cachedWrite，Codex 有 thoughtTokens），`_meta.quota` 里有按模型分的用量。
- **会话标题**：
  - Claude：回合结束后马上推 `session_info_update`，内容是 AI 生成的 `title` 加上 `updatedAt`。
  - Codex：先推一个由 prompt 原文截断而成的标题；约 5 秒后再推一个 AI 生成的标题（由 codex-acp 另外调用一次 `gpt-5.6-luna`）。
- **运行状态（Codex 独有）**：Codex 用 `session_info_update._meta.codex.threadStatus` 推送线程状态。录到的取值有：
  - `active`，`activeFlags` 为 `[]`、`["waitingOnApproval"]` 或 `["waitingOnUserInput"]`；
  - `idle`；
  - `systemError`。

  这实际上就是 v1 下的运行状态信号（R02 矩阵第 34 项原来写的是"仅 v2"）。Claude 没有对应的信号。

### s03 执行命令

录制：`*/s03-exec.jsonl`

- **Claude**：
  - `tool_call` 的 `content` 里有 `{type: "terminal", terminalId}`，`_meta.terminal_info` 里有 `terminal_id`。
  - 命令结束时一次性发 `_meta.terminal_output.data`，stdout 和 stderr 合在一起，第一行是 "Exit code 3"。然后发 `_meta.terminal_exit: {exit_code, signal}`，`status` 是 `failed`（退出码不为 0 时）或 `completed`。
  - **输出不会流式推送**：s07 里每秒输出一行的循环，直到结束都没有中间输出。
  - `ls` 这类只读命令不需要审批，`sh -c ...` 需要审批。
- **Codex**：
  - 一般命令同样有 `terminal_info`、`terminal_output` 和 `terminal_exit`（`exit_code` 是 3），`status` 是 `failed`。输出是 "err\nout\n"，stdout 和 stderr 合并。
  - 被归类为读取或列目录的命令（例如 `ls`）显示为 `kind: read`，标题是 "List files"。这类命令没有 terminal 相关的 meta，输出放在 `content` 文本里，退出码放在 `rawOutput.exit_code` 里。这是 codex-acp 对声明了 `terminal_output` 的客户端的约定。
  - Codex 会把 `sh -c '...'` 拆开，标题直接显示里面的命令。
  - **Codex 的输出是流式的**：s07 中每秒输出一行的循环，每行一个 `_meta.terminal_output`，内容是增量（"tick 2\n"、"tick 3\n"…）。但第一行 "tick 1" 没有出现，看起来工具调用开始之前产生的输出被丢掉了（codex-local/s07，只观察到一次）。
- **结论**：两边都能拿到输出和退出码，但位置不统一：Claude 在 `_meta.terminal_exit`；Codex 在 `_meta.terminal_exit` 或 `rawOutput.exit_code`。MS-3 需要一个按适配器区分的提取层。

### s04 审批（允许一次、拒绝一次）

录制：`*/s04-approval.jsonl`、`codex-*/s04b-reject-command.jsonl`

- **Claude**（default 模式，写两个文件）：
  - 第一个 Write 选"允许一次"，第二个选"拒绝"。被拒绝的 `tool_call_update` 是 `status: failed`，`rawOutput` 为 "User refused permission to run tool"，`_meta.claudeCode.nonExecutionKind: "permission-rule"`。
  - **回合继续**，模型说明 b.txt 没有创建，最后 `stopReason: end_turn`。
  - 选项（Write）：`allow-once`（allow_once）、`allow-with-updates`（allow_always，"Yes, allow all edits in claude/ during this session"，只在本会话有效）、`reject`（reject_once）。
  - 选项（Bash）：allow_always 的那一项是 "Yes, and don't ask again for `<cmd>` commands"，会写入持久规则。
  - 拒绝 Bash 命令（s04b）也一样：工具调用变为 `failed`，回合继续，模型说明没有执行，最后 `end_turn`。
- **Codex**（read-only 模式）：
  - 文件修改的选项：`allow_once`、`allow_for_session`（allow_always，"don't ask again for these files"）、`cancel`（reject_once，"No, and tell Codex what to do differently"）。
  - 命令的选项：`allow_once`、`accept_execpolicy_amendment`（allow_always，"don't ask again for commands that start with …"，持久的 exec policy）、`cancel`。
  - 不管是文件修改还是命令，**选 `cancel` 都会让整个回合以 `stopReason: cancelled` 结束**，模型没有机会回复。ACP 层没有提供"拒绝但继续"的选项。**#556 复现**，而且不是"有时"，是每次都这样。
- **对 AP-2 的影响**：两边都把"仅本会话"和"持久写入"的选项都标成 `allow_always`，只能从 `optionId` 或 `name` 区分。

### s05 结构化提问

录制：`*/s05-questions.jsonl`

- **Claude AskUserQuestion**：
  - 先发一个 `tool_call`（kind 为 other，标题 "Asking for your input"），然后发 `elicitation/create`，form 模式，带 `sessionId` 和 `toolCallId`。
  - `requestedSchema` 的每个问题是一个 `question_N` 字段，`oneOf` 列出选项（const 加 title 加 description），另外还有一个 `question_N_custom` 自由文本字段。
  - 回答 `accept` 后，工具完成，回合继续。
  - 源码显示，客户端如果没有声明 form elicitation，适配器会直接禁用 AskUserQuestion。
- **Claude 计划确认**（plan 模式）：
  - 模型把计划写进 `~/.claude/plans/<随机名>.md`（plan 模式下 Claude Code 的固有行为），然后调用 ExitPlanMode。
  - 对应的 `session/request_permission`：kind 是 `switch_mode`，标题 "Approve Plan"，有 5 个选项。只有 `exit-plan-default`（"Yes, manually approve edits"）是 allow_once。另外三个 allow_always 的选项会切换到 auto 或 bypass 模式，或者清空上下文。还有一个 `reject`。
  - 选了 `exit-plan-default` 之后，依次收到 `current_mode_update`（default）、`config_option_update`，以及 **`plan_update`**（unstable，`{type: "markdown", planId: "plan-mode", content}`）。然后在同一个回合里继续执行，Write 时再次请求审批。
- **Codex request_user_input**（plan 协作模式）：
  - `elicitation/create` 是 form 模式，字段由模型决定（这次是 `color` 和 `color_note`），线程状态变为 `waitingOnUserInput`。
  - 回答后，模型流式推了 3 次 `plan_update`（Markdown 格式，planId 是条目 ID）。
  - 然后发 `session/request_permission`，`toolCallId` 是 `plan-review:<id>`，kind 是 `switch_mode`，标题 "Implement this plan?"，选项是 `implement_plan`（allow_once）和 `revise_plan`（reject_once）。
  - 选择实施后，推 `config_option_update`（`collaboration_mode` 回到 default），并在**同一个 `session/prompt` 里**开始实施，最后才返回 `end_turn`。注意：发出计划确认请求的时候，线程状态已经是 `idle`。
- **两边都没有出现 URL 模式的 elicitation。**

### s06 回合进行中再发 prompt（排队）与 steering

录制：`*/s06-queue-steer.jsonl`

- **第二个 `session/prompt`（第一个回合正在执行 `sleep 10`）**：
  - Claude：第二条消息在下一次调用模型时被并入当前回合。第一个 prompt 在工具结束后立即返回 `end_turn`，usage 全是 0，**它自己的最终回复（"first-done"）从来没有生成**；第二个 prompt 回复 "second-done" 后返回。
  - Codex：第二个 prompt 接管了回合，回复 "second-done" 后返回 `end_turn`。**第一个 prompt 一直没有响应**，直到客户端 180 秒超时（bundled 和 local 都一样）。
  - 结论：两个适配器"运行中再发 prompt"的语义都不是"排到下一回合"。Claude 会丢掉前一条消息的回复；Codex 会让前一个请求永远挂起。CP-6 的排队必须由宿主实现，并且**绝不能并发发送 `session/prompt`**。
- **`_session/steering`**：
  - **Claude**：
    - 在第一个 `tool_call` 刚出现、参数还在流式生成时插话，返回 `{outcome: "injected"}`，耗时 1 ms。回合**没有失败**（**#1267 没有以"回合失败"的形式复现**）。但原来那个工具调用被丢弃了，一直停在 pending，直到回合结束才补发一个 `failed`。模型重新发起了一次同样的工具调用，最终回复里包含了插话的内容。
    - 在命令执行期间插话，也返回 `injected`。正在执行的 Bash 被**移到后台**，prompt 先以 `end_turn` 结束（模型说"还在后台运行，等它完成"）。命令完成后，Claude 自己发起了一个回合，"fourth-done STEERED2" 是在 **prompt 已经返回之后**才通过 `session/update` 推过来的。
    - 空闲时插话，带 `_meta.steering.idleBehavior: "promptRequired"`，返回 `{outcome: "promptRequired", reason: "noRunningTurn"}`，不启动任何回合。不带这个参数时返回 `startedNewTurn`，随后的输出没有对应的 prompt 请求。
  - **Codex**：
    - 两个时机都返回 `injected`，耗时 2–4 ms。工具照常执行完，最终回复包含插话内容，回合正常结束。
    - 空闲时**忽略 `idleBehavior` 参数**，直接返回 `startedNewTurn`，后面是一个没有对应 prompt 的回合。
- **对事件模型的影响**：两边都会出现**不属于任何 `session/prompt` 的 `session/update`**（后台任务完成后 Claude 自发的回合、空闲时 steering 启动的回合、Codex 在下一个回合里补发上一个回合遗留的工具更新）。宿主不能把"prompt 请求到响应之间"当作回合的边界。

### s07 取消

录制：`*/s07-cancel.jsonl`、`*/s07b-cli-crash-recovery.jsonl`；作废的第一次 Claude 运行：`claude/s07x-sleep-backgrounded.jsonl`

- **命令执行中取消**：
  - Claude：发出 `session/cancel` 后约 20 ms 返回 `stopReason: cancelled`。工具调用变为 `failed`，带 `terminal_exit`。**正在运行的命令被终止**（取消后的进程采样里已经没有那个循环）。
  - Codex：约 6–9 ms 返回 `cancelled`。但**命令没有被终止**：bundled 的 `sleep 30` 在取消后还在运行，约 30 秒后在下一个回合里推来了这个工具调用的 `completed`；local 的循环在取消后继续每秒推送输出，直到 30 行全部跑完才推 `completed`。
- **审批挂起时取消**：
  - Claude：发出 `session/cancel` 后，适配器**先给客户端发 `$/cancel_request`**，撤回挂起的那个审批请求，约 5 ms 后 prompt 返回 `cancelled`。
  - Codex：约 9 ms 返回 `cancelled`，**不会撤回审批请求**，客户端要自己丢弃。
  - 两边在取消之后都能正常进行下一个回合（"after-cancel"）。
- **CLI 进程在回合中途被杀（对应 #600）**：
  - Codex：杀掉 `codex app-server` 后，约 10 ms 内 prompt 返回错误：`{code: 1001, message: "The Codex app-server was killed by SIGKILL ... The agent starts it again on the next request.", data: {exitCode, signal, restartable: true}}`，message 后面还附带了 app-server 的 stderr 尾部。下一个 prompt 自动重启 app-server，同一个 sessionId 可以继续对话。**#600（永不返回）没有复现**。codex-local 这一步碰上网关连续重连 5 次，命令开始执行前客户端的 90 秒超时就先到了；随后杀掉 app-server，适配器仍然在 10 ms 内对这个请求补发了同样的 1001 错误。
  - Claude：杀掉 claude CLI 后，约 8 ms 内 prompt 返回 `-32603 "Internal error: The Claude Agent process exited unexpectedly. Please start a new session."`。之后的 prompt 返回 `-32603 ... "Session not found"`，**适配器不会自动重启**。
- **CLI 在空闲时被杀（s07b）**：
  - Claude：下一个 prompt 返回 "Session not found"；在同一个适配器进程里调用 `session/resume`（约 0.8 秒），之后就能继续对话。
  - Codex：下一个 prompt 透明地重启 app-server 并完成，客户端感觉不到。
- **试错**：Claude Code 会拒绝在前台长时间运行的 `sleep N`（模型原话是 "foreground sleep was blocked by the harness"），转而在后台运行，于是第一次 s07 的回合立即结束了，取消发生在回合之外。重跑时改用 `for i in $(seq 1 30); do echo tick $i; sleep 1; done`。codex-bundled 用的还是旧的 `sleep 30`，codex-local 用的是循环，两者结论相同。

### s08 会话列表

录制：`*/s08-list.jsonl`（不属于本次实测的会话已在写入磁盘前替换成字段结构）

- 两边返回的字段完全相同：`{sessionId, cwd, title, updatedAt}`，没有 `_meta`。
- **Claude**：一次返回本机全部 179 个会话，没有分页。只包含至少有一个回合的会话（空会话和未登录时建的会话都不在列表里）。按 `cwd` 过滤时，会把 `/var/...` 和 `/private/var/...` 视为同一个路径。
- **Codex**：本机 136 个线程，分 6 页，游标是 `updatedAt` 的 ISO 时间字符串。按 `cwd` 过滤时**必须传真实路径**：传 `session/new` 时用的 `/var/...` 会得到 0 条，换成 realpath（`/private/var/...`）才有结果。过滤后即使这一页是空的，也会返回 `nextCursor`，客户端必须继续翻页。
- 两边返回的 `cwd` 都是真实路径（`/private/var/...`），不是 `session/new` 时传入的路径。宿主做"会话属于哪个工作区"的匹配时，要先对路径做 realpath。

### s09 加载、回放、崩溃后恢复

录制：`*/s09-load-resume.jsonl`

- **`session/load` 回放**（加载 s02 的会话）：
  - 两边都在 `session/load` 响应**之前**推完全部回放。回放是压缩过的：每条消息只有一个 chunk，工具调用直接以最终状态出现，终端输出和退出码也在。不回放审批请求，也不回放 usage。
  - **两边的回放都没有任何时间戳**（Claude **#1307**、Codex **#617** 都复现）。
  - Claude 约 0.9 秒（包含启动 CLI），Codex 约 0.1 秒。
- **`/compact` 之后再加载**：
  - Claude：回放只剩 1 个 `compaction_update`（带完整摘要）加上最后一条 agent 消息，**压缩之前的历史全部丢失**（**#1297 复现**）。
  - Codex：回放仍然包含压缩前的全部历史，最后多一个 `compaction_update`。
  - 两边的 `/compact` 都以 `compaction_update`（in_progress → completed，completed 带摘要）加 `usage_update` 的形式出现，`stopReason` 是 `end_turn`。
- **SIGKILL 适配器后重启，再 `session/resume`**：两边都不回放（resume 期间没有任何 update），继续提问时模型记得之前创建的文件（"hello.txt"）。
- **大会话回放（Codex #516/#539）没有测**，因为需要一个很大的会话，成本不划算。

### s10 错误形态

录制：`*/s10-unauthenticated.jsonl`、`codex-bundled/s10x-model-unsupported.jsonl`

- **未登录**（用空的 `CLAUDE_CONFIG_DIR` 或 `CODEX_HOME`，没有碰真实账号）：
  - 两边都在 `initialize` 后推 `_auth/status_update {kind: "none", label: "Not logged in"}`。
  - Claude：`session/new` **成功**，`session/prompt` 返回 `-32000 "Authentication required"`。
  - Codex：`session/new` 直接返回 `-32000 "Authentication required"`。
- **模型不被支持**（第一次用 `gpt-5.4-mini` 时意外录到）：
  - 不是 JSON-RPC 错误。错误以 **`agent_message_chunk` 的文本**形式出现，内容是原始错误 JSON："The 'gpt-5.4-mini' model is not supported when using Codex with a ChatGPT account."。
  - 同时线程状态变为 `systemError`，`session/prompt` 照常返回 `end_turn`。
- **上游过载**（网关频繁出现）：
  - Codex 推 `session_info_update._meta.codex.error`，内容是 `{message: "Reconnecting... 1/5", additionalDetails: "stream disconnected before completion: Our servers are currently overloaded...", willRetry: true, codexErrorInfo, turnId}`，回合会自动重试。
  - Claude 没有遇到。
- **额度**：没有遇到额度用尽。Claude 的 `usage_update._meta._claude/rateLimit` 里一直带着 `status: "allowed"` 和窗口使用率，可以用来提前预警。

### s11 图片输入

录制：`*/s11-image.jsonl`

两边都接受 8×8 的纯红 PNG（`{type: "image", mimeType, data}`），都回答 "Red"。

### s12 进程拓扑与内存

录制：`*/s12-resources.jsonl`，运行中的采样在 s06 和 s07 里。数值是 RSS（MB），在不同时刻有波动。

| | Claude | Codex（bundled / local） |
|---|---|---|
| 进程结构 | 1 个适配器 node 进程，加上**每个会话 1 个 claude CLI 进程**，再加上每个会话各自的用户 MCP 服务器 | 1 个适配器 node 进程，1 个 codex 包装 node 进程，**1 个 app-server 服务所有会话**，再加上每个线程各自的用户 MCP 服务器 |
| 适配器空闲（没有会话） | 约 118 | 约 210（适配器约 84，包装进程约 47，app-server 约 79） |
| 每增加一个空闲会话 | CLI 约 220–290，加上用户 MCP（本机约 130–260） | app-server 增加约 10–45，加上用户 MCP（本机约 380–480） |
| 3 个空闲会话合计 | 约 1336 | 约 1540 / 1632 |
| 运行中（1 个会话） | 约 650–950（取决于当时拉起了哪些 MCP） | 约 560–745 |
| 3 个会话都 `session/close` 之后 | CLI 和 MCP 全部退出，回到约 80 | **用户 MCP 服务器没有退出**，内存基本不回落（1540 → 1540 / 1632 → 1585） |
| `initialize` 耗时 | 约 0.15 秒 | 约 0.1–0.3 秒；本机第一次启动约 4.3 秒 |
| `session/new` 耗时 | 约 0.7–0.9 秒（要启动 CLI） | 约 0.1–0.3 秒 |

- **关闭时的子进程**：
  - 关闭 stdin 后，Claude 适配器在约 5 ms 内退出，**不等子进程**。claude CLI 和 MCP 服务器会在大约 1 秒内因为 stdin 关闭而自行退出。`initialize` 时启动的 `claude auth status` 探测进程，如果适配器很快退出，也会短暂残留。
  - Codex 适配器关闭 stdin 后，大约 2 秒内带着 app-server 一起退出。
  - SIGKILL 适配器之后，两边的 CLI 都在 3 秒内退出，没有长期遗留的孤儿进程。用户的 `notify` 程序（Codex，回合结束时触发）会短暂存在。
- **NFR-11（6 个会话）的粗略估算**：只算 agent 本身，Claude 约 0.1 + 6 × 0.25 ≈ 1.6 GB，Codex 约 0.3 GB。如果用户配置了很多 MCP，这两个数都会成倍增加，而且 Codex 关闭会话后不释放 MCP。

## 4. 版本差异：codex-bundled 对比 codex-local

codex-local（0.159.2）比 codex-acp 锁定的版本（0.160.1）旧。在 s01–s12 里，两者的协议行为没有观察到差异：相同的能力、配置项、错误码，排队同样挂起，`cancel` 同样结束回合，回放同样完整。实验 API 的版本不匹配没有导致任何报错。版本耦合的系统性测试归 SP-2。

## 5. 试错记录

- `gpt-5.4-mini` 出现在模型列表里，但网关以"ChatGPT 账号不支持"为由拒绝了它。改用 codex-acp 生成标题时自己也在用的 `gpt-5.6-luna`。教训：模型列表里有的模型不一定能用，产品不能只靠列表判断。
- Claude Code 会把前台的长 `sleep` 转到后台（见 s07）。
- prompt 里如果写了"如果被拒绝就回复 rejected"，gpt-5.6-luna 会直接回复 rejected 而不调用工具（作废两次）。改成中性措辞后才正常触发审批。
- 第一版客户端判断"本次实测的会话"时，用的是传入的路径，没有考虑到适配器返回的是 realpath，导致第一次 s08 把自己的会话也当成别人的会话做了脱敏。修正后重跑。
- 第一版脱敏规则把 52 个字符的 `messageId` 也当成不透明 token 替换掉了，修正为只替换 64 个字符以上的 base64 或十六进制串。

## 6. 对用户配置的改动

- `~/.claude/settings.json` 和 `~/.codex/hooks.json`：实测前（15:29）做过 sha256，实测结束后再比对，结果一致。
- `~/.codex/config.toml`：**sha256 与实测前不同**，文件修改时间是 15:55:09。
  - 文件里**没有**本次实测任何临时目录的 `[projects."<tmp>/…"]` 信任条目：用临时目录名检索为 0 条；`[projects.*]` 条目共 35 个，其中位于系统临时目录下的 2 个在实测开始前就已经存在。
  - 15:55:09 那一刻，本实测只有 codex-bundled 的 s06 在运行，而且正处在等待第一个 prompt 超时的空闲期。16:06 之后又跑了 codex-local 的 s07–s12、s04b 和 s08，文件都没有再变化。
  - 同一时间本机还有其他写入者：Codex 桌面应用在运行，SP-4b 也在并行运行 codex。因为实测前没有保存这个文件的副本（只保存了哈希），**无法确定这次改动是谁写的、改了什么**。按编排者的要求，没有为了核对而复制或改动这个文件。
- codex-acp 唯一会写 Codex 配置的代码路径是 `_codex/hooks/trust`（`config/batchWrite`，写 `hooks.state`），本实测没有调用。
- Claude Code 在 plan 模式下写了 `~/.claude/plans/` 下的一个计划文件，这是数据文件，不是配置；`~/.claude.json` 里检索不到本次的临时目录。
- 没有选择任何 `allow_always` 选项，所以两边都没有写入权限规则。

## 7. 本机遗留

下面列出的会话 ID 和临时目录是为了让用户决定是否清理，所以保留了真实 ID；路径中的用户和临时目录部分已经写成占位符。完整的 ID 清单在汇报里。

- 临时目录（全部可以删除）：`<tmpdir>/cupper-sp1.0FUfZd/`，里面有 `adapters/`（npm 安装的两个适配器）、`repos/{claude,codex-bundled,codex-local}`（测试仓库）、`raw/`（未脱敏的录制）、`empty/`（未登录测试用的空配置目录，s10 的 Claude 会话写在这里；Codex 在 `session/new` 时就失败了，没有留下会话）、`terms.txt`（本地脱敏词表）。
- Claude（`~/.claude`）：
  - `projects/-private-var-folders-<…>-cupper-sp1-0FUfZd-repos-claude/`：13 个有回合的会话 transcript，外加一个空的 `memory/` 目录。
  - `session-env/<会话 ID>/`：17 个，空会话也有。
  - `plans/i-want-a-file-flickering-snowglobe.md`：s05 的 plan 模式产生。
  - 系统临时目录下 `claude-<uid>/-private-var-folders-<…>-repos-claude/`：后台任务的输出文件。
- Codex（`~/.codex`）：
  - `sessions/2026/10/16/rollout-*-<线程 ID>.jsonl`：24 个线程，其中两个来自被中途停掉的第一次 s03 和 s04。
  - state 数据库和 `session_index.jsonl` 里对应的索引和标题。
  - 这些线程也会出现在本机 Codex 桌面应用的历史里。
