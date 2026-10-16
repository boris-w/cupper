# R02 汇报

- 执行者：子 agent（技术调研员，协议方向）
- 分支：`research/R02-acp-adapters`
- 完成日期：2026-10-16

## 做了什么

梳理了 ACP（Agent Client Protocol）规范的现状、官方 SDK、Claude Code 和 Codex 两个 ACP 适配器的实际能力，以及 Codex 官方客户端使用的 app-server 协议，并对照"复刻 Codex UI"的需求整理出缺口和对架构的启示。

调研方法：

- 把四个仓库克隆到 worktree 之外的临时目录，只读源码，没有执行：ACP 规范仓库、`claude-agent-acp`、`codex-acp`、`openai/codex`（只稀疏检出 app-server 相关目录）。能力是否支持，以 JSON Schema、`initialize` 返回的能力、方法注册代码为准，README 只作辅助。
- 用脚本从 schema 中提取方法清单、`session/update` 变体，以及 stable 和 unstable 两套 schema 的差异；从 `common.rs` 按 `#[experimental]` 标记给 app-server 的方法分类。
- 用 `gh` 拿发布记录、open issues 和仓库元数据（改名、迁移、归档、许可证）。用 WebFetch 和 WebSearch 读官方文档（Claude Agent SDK、Codex app-server、Zed）。
- 来源编号、版本、日期和原始摘录都在同目录的 [`sources.md`](./sources.md)。正文用 `[Sx]` 引用来源。

标注约定：

- **已核实**：直接读到了源码、schema 或官方文档原文。
- **二手**：只拿到搜索摘要或第三方报道（原文抓取返回 403）。
- **推测**：根据已核实的事实推断出来的结论。

没有特别标注的事实都属于"已核实"。

### 核心结论（先看这里）

1. **ACP v1 是稳定版**（协议版本 1，Schema v1.25.0，2026-10-09），会话列表、加载、恢复、关闭、删除、配置项（模型、模式、推理强度）、用量、压缩、通知都已稳定。**v2 还是草案**（2.0.0-alpha.8），官方要求实现方用版本协商加功能开关来隔离 v2。[S1][S3][S4]
2. **标准传输只有 stdio**。远程传输（Streamable HTTP + WebSocket）的 RFD 还处于 Active 状态，TypeScript 和 Rust SDK 已经实现了它，但 v1 下断线期间的消息不会补发。[S2][S6][S8]
3. **两个适配器都已迁到 ACP 官方组织**，几乎每天发版。Claude 适配器：`zed-industries/claude-code-acp` 先改名为 `claude-agent-acp`，再迁到 `agentclientprotocol/claude-agent-acp`。Codex 适配器：Zed 原来的 Rust 版 `zed-industries/codex-acp` 已归档，由 JetBrains 发起的 TypeScript 新实现 `agentclientprotocol/codex-acp` 接替，内部实际是在驱动 Codex app-server。[S9][S12][S13]
4. 对"Codex 风格 UI"来说，**排队、插话（steering）、worktree、回合级汇总 diff、回滚、多端同时观察、运行状态（idle/running）都没有 ACP v1 标准**。这些能力要么靠适配器私有扩展（`_session/steering`、`_meta.claudeCode.promptQueueing`、JetBrains AIR 扩展），要么需要我们自己实现。[S7][S9][S12]
5. Codex app-server 原生提供的能力远多于 ACP：服务端队列、turn 汇总 diff、文件搜索、文件系统、命令执行、多连接订阅、经 chatgpt.com 中继的远程控制和配对等。但它的 WebSocket 传输被官方标为"实验、不支持生产"，大量方法需要 `experimentalApi`，也没有语义化版本承诺。[S15][S16]
6. **政策风险**：Anthropic 不允许第三方产品提供 claude.ai 登录（除非事先获批），产品也不能以 "Claude Code" 命名。这会直接影响 PRD 里的定位（Q1）和账号方案（Q7）。[S11]
7. 架构上的建议：**ACP 只作为"接 agent 的南向接口"**；桌面端内置一个会话宿主（Session Host），统一事件模型和同步协议，桌面 UI 和将来的移动端都走这个协议。这样接入移动端只是增加一种传输，而不是推倒重来。详见「对架构的启示」。

---

## 1. ACP 规范现状

### 1.1 版本与稳定性

- **协议版本**：stable 为 `1`，在 `initialize` 的 `protocolVersion` 字段中协商。仓库 README 明确说，不要用 crate 或 schema 的发布版本号推断线上兼容性。[S1]
- **发布节奏**：Schema v1.x 大约每 1–2 周一版（v1.25.0，2026-10-09）；Rust schema crate 1.11.0 与它同日发布。从 0.14.0 到 1.0.0 是在 2026-06-24。[S1][S4]
- **v2 草案**：2026-07-20 公布，现为 2.0.0-alpha.8。官方原话是 "various pieces can, and will, change before stabilization"，并要求实现方同时支持 v1 和 v2。[S3]
- **演进机制**：新功能走 RFD 流程（Draft → Active → Preview → Completed），只有 Completed 才算稳定承诺。RFD 未完成的功能以 **unstable schema** 的形式发布，SDK 中以 `unstable_` 前缀暴露。[S5]
- **治理**：Zed 担任项目牵头方（BDFL）。两位 Lead Maintainer 分别是 Ben Brandt（Zed）和 Sergey Ignatov（JetBrains），JetBrains 也深度参与。[S1 `MAINTAINERS.md`][S4]
- **许可证**：Apache-2.0。另外有一个实验性的兼容性测试套件 `acp-tck`。[S1][S8]

### 1.2 传输与生命周期

- **传输**：JSON-RPC 2.0，消息以换行分隔，跑在 stdio 上。客户端把 agent 作为子进程启动，stdout 上只能出现 ACP 消息。规范允许自定义传输。[S2 transports]
- **远程传输**：Streamable HTTP & WebSocket 的 RFD 处于 Active 状态（由 JetBrains、Block 的维护者推动），设计是一个 `/acp` 端点同时支持 POST + SSE 和 WebSocket upgrade，客户端必须支持 cookie 以保证会话亲和。按 RFD 原文，**v1 下断线期间的消息不补发**，补发、续传和心跳要等 v2。TS SDK 已有 `ws-stream`、`server-sse` 等实现，Rust SDK 有 `agent-client-protocol-http` crate，但在规范层面还不是稳定传输。[S6][S8]
- **生命周期（v1）**：
  1. `initialize` 协商版本，交换能力和实现信息。
  2. 需要时调用 `authenticate`。
  3. 用 `session/new`、`session/load`（回放历史）或 `session/resume`（不回放）建立会话。
  4. `session/prompt` 开始一个回合，期间 agent 推送 `session/update`，按需发起 `session/request_permission` 或 elicitation，客户端可以发 `session/cancel`。
  5. `session/prompt` 的响应带回 `stopReason`，表示**本回合结束**。

  [S2 overview, prompt-turn, session-setup]
- **v2 草案的生命周期变化**：
  - `session/prompt` 在用户消息被"插入"会话时就返回 `messageId`，不再等回合结束。
  - 回合是否结束改由 `state_update`（`running` / `idle` / `requires_action`）表示。
  - `session/update` 可以在回合之外随时发送。
  - 草案在设计上为排队、插话、多客户端回放留了空间，但**排队和插话本身仍不规定**。

  [S3][S7]

### 1.3 方法与通知能力清单

稳定性取值：stable 指已进入 v1 stable schema；unstable 指只在 `schema.unstable.json` 中、对应 RFD 未完成；draft 指只在 v2 草案中。完整原文见 sources.md 摘录 A、B。

**Client → Agent（agent 实现的方法）**

| 方法 / 通知 | 作用 | 稳定性 | 能力位 / 备注 |
|---|---|---|---|
| `initialize` | 协商协议版本，交换能力、`clientInfo` 和 `agentInfo` | stable（基线） | — |
| `authenticate` | 协议内认证（agent 自己处理的登录方式） | stable | `authMethods` |
| `logout` | 结束认证状态 | stable（2026-05-21） | `agentCapabilities.auth.logout` |
| `session/new` | 新建会话：`cwd`、`mcpServers`、`additionalDirectories` | stable（基线） | `additionalDirectories` 自 2026-06-01 起稳定 |
| `session/load` | 加载会话，并通过 `session/update` 回放全部历史 | stable | `loadSession` |
| `session/resume` | 恢复会话，不回放历史 | stable（2026-04-22） | `sessionCapabilities.resume` |
| `session/list` | 列出会话，可按 `cwd` 过滤，用游标分页；返回 `title` 和 `updatedAt` | stable（2026-03-09） | `sessionCapabilities.list` |
| `session/delete` | 从会话列表中删除 | stable（2026-06-05） | `sessionCapabilities.delete` |
| `session/close` | 关闭活动会话：取消正在进行的工作并释放资源 | stable（2026-04-23） | `sessionCapabilities.close` |
| `session/prompt` | 发送用户消息（ContentBlock 数组）；响应即回合结束，带 `stopReason` | stable（基线） | 图片、音频、嵌入资源由 `promptCapabilities` 控制 |
| `session/cancel`（通知） | 取消当前回合 | stable（基线） | — |
| `session/set_mode` | 切换模式 | stable，但文档说明**将被移除**，改用配置项 | `modes` |
| `session/set_config_option` | 设置配置项：select 或 boolean；分类有 `mode`、`model`、`model_config`、`thought_level` | stable（2026-02-04；boolean 2026-07-06；`model_config` 2026-06-24） | boolean 需要客户端声明 `session.configOptions.boolean` |
| `session/fork` | 基于现有会话分叉出新会话 | unstable（RFD Draft） | `sessionCapabilities.fork` |
| `providers/list` / `providers/set` / `providers/disable` | 由客户端配置 LLM 提供方或网关 | unstable（RFD Draft） | `agentCapabilities.providers` |
| `mcp/message` | MCP-over-ACP：客户端通过 ACP 连接向 agent 提供工具 | unstable（RFD Draft） | `mcpCapabilities.acp` |
| `nes/*`、`document/did*` | 编辑器"下一编辑建议" | unstable（RFD 标为 To be removed） | 与本项目无关 |
| `$/cancel_request`（双向通知） | 按 JSON-RPC id 取消任意未完成的请求 | stable（2026-06-29） | — |

**Agent → Client（客户端实现的方法）**

| 方法 / 通知 | 作用 | 稳定性 | 能力位 / 备注 |
|---|---|---|---|
| `session/request_permission` | 请求用户授权一次工具调用；选项分为 `allow_once`、`allow_always`、`reject_once`、`reject_always` | stable（基线） | v2 草案改为带 `title`、`description` 和可扩展 `subject` 的结构 |
| `session/update`（通知） | 流式推送会话更新，变体见下表 | stable | — |
| `fs/read_text_file`、`fs/write_text_file` | agent 通过客户端读写文件 | stable；**v2 草案删除** | `clientCapabilities.fs` |
| `terminal/create`、`terminal/output`、`terminal/wait_for_exit`、`terminal/kill`、`terminal/release` | agent 请求客户端创建并管理终端 | stable；**v2 草案删除** | `clientCapabilities.terminal` |
| `elicitation/create` | 向用户请求结构化输入（表单模式，或 URL 模式用于 OAuth 等） | stable（2026-07-22） | `clientCapabilities.elicitation.{form,url}` |
| `elicitation/complete`（通知） | URL 模式的交互已完成 | stable | — |
| `mcp/message` | MCP-over-ACP | unstable | — |

**`session/update` 的变体**

| 变体 | 作用 | 稳定性 |
|---|---|---|
| `user_message_chunk`、`agent_message_chunk`、`agent_thought_chunk` | 用户消息、agent 回复、思考过程的流式片段；可选 `messageId` 用于分组 | stable（`messageId` 2026-06-05） |
| `tool_call`、`tool_call_update` | 工具调用：`kind`、`status`、`content`（文本、`diff` 或 `terminal`）、`locations`（用于跟随）、`rawInput`、`rawOutput`、`name` | stable（`name` 2026-09-17） |
| `plan` | 执行计划：条目带 `content`、`priority`、`status`，每次发全量替换 | stable |
| `available_commands_update` | 可用的斜杠命令 | stable |
| `current_mode_update` | 当前模式变化 | stable（随 modes 一起将被淘汰） |
| `config_option_update` | 配置项变化 | stable |
| `session_info_update` | 会话标题、更新时间等元数据 | stable（2026-03-09） |
| `usage_update` | 上下文窗口占用（`used`、`size`）和累计费用 | stable（2026-06-05） |
| `notice` | 不进入历史的用户提示（toast 一类） | stable（2026-10-09），需要客户端声明 `session.notices` |
| `compaction_update`、`compaction_summary_chunk` | 上下文压缩事件及其摘要 | stable（2026-10-09），需要客户端声明 `session.compaction` |
| `plan_update`、`plan_removed` | 多计划，支持条目、文件、Markdown 三种格式 | unstable（plan-operations RFD Draft） |
| `subagent_update`、`session_message`、`session_message_chunk` | 子代理作为子会话展示 | unstable（subagents RFD Draft） |
| `state_update`、`user_message`、`agent_message`、`tool_call_content_chunk`、`terminal_update`、`terminal_output_chunk` | v2 的运行状态、按 ID upsert 的消息、流式工具内容、agent 自有的展示终端 | draft（仅 v2） |

**其他机制**：

- **扩展**：任何带 `_meta` 字段的对象都能附加自定义数据（`traceparent` 等键保留给 W3C trace）；以 `_` 开头的方法名留给自定义请求和通知；自定义能力在 `initialize` 中声明。v2 进一步允许枚举值使用 `_` 前缀扩展。[S2 extensibility][S3]
- **斜杠命令**：agent 通过 `available_commands_update` 公布可用命令，执行时客户端把 `/cmd args` 作为普通文本放进 `session/prompt`。[S2 slash-commands]
- **MCP 配置**：所有 agent 必须支持 stdio 类型的 MCP 服务器；http 和 sse 是可选能力（sse 已被 MCP 标为废弃）。[S2 session-setup]
- **认证方法类型**：`agent`（默认，走 `authenticate`）和 `terminal`（客户端在交互式终端里用附加参数重新运行 agent 程序完成登录，需要客户端声明 `auth.terminal`，2026-08-20 起稳定）。另有 `auth/status` 查询（get-auth-state RFD）仍是 Draft。[S2 authentication][S4][S5]
- **Agent Registry**：2026-03-09 稳定。各 agent 以 `agent.json` 描述分发方式（npx、二进制等），客户端可以据此一键安装。[S4][S18]

### 1.4 尚未进入标准、但与本项目相关的 RFD

| RFD | 状态 | 与本项目的关系 |
|---|---|---|
| streamable-http-websocket-transport | Active | 移动端远程连接的潜在标准；v1 不补发断线期间的消息 |
| v2 系列（prompt 生命周期、消息 upsert、diff 改版、权限请求、终端输出等） | Active（草案） | 事件模型更适合多端同步；v2 不规定排队和插话 |
| session-fork | Draft | 分叉会话 |
| plan-operations | Draft | 多计划、Markdown 计划（对应 Codex 的 plan 模式） |
| subagents | Draft | 子代理分栏展示 |
| client-system-prompt | Draft | 客户端注入系统提示 |
| custom-llm-endpoint（providers） | Draft | 自带网关 |
| get-auth-state | Draft | 查询登录状态 |
| end-turn-token-usage | Draft | 每回合 token 用量 |
| diff-delete | Draft | diff 表示删除文件 |
| proxy-chains、mcp-over-acp | Draft | 代理链、通过 ACP 注入工具 |

[S5]

## 2. 官方 SDK

| SDK | 包名 | 最新版本（日期） | 成熟度 | 许可证 |
|---|---|---|---|---|
| TypeScript | `@agentclientprotocol/sdk` | v1.8.0（2026-10-09） | 2026-06-25 达到 1.0；v1 为稳定入口，v2 需从 `experimental/v2` 显式导入；已带 HTTP、SSE、WebSocket 传输实现。两个适配器都基于它 | Apache-2.0 |
| Rust | `agent-client-protocol`（另有 `agent-client-protocol-schema`、`-http`、`-conductor` 等 crate） | v3.3.0（2026-10-09） | 2026-06-25 达到 1.0，之后已发到 3.x；Zed 用它；`-http` crate 同时提供 HTTP+SSE 和 WebSocket 的客户端与服务端 | Apache-2.0 |
| Python | `agent-client-protocol` | 0.12.1 稳定版（2026-08-16）；1.0.0rc3（2026-10-12） | 1.0 尚在 RC 阶段 | Apache-2.0 |
| Kotlin | `com.agentclientprotocol:acp` | v0.33.0（2026-10-09） | 0.x；目前只支持 JVM | Apache-2.0 |
| Java | `java-sdk` | v0.80.0（2026-10-08） | 0.x；带 Spring AI 示例 | Apache-2.0 |

社区还有 Go、Swift、Dart、C#、C++ 等实现，均非官方。[S8][S1 docs/libraries]

## 3. Claude Code 适配器：`agentclientprotocol/claude-agent-acp`

### 3.1 身份、改名与迁移

| 时间 | 事件 |
|---|---|
| 2025-08-27 | 以 `zed-industries/claude-code-acp`（npm `@zed-industries/claude-code-acp`）创建 |
| 2026-02-17（v0.17.0） | 改名为 `@zed-industries/claude-agent-acp`，原因是要符合 Anthropic 的品牌规范 |
| 2026-03-26（v0.24.0） | 迁到 ACP 官方组织，改名为 `@agentclientprotocol/claude-agent-acp`，"以便多个团队共同维护"；旧的 GitHub 地址会重定向到新仓库 |

[S9 CHANGELOG][gh api]

- **当前版本**：v0.91.0（2026-10-15）。两周内从 0.85 发到 0.91，变更非常频繁。[S9]
- **维护方**：Registry 中的作者是 Anthropic、Zed Industries 和 JetBrains；`package.json` 的 author 是 Zed Industries；源码中大量是 JetBrains AIR 的扩展。[S18][S9]
- **安装与运行**：`npx @agentclientprotocol/claude-agent-acp`（也是 Registry 采用的分发方式），要求 Node ≥ 22；只支持 stdio。[S9 package.json, serve.ts][S18]
- **依赖**：`@anthropic-ai/claude-agent-sdk` 0.3.300。SDK 以平台 optional dependency 的形式自带原生 Claude Code 二进制，可用 `CLAUDE_CODE_EXECUTABLE` 指定其他二进制。会话保存在 Claude Code 自己的存储中（`listSessions` 走 SDK），因此能看到 CLI 创建的会话。[S9 acp-agent.ts `claudeCliPath`, `listSessions`]
- **许可证**：适配器代码是 Apache-2.0；Claude Agent SDK 受 Anthropic Commercial Terms 约束，npm 上标为 "SEE LICENSE IN README.md"。Registry 条目把 license 写成 "proprietary"，与仓库不一致，原因是**推测**：反映了 SDK 的条款。[S9][S11][S18]

### 3.2 能力（以 `initialize()` 和方法注册代码为准）

- **协议**：默认 v1。设置 `CLAUDE_AGENT_ACP_EXPERIMENTAL_V2=1` 后会按客户端请求的版本选择 v1 或 v2。在 v2 下，steering 和 AIR 扩展暂不可用。[S9 index.ts, README]
- **能力位**：
  - `loadSession`
  - `promptCapabilities`：`image`、`embeddedContext`（没有 audio）
  - `mcpCapabilities`：http、sse
  - `auth.logout`
  - `providers`（unstable）
  - `sessionCapabilities`：list、resume、close、delete、fork（unstable）、additionalDirectories

  [S9]
- **认证**：
  - 客户端支持 terminal auth 时，提供 "Claude Subscription"（`claude auth login --claudeai`）和 "Anthropic Console"（`--console`）两种 terminal 方法。远程环境下改为提供 `claude /login`。
  - 客户端声明 `auth._meta.gateway` 时，提供自定义网关方法（Anthropic 或 Bedrock 协议，传入 baseUrl 和 headers）。
  - ACP 层**没有"直接输入 API key"的方法**。API key 由 Claude Code 自己从环境变量（`ANTHROPIC_API_KEY`）或 `apiKeyHelper` 读取（依据 `hide-claude-auth.ts` 的注释）。
  - 启动时加 `--hide-claude-auth` 参数会隐藏订阅登录，并拒绝任何会消耗订阅额度的会话。

  [S9]
- **配置项**：
  - 模式：Manual、Accept edits、Plan、Auto、Bypass permissions
  - 模型
  - Effort（`thought_level`）
  - Fast mode（`model_config`；客户端支持时为 boolean 类型）

  同时保留旧的 `session/set_mode`。[S9 session-mode.ts, session-effort.ts]
- **会话更新**：
  - 消息和思考的流式片段；
  - 工具调用，带 locations（用于跟随）和 `rawInput`；
  - TodoWrite 转换为 `plan`；
  - `usage_update`、`session_info_update`（标题）、`notice`、`compaction_*`；
  - `available_commands_update`，包含自定义斜杠命令和 `/mcp`。

  [S9]
- **diff**：Edit 和 Write 工具生成 v1 的 `diff`（oldText/newText 为全文）。对 AIR 客户端可改发 git patch；v2 下为结构化 diff。[S9 diff.ts]
- **终端**：**不使用** ACP 客户端的 `terminal/*` 和 `fs/*`。Bash 命令和文件读写都由 Claude Code 自己执行（源码中找不到对客户端 fs 方法的调用）。命令输出的展示方式：如果客户端声明了 `_meta.terminal_output`，按 Zed 的约定用 `_meta.terminal_info`、`terminal_output`、`terminal_exit` 发送；否则以代码块形式放进内容。[S9 tool-calls/renderer.ts, air-extensions.md「Zed conventions」]
- **审批**：`session/request_permission` 支持"允许一次"、"总是允许"（会写入规则）和"拒绝"。AskUserQuestion 和 MCP OAuth 走 elicitation。[S9]
- **私有扩展**：
  - `_meta.claudeCode.promptQueueing: true`：回合进行中可以再次调用 `session/prompt`，新 prompt 进入 Claude Code 的队列。
  - `_session/steering`：把消息插入正在运行的回合。
  - `_session/async_task/stop`：停止某个后台任务。
  - `_session/goal`：长期目标，按客户端身份只对 AIR 开放。
  - `_auth/status_update`：推送登录身份。
  - `_claude/sdkMessage`：可选的原始 SDK 消息透传。
  - `session/new` 的 `_meta.claudeCode.options`：直接透传 SDK 的 Options。
  - 会话改名、归档、列表订阅（`_session/rename`、`_session/archive`、`_session/list/subscribe` 等）只对声明了 AIR sessionIndex 的客户端开放。

  [S9]
- **子代理**：客户端声明 `clientCapabilities.subagents`（unstable）后，每个子代理以独立子会话的形式出现，支持单独取消。[S9 README]

### 3.3 已知限制（open issues 节选，截至 2026-10-16）[S10]

- #1297：`session/load` 只回放最近一次压缩之后的历史，更早的内容会丢失。
- #1307：回放的消息不带时间戳。
- #1285：普通 ACP 客户端无法知道后台任务是否仍在运行。
- #1269：PostToolUse 钩子会等待 `session/update` 送达；**客户端读得慢会卡住会话**（每个工具最长 600 秒）。这一点对远程和移动端尤其重要。
- #1267：工具调用仍在流式输出时 steering 会让回合失败；#1280：自主回合（task notification）中无法 steering。
- #1110：`session/fork` 返回的会话不是活动会话。
- #1343：会话中 CLAUDE.md 的 @-import 没有展开。
- #1236：goal 扩展按客户端身份而不是能力开放。

## 4. Codex 适配器：`agentclientprotocol/codex-acp`

### 4.1 身份、改名与迁移

| 时间 | 事件 |
|---|---|
| 2025-09-16 | Zed 创建 `zed-industries/codex-acp`（Rust，基于 Codex CLI 内部 crate） |
| 2025-11-19 | JetBrains 开发者另起 TypeScript 实现，2025-12-03 改名为 `codex-acp`，**基于 Codex App Server** |
| 2026-06-08 | Zed 版发布最后一个版本 0.16.0 |
| 2026-06-22 | Zed 版 README 写明 "Development has moved to agentclientprotocol/codex-acp" |
| 2026-06-23 | 新实现发布 v1.0.0 |
| 2026-07-22 | Zed 版仓库归档 |

[S12][S13]

- **当前版本**：v2.2.2（2026-10-10）；v2.0.0（2026-09-28）有破坏性变更。[S12]
- **维护方**：Registry 作者是 OpenAI、JetBrains、Zed Industries；LICENSE 版权写的是 JetBrains s.r.o.，Apache-2.0。[S18][S12]
- **安装与运行**：`npx @agentclientprotocol/codex-acp`，也可以用 bun 打出六个平台的独立二进制；只支持 stdio。适配器自带 `@openai/codex`（锁定 0.160.1，而 Codex 最新版是 0.163.0），可用 `CODEX_PATH` 换成其他二进制。运行时会启动 `codex app-server` 子进程做协议翻译，**并启用 app-server 的 `experimentalApi`**。[S12 README, package.json, CodexAcpClient.ts][S15]

### 4.2 能力

- **协议**：同时支持 v1 和 v2，按客户端在 `initialize` 中请求的版本路由，不需要额外开关。[S12 AcpAgentRouter.ts]
- **能力位**：
  - `loadSession`
  - `promptCapabilities`：`image`、`embeddedContext`（也支持 resource link）
  - `mcpCapabilities`：http（没有 sse）
  - `auth.logout`
  - `providers`（unstable）
  - `sessionCapabilities`：list、resume、close、delete、fork（unstable）、additionalDirectories、subagents

  [S12]
- **认证**：
  - `api-key`：读 `CODEX_API_KEY` 或 `OPENAI_API_KEY`，也可以在 `_meta` 中传入；
  - `chat-gpt`：浏览器登录；
  - `chat-gpt-device-code`：需要客户端支持 URL 模式 elicitation；
  - `gateway`：需要客户端声明。

  [S12 CodexAuthMethod.ts]
- **配置项**：
  - 模式：Read-only、Workspace、Agent、Full access。每个模式对应一组审批策略和沙箱策略。
  - Collaboration mode：Default 或 Plan。分类名用的是 `collaboration_mode`，没有加 `_` 前缀，严格来说占用了规范保留的命名。
  - 模型
  - Reasoning effort（`thought_level`）
  - Fast mode

  另外保留旧的 `session/set_model`。[S12]
- **会话列表**：通过 `thread/list` 列出 Codex 主目录中来源为 cli、vscode、exec、appServer 的线程，标题由 AI 生成。[S12 CodexAcpClient.ts]
- **diff**：**每个 hunk 发一个 ACP diff，oldText/newText 只包含该 hunk 的改动行和上下文，不是全文**。对 AIR 和 v2 客户端改发 git patch。Codex 回合级的汇总 diff（`turn/diff/updated`）只通过 AIR 的"文件变更报告"扩展提供，标准客户端拿不到（#601）。[S12 FileChangeReporter.ts][S14]
- **计划**：`turn/plan/updated` 转换为 `plan`。Plan 模式下的 Markdown 计划：客户端支持 plan 更新时以 `plan_update`（unstable）发送，否则作为普通消息。[S12 CodexPlanStream.ts]
- **排队与插话**：同一会话的 prompt 走一个回合启动 FIFO，回合进行中再发的 prompt 会**隐式排队**；`_session/steering` 映射到 app-server 的 `turn/steer`。[S12 CodexAcpServer.ts, SteeringQueue.ts]
- **斜杠命令**：`/status`、`/mcp`、`/skills`、`/goal`、`/review`、`/review-branch`、`/review-commit`、`/compact`、`/logout`，以及用户配置的 skills。[S12 README]
- **其他**：子代理会话、后台终端任务、goal、会话改名和归档、列表订阅，这些都是 AIR 扩展；`_auth/status_update`；自动重启崩溃的 app-server。[S12]

### 4.3 已知限制（open issues 节选）[S14]

- #537：`session/delete` 实际是归档，不是删除。
- #601：turn 汇总 diff 不对外提供。
- #600：app-server 中途退出时，`session/prompt` 永远不返回。
- #516 / #539：大会话回放不完整或卡顿。
- #617：回放不带时间戳。
- #533：没有暴露 turn ID 和回合开始确认。
- #556：拒绝审批有时只能结束整个回合。
- #555 / #406 / #519：模式预设不完整（没有"每次都问"的模式，没有"可写工作区且允许联网"的组合）。
- #448：不支持临时（不落盘）会话。
- #568：用户反馈频繁发版会破坏 IntelliJ 中的使用。

## 5. Codex app-server 协议：与 ACP 对比

- **定位**：Codex 用来驱动富客户端的 JSON-RPC 2.0 接口（线上省略了 `"jsonrpc"` 字段）。核心模型是三层：thread（会话）→ turn（回合）→ item（条目）。官方文档明确点名 VS Code 扩展使用它；OpenAI 博客称 web、CLI、IDE 和 macOS 应用共用同一个 harness，并为桌面应用并行协调多个 agent 的需求设计了稳定的 API 表面（**二手**）。[S16][S17]
- **传输**：`--listen stdio://`（默认）、`unix://`、`ws://IP:PORT`、`off`。官方文档说 app-server 命令本身以及 WebSocket 传输"experimental and aren't supported for production workloads"。[S15 main.rs][S16]
- **稳定性**：
  - 方法分为 stable 和 experimental 两层；experimental 方法需要在 `initialize` 中设置 `experimentalApi: true`。
  - 生成的 TypeScript 和 JSON Schema 与 Codex 版本严格对应。
  - 没有正式的语义化版本或兼容性承诺。
  - 有废弃通知（例如 `thread/rollback` 已被删除）。

  [S16][S15 README]
- **规模**：客户端请求约 175 个（其中 stable 约 105 个），服务端通知约 86 个，服务端请求 9 个（另有 2 个旧版审批请求）。[S15，见 sources.md 摘录 F]

**app-server 有、ACP（v1 stable）没有的能力**（已核实于 S15，标"实验"的需要 `experimentalApi`）：

| 能力 | app-server 中的方法 / 通知 | 对 Codex 风格 UI 的意义 |
|---|---|---|
| 多客户端同时订阅一个线程 | 连接级订阅（`subscribed_connection_ids`）、`thread/unsubscribe`、`thread/loaded/list` | 桌面和移动端同时看同一个会话 |
| 远程控制与设备配对 | `remoteControl/enable`、`pairing/start`、`client/list`、`client/revoke`（实验）；经 `wss://chatgpt.com/backend-api/wham/remote/control/server` 中继 | 这是 Codex 自己的移动端远程方案（对 R03 有参考价值） |
| 服务端排队：添加、编辑、删除、重排、手动启动 | `thread/queue/*`、`thread/queue/changed`（实验） | 对应 Codex UI 的排队消息 |
| 插话 | `turn/steer`（stable，带 `expected_turn_id` 前置条件） | 回合进行中补充指令 |
| 中断 | `turn/interrupt` | 等价于 ACP 的 cancel |
| 回合汇总 diff | `turn/diff/updated`（整回合的统一 diff）、`item/fileChange/patchUpdated`、`gitDiffToRemote` | 回合级和分支级的 diff 视图 |
| 回滚、分叉 | `thread/revert`、`thread/fork` | 撤销某个回合，从某点分叉 |
| 线程管理 | `thread/name/set`、`thread/archive`/`unarchive`、`threadSection/*`、`project/*`（实验）、`thread/search`（实验）、已读状态（实验）、`thread/attachment/*`（关联 PR 等） | 侧边栏、项目、搜索、未读标记 |
| 代码审查 | `review/start`、进入/退出审查模式的条目 | `/review` |
| 文件与搜索 | `fs/*`（读写、目录、监听）、`fuzzyFileSearch` | @ 提及的文件选择器、文件浏览（移动端尤其需要） |
| 命令与进程 | `command/exec*`、`thread/shellCommand`、`process/*`、`thread/backgroundTerminals/*`（实验） | 内置终端、后台任务 |
| 账号与用量 | `account/*`、`account/rateLimits/read`、`account/usage/read`、`thread/tokenUsage/updated` | 额度和用量面板 |
| 模型与配置 | `model/list`、`permissionProfile/list`、`config/*`、`collaborationMode/list`（实验） | 设置页 |
| 生态 | `skills/*`、`plugin/*`、`app/*`、`hooks/list`、`mcpServerStatus/list`、`mcpServer/oauth/login`、`externalAgentConfig/*` | 插件、技能、MCP 管理 |
| 实时语音 | `thread/realtime/*`（实验） | 语音输入 |

**ACP 有、app-server 没有的**：跨厂商的统一接口（一个客户端可以接 Claude、Codex、Gemini 等几十个 agent）、Registry 安装分发、标准化的 terminal 认证方法类型、配置项的 UX 分类（app-server 有自己的一套等价物）。

**推测**：Codex 桌面应用的 worktree 功能不在 app-server 协议里。源码中 app-server 和协议 crate 都检索不到 worktree API（已核实），因此推测 worktree 由桌面客户端自己管理（Codex 应用本身闭源，无法核实）。

## 6. 能力对照矩阵

列说明：ACP 规范指 v1 stable，另行注明 unstable 或 draft 的除外；Claude 适配器指 `claude-agent-acp` v0.91.0；Codex 适配器指 `codex-acp` v2.2.2。取值为 ✅ 支持、🟡 部分支持、❌ 不支持。"扩展"表示通过 `_meta` 或 `_` 方法实现的私有扩展，不属于 ACP 标准。

| # | 能力 | ACP 规范 | Claude 适配器 | Codex 适配器 | 依据 |
|---|---|---|---|---|---|
| 1 | stdio 传输 | ✅ | ✅ | ✅ | S2；S9 serve.ts；S12 index.ts |
| 2 | 远程传输（HTTP/WS） | ❌ RFD Active，SDK 已有实现 | ❌ 只有 stdio | ❌ 只有 stdio | S6、S8、S9、S12 |
| 3 | 版本协商（v1/v2） | ✅ v1；v2 为 draft | 🟡 v2 需设置环境变量开启 | ✅ v1 和 v2 自动路由 | S3、S9、S12 |
| 4 | 订阅账号登录 | ✅ 有 terminal 和 agent 两类认证方法 | ✅ Claude 订阅、Console（terminal 方法）；受 Anthropic 政策限制 | ✅ ChatGPT 浏览器登录或设备码 | S2、S9、S11、S12 |
| 5 | API key | 🟡 没有标准的"传入 key"方式 | 🟡 只能通过环境变量或 apiKeyHelper；另有网关方法 | ✅ `api-key` 方法 | S9、S12 |
| 6 | 登出 | ✅ | ✅ | ✅ | S1、S9、S12 |
| 7 | 新建会话、额外工作目录 | ✅ | ✅ | ✅ | S1、S9、S12 |
| 8 | 会话历史列表 | ✅ `session/list` | ✅ 含 CLI 创建的会话 | ✅ 含 CLI 和 VS Code 创建的线程 | S2、S9、S12 |
| 9 | 加载并回放历史 | ✅ `session/load` | 🟡 只回放最近一次压缩之后（#1297），不带时间戳 | 🟡 大会话不完整或很慢（#516/#539），不带时间戳 | S10、S14 |
| 10 | 恢复但不回放 | ✅ `session/resume` | ✅ | ✅ | S1、S9、S12 |
| 11 | 删除会话 | ✅ | ✅ | 🟡 实为归档（#537） | S14 |
| 12 | 关闭会话 | ✅ | ✅ | ✅ | S9、S12 |
| 13 | 分叉会话 | 🟡 unstable | 🟡 支持，但有 #1110 | 🟡 支持（unstable） | S1、S10、S12 |
| 14 | 会话改名、归档、列表订阅 | ❌ 只有 `session_info_update` 推送标题 | 🟡 扩展，仅 AIR | 🟡 扩展，仅 AIR | S9、S12 |
| 15 | 图片输入 | ✅ 由 `promptCapabilities.image` 声明 | ✅ | ✅ | S1、S9、S12 |
| 16 | 消息和思考流 | ✅ | ✅ | ✅ | S1、S9、S12 |
| 17 | 工具调用展示（类型、状态、位置、原始输入） | ✅ | ✅ | ✅ | S2、S9、S12 |
| 18 | 单次编辑的 diff | ✅ 全文 oldText/newText；v2 草案为结构化 diff 加 git patch | ✅ 全文 diff；AIR 可选 git patch | 🟡 每个 hunk 一段，不是全文；AIR 和 v2 用 git patch | S2、S3、S9、S12 |
| 19 | 回合或会话级的汇总 diff、变更文件清单 | ❌ | 🟡 扩展（AIR 文件变更报告） | 🟡 扩展（AIR）；标准客户端拿不到（#601） | S9、S12、S14 |
| 20 | agent 执行命令的终端输出 | 🟡 v1 只有"客户端执行"的终端模型；agent 自己执行时的展示要到 v2 草案才有 | 🟡 Zed `_meta.terminal_*` 约定，或代码块 | 🟡 同左 | S2、S3、S9、S12 |
| 21 | 客户端 fs 和终端能力 | ✅ stable（v2 删除） | ❌ 不使用 | ❌ 不使用 | S1、S3、S9、S12 |
| 22 | 计划和待办 | ✅ `plan` 全量替换；多计划和 Markdown 计划为 unstable | ✅ TodoWrite 转 `plan` | ✅ `plan`；plan 模式的 Markdown 用 `plan_update`（unstable） | S1、S9、S12 |
| 23 | 审批（权限请求） | ✅ 一次、总是、拒绝 | ✅ "总是允许"会写入规则 | ✅ 命令、文件、MCP 审批，以及计划确认 | S2、S9、S12 |
| 24 | 结构化提问（elicitation） | ✅ 2026-07 稳定 | ✅ AskUserQuestion、MCP OAuth | ✅ request_user_input、设备码登录 | S4、S9、S12 |
| 25 | 模式切换 | ✅ 配置项分类 `mode`（`set_mode` 将被淘汰） | ✅ 五种权限模式 | ✅ 四种沙箱和审批预设，外加 Plan 协作模式 | S2、S9、S12 |
| 26 | 模型切换 | ✅ 分类 `model` | ✅ | ✅ | S2、S9、S12 |
| 27 | 推理强度切换 | ✅ 分类 `thought_level` | ✅ effort | ✅ reasoning effort | S2、S9、S12 |
| 28 | 其他模型参数（Fast mode 等） | ✅ `model_config`、boolean | ✅ | ✅ | S2、S9、S12 |
| 29 | 斜杠命令 | ✅ | ✅ 含自定义命令和 `/mcp` | ✅ `/review`、`/compact` 等 | S2、S9、S12 |
| 30 | MCP 配置 | ✅ stdio 必须；http、sse 可选 | ✅ stdio、http、sse | 🟡 stdio、http（没有 sse） | S2、S9、S12 |
| 31 | 中断 | ✅ `session/cancel`、`$/cancel_request` | ✅ | ✅（但有 #600） | S1、S9、S12、S14 |
| 32 | 排队（运行中继续发消息） | ❌ v1 未定义，v2 也明确不规定 | 🟡 扩展 `promptQueueing`：并发的 prompt 进入队列，不能编辑或重排 | 🟡 隐式 FIFO，不能编辑或重排 | S7、S9、S12 |
| 33 | 插话（steering） | ❌ | 🟡 扩展 `_session/steering`，有 #1267、#1280 | 🟡 扩展 `_session/steering`，映射到 `turn/steer` | S9、S10、S12 |
| 34 | 运行状态（idle / running / 等待用户） | ❌ v1 只能看 prompt 是否已返回；v2 草案有 `state_update` | 🟡 仅 v2 | 🟡 仅 v2 | S3、S9、S12 |
| 35 | 用量和上下文窗口 | ✅ `usage_update` | ✅ | ✅ | S1、S9、S12 |
| 36 | 上下文压缩展示 | ✅（2026-10-09 稳定） | ✅ | ✅ | S4、S9、S12 |
| 37 | 会话标题 | ✅ `session_info_update` | ✅ | ✅ AI 生成 | S1、S9、S12 |
| 38 | 子代理 | 🟡 unstable | ✅ 需客户端声明能力 | ✅ 需客户端声明能力 | S5、S9、S12 |
| 39 | worktree | ❌ 只有 `cwd` 和 `additionalDirectories` | 🟡 只在 AIR 会话列表中标记 `agentWorktree`，不能通过 ACP 创建 | 🟡 同左 | S2、S9、S12 |
| 40 | 后台任务 | ❌ | 🟡 扩展（AIR async tasks） | 🟡 扩展（AIR） | S9、S12 |
| 41 | 回滚（rewind / revert） | ❌ | ❌ 方法注册中没有 | ❌ 没有对外暴露（app-server 自身有 `thread/revert`） | S9、S12、S15 |
| 42 | 多客户端同时观察一个会话 | ❌ stdio 是一对一的；v2 草案为此铺路 | ❌ | ❌ | S2、S3 |
| 43 | 扩展机制 | ✅ `_meta`、`_` 方法、自定义能力 | ✅ 大量使用 | ✅ 大量使用 | S2、S9、S12 |

## 7. 与"Codex 风格 UI"相关的缺口

下表的"app-server"列用于说明：如果改为直连 Codex app-server，能拿到什么。

| 功能 | ACP 标准支撑 | 两个适配器现状 | app-server 原生 | 我们需要补什么 |
|---|---|---|---|---|
| **会话历史列表与恢复** | 有：`session/list`、`load`、`resume`、`close`、`delete` 都已稳定 | 都支持，并能看到 CLI 创建的会话；但回放不完整（Claude 只回放压缩之后，Codex 大会话有问题），不带时间戳；改名和归档只对 AIR 开放 | 有，并且还有分区、项目、搜索、已读状态 | 宿主层建一个**跨 agent 的统一会话索引**（标题、项目、时间、状态、未读），自己持久化事件日志，补齐时间戳和完整历史，不完全依赖适配器回放 |
| **diff 和文件变更展示** | 只有单次编辑的 `diff`（全文）；没有回合级汇总；v2 草案有结构化 diff 和 git patch | Claude 给全文；Codex 按 hunk 给片段；汇总 diff 只对 AIR 开放 | 有 `turn/diff/updated`、`gitDiffToRemote` | 宿主层**直接对工作目录或 worktree 做 git diff** 来生成回合和会话的变更视图（与 agent 无关、最可靠）；工具调用里的 diff 只作为过程展示 |
| **计划和待办** | 有 `plan`（条目列表）；Markdown 计划只有 unstable 的 `plan_update` | Claude 的 TodoWrite 和 Codex 的计划都会转换；Codex 的 plan 模式 Markdown 需要客户端支持 unstable 能力 | 有 `turn/plan/updated`、`item/plan/delta` | 声明支持 unstable 的 `plan_update`（或做降级处理）；计划审批（例如"实施这个计划？"）复用权限请求 |
| **审批流程** | 有 `request_permission`（四类选项）和 elicitation | 都支持；但"总是允许"的语义由 agent 自己定义，Codex 有时只能用结束回合来拒绝（#556） | 更细：命令、文件、权限分开审批，还有 Guardian 自动审查 | UI 按选项的 `kind` 渲染；宿主层统一维护"待审批"队列，移动端只消费这个队列（审批是移动端的核心场景） |
| **worktree** | 无 | 无（只有 AIR 的标记） | 协议中无（已核实）；推测由 Codex 桌面应用自己管理 | **宿主层自己实现**：创建和清理 git worktree、把 `cwd` 指过去、做合并和 PR；完全不依赖协议 |
| **模型和推理强度切换** | 有：配置项分类 `model`、`thought_level`、`model_config`，可在回合进行中修改 | 都支持 | 有，并且可以按回合覆盖 | 按分类渲染选择器，未知的分类要能降级显示；不要依赖已废弃的 `session/set_model` |
| **图片输入** | 有，需要能力声明 | 都支持 | 有（还支持本地图片路径） | 粘贴或拖拽后转成 base64 的 `image` 块；移动端拍照上传走同一路径 |
| **中断** | 有 `session/cancel`（要求等待 `cancelled` 结果） | 都支持，Codex 有不返回的风险（#600） | 有 `turn/interrupt` | UI 先标记取消，再等待 `stopReason`；宿主层加超时兜底 |
| **排队** | 无（v2 也不规定） | 都是私有实现：Claude 的 `promptQueueing`、Codex 的隐式 FIFO，都不能编辑或重排 | 有 `thread/queue/*`（实验），支持编辑和重排 | **在宿主层实现可编辑的队列**，回合结束（`stopReason` 返回）后再发下一条，不依赖 agent 内部的队列；以后可以改用 v2 的 idle 状态驱动 |
| **插话（steering）** | 无 | 两边都有 `_session/steering` 扩展，行为不完全一致 | 有 `turn/steer`（stable） | 作为"增强能力"按 `_meta.steering.supported` 探测后启用，失败时退回排队 |

## 8. 已有 ACP 客户端（参考）

| 客户端 | 形态 / 许可证 | 与本项目的关系 | 来源 |
|---|---|---|---|
| Zed | 编辑器（Rust）；ACP 的发起方 | 参考实现；外部 agent 从 Registry 安装，认证由 agent 自己负责，可以导入 agent 已有的线程 | S19、S20 |
| JetBrains Air | 桌面"智能体开发环境"，2026-03 公开预览；闭源 | **形态最接近本项目**：多个 agent 并行运行，支持 worktree 和 Docker 隔离，通过 ACP 接入 Claude Agent、Codex 等。两个适配器里大量的 AIR 扩展就是为它做的。据报道 Claude 不能用订阅账号接入（**二手**） | S21、S9、S12 |
| Harnss | 桌面应用，MIT，TypeScript | **混合接入**：Claude Code 用 Agent SDK 直连，Codex 用 app-server 直连，其他 agent 走 ACP；带 git 和 worktree 管理 | S22 |
| Codeg（原 `xintaofei/codeg`，现 `spacering-net/codeg`） | 桌面、Web 和自托管服务，Apache-2.0，Rust | 所有 agent 都走 ACP；每个待办在独立的 worktree 中运行；汇总多个 agent 的会话 | S23、S19 |
| AionUi | 桌面应用，Apache-2.0，约 3.4 万 star | 同时管理 Claude Code、Codex 等 20 多个 agent 的协作类应用 | S19 |
| ACP UI（formulahendry） | 桌面、移动、Web，MIT，Vue | 跨端 ACP 客户端，适合参考移动端形态；最近一次推送是 2026-05 | S19 |
| Agmente | iOS，MIT，Swift | 同时支持 ACP 和 Codex app-server 两种后端 | S23 |
| Happy | 移动、桌面、Web，MIT | 移动端加端到端加密中继的代表作（细节见 R03） | S19 |
| 其他 | Superlite、Vibex、Threadlines、Capsule、Braide、Poolside Desktop 等 | ACP 官方客户端清单中带 GUI、支持并行会话或 worktree 的桌面客户端，数量在 60 个以上 | S19 |

可以观察到：一类成熟客户端（Harnss、Agmente）会**给 Codex 单独走 app-server**，以拿到 ACP 没有的能力；另一类（Codeg、Air、Zed）坚持全部走 ACP，worktree 和 git 这类能力由客户端自己实现。

---

## 对架构的启示

以下是给产品-架构对齐会的输入，出发点是"桌面端先行、移动端平滑接入"。这些是建议，不是最终选型，最终选型留到阶段 ② 用 ADR 决定。

1. **ACP 只作为南向接口（接 agent），不作为桌面与移动端之间的协议。**
   ACP 的 stdio 传输是一对一的，v1 没有运行状态、断线不补发、不支持多客户端，远程传输也还没有稳定。而移动端需要多端同时观察、断线重连补发、端到端加密中继。如果让手机直接充当"远程 ACP 客户端"，这些需求一个也满足不了。

2. **桌面端内置一个会话宿主（Session Host），从第一天起就把 UI 和 agent 解耦。**
   宿主是一个常驻的本地后端进程，负责：
   - 持有所有 ACP 子进程连接；
   - 把各家的事件规范化为**内部事件模型**；
   - 写入带序号的事件日志；
   - 对外提供同步协议（订阅、按序号补发、发送命令）。

   桌面 UI 也通过本地回环使用同一套协议。这样第二阶段接入移动端时，只需要给这套协议加一种传输（中继加端到端加密），不用重写。这也顺带解决了 #1269 一类的问题：宿主在本地及时读取 ACP 流，不会把移动网络的背压传回 agent。

3. **内部事件模型参考 ACP v2 草案，但不直接绑定 v2。**
   v2 的几项设计正好适合多端同步：消息和工具调用按 ID upsert、`state_update`（running / idle / requires_action）、prompt 插入时返回 `messageId`。建议内部模型直接采用这些语义，由一个 v1→内部模型的转换层对接现在的适配器；等 v2 稳定后再加一个直通层。这样既吃到了 v2 的设计红利，又不承担草案变动的风险。

4. **ACP 不覆盖的能力，统一做成宿主服务，桌面和移动端共用。**
   包括：worktree 管理、git diff 汇总（回合级和会话级）、文件浏览和模糊搜索（@ 提及）、可编辑的消息队列、统一的会话索引（跨 agent，带未读和状态）、待审批队列、通知推送、用户自己的终端。这些正是 Codex 风格 UI 的缺口，也正是移动端需要远程调用的能力。放在宿主层，就是一次实现、两端复用。

5. **把 agent 驱动层抽象成接口，按 agent 选择不同实现。**
   - **Claude**：首选 `claude-agent-acp`（ACP），备选是直接用 Claude Agent SDK。两者之间差距不大，而且适配器本身就是 SDK 的一层翻译，还提供 `_claude/sdkMessage` 原始消息透传作为逃生口。
   - **Codex**：首期用 `codex-acp`，与 Claude 走同一套 ACP 路径，实现成本最低。同时预留"直连 app-server"的实现，用来拿到队列、汇总 diff、文件搜索、多连接等能力。需要注意：codex-acp 本身已经依赖 app-server 的实验 API，并不比直连更稳；直连则要自己承担 app-server 无版本承诺的风险。Harnss、Agmente 已经采用混合方案。
   - 需要在阶段 ② 用 ADR 决定 Codex 走哪条路径。
   - 支持 `_session/steering`、`promptQueueing`、Zed 的 terminal `_meta` 约定这类私有扩展时，一律**按能力探测、可以降级**。AIR 扩展按客户端身份开放（`_meta.jetbrains.air`），不建议冒充 AIR 客户端。

6. **版本与分发策略：钉版本、随应用分发、加回归测试。**
   两个适配器几乎每天发版，有过破坏性变更（codex-acp 2.0.0），也有用户反馈频繁发版破坏使用体验（#568）；ACP schema 每 1–2 周一版。建议：
   - 适配器锁定版本，随应用一起打包（或通过 Registry 安装并钉住版本）；
   - Node 22 运行时要随应用携带，或使用适配器的独立二进制（codex-acp 已有 bun 打包）；
   - 用 acp-tck 和录制回放做兼容性回归。

   Claude Code 和 Codex 的原生二进制由适配器的依赖带入，会话数据与 CLI 共享 `~/.claude` 和 `~/.codex`。

7. **账号与合规需要尽早在 PRD 中定下来（对应待澄清问题 Q1 和 Q7）。**
   Anthropic 明确禁止第三方产品提供 claude.ai 登录（除非事先获批），产品也不能以 "Claude Code" 命名，SDK 受 Commercial Terms 约束。如果产品是自用或开源、由用户自己运行，风险相对可控（具体仍需确认）；如果是商业分发，Claude 侧大概率只能用 API key 或用户自己的网关。Codex 侧支持 ChatGPT 登录，但本次未查到 OpenAI 对第三方客户端使用 ChatGPT 登录的明确条款（**未核实**）。

8. **移动端的远程方案不要等 ACP 远程传输。**
   ACP 的 HTTP/WS RFD 将来稳定后，也只解决"传输"问题，v1 不补发断线期间的消息，也不涉及端到端加密和多端同时观察。Codex 自己的远程控制走的是 chatgpt.com 中继和配对（已核实端点；加密细节未核实），与我们自建中继加端到端加密的方向不同，可以作为 R03 的对照。

## 关键取舍

- **以源码和 schema 为准，README 只作辅助。** 结果确实发现了 README 与代码、或 Registry 与仓库不一致的地方（例如 Registry 中 claude-acp 的 license 写成 proprietary），也发现了 README 没写的限制（Codex 的 diff 按 hunk 发送、Claude 不使用客户端的 fs 和终端）。
- **只看本地克隆的 HEAD，不追溯每个提交的历史。** blobless 克隆下无法做 pickaxe 搜索，改名和迁移的时间改用 CHANGELOG 和 GitHub release 的日期确认。
- **矩阵以 v1 stable 为基准。** unstable 和 draft 的能力单独标注，因为产品首期应该建立在 stable 之上；v2 只作为内部模型设计的参考。
- **app-server 只做与 ACP 的能力差异对比，不做完整接口文档。** 完整的方法清单放在 sources.md 摘录 F。
- **不评估各客户端的实际体验。** 没有运行任何程序，客户端清单只用于了解形态和接入方式。

## 验证方式

没有构建和测试（纯调研、不写代码）。按 brief 的验收标准逐条核对如下：

| 验收标准 | 完成情况 |
|---|---|
| ACP 能力清单：方法或通知 / 作用 / 稳定性 | ✅ 第 1.3 节三张表覆盖全部 stable 和 unstable 方法与通知（含 `session/update` 的全部变体），以及 v2 draft 的差异；原文清单见 sources.md 摘录 A、B |
| 能力对照矩阵：能力 × （ACP、Claude 适配器、Codex 适配器），标注支持程度并写依据 | ✅ 第 6 节，43 项，每行都有依据编号 |
| 重点说明 Codex 风格 UI 的缺口（会话历史与恢复、diff、计划、审批、worktree、模型和推理强度、图片输入、中断和排队） | ✅ 第 7 节逐项说明（中断和排队拆成两行，另加插话），并给出"我们需要补什么" |
| 每条事实附来源和日期，区分已核实和推测 | ✅ 正文用 `[Sx]` 引用；sources.md 列出每个来源的版本、提交、日期和获取方式；二手和推测内容在正文中单独标注 |
| 「对架构的启示」一节 | ✅ 共 8 条，从"桌面端先行、移动端平滑接入"的角度给出 |

自查：所有 GitHub 数据都通过 `gh` 现场获取（2026-10-16）；源码引用都给出了文件和函数位置，便于复查。

## 提交列表

```
（提交后补充）
```

## 遗留问题 / 范围外发现

1. **政策需要确认**：Anthropic 对"开源或自用客户端由用户自行登录 Claude 订阅"的态度，以及 OpenAI 对第三方客户端使用 ChatGPT 登录的条款，本次都没有找到权威结论。建议在 PRD 定位确定后专门确认，或咨询法务。
2. **没有抓到原文的二手来源**：OpenAI app-server 博客、JetBrains Air 的博客和报道（403），相关结论都标了"二手"。如有需要，可以人工打开原文核对。
3. **没有实际运行验证**：没有启动适配器做端到端测试，例如回放完整性、排队行为、steering 失败率。建议在技术设计阶段派一个原型任务（例如 T 系列的 spike），用最小客户端实测矩阵中标为 🟡 的项目。
4. **Codex 桌面应用的 worktree 实现方式**是推测（协议中没有）。这和 R01（Codex UI 拆解）有关，可由 R01 确认。
5. **范围外发现**：Codex app-server 的远程控制（`remoteControl/*`，经 chatgpt.com 中继和配对）是 Codex 官方的移动端远程方案，建议转给 R03 参考。ACP 客户端清单的 Mobile clients 一节（AgentDeck、Reemoat、Runmote 等都号称"端到端加密中继 + 扫码配对"）也和 R03 直接相关。
6. **codex-acp 的分类命名不规范**：配置项分类用了非 `_` 前缀的 `collaboration_mode`，严格来说占用了规范保留的命名。我们的客户端需要能处理未知分类（规范本身也要求这样做）。
