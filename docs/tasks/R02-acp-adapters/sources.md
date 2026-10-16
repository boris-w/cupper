# R02 来源索引与原始摘录

访问日期均为 2026-10-16。仓库都克隆到 worktree 之外的临时目录，只读，未执行任何代码。报告正文用 `[Sx]` 引用本页的来源编号。

## 来源索引

| 编号 | 来源 | 版本 / 提交 / 日期 | 获取方式 |
|---|---|---|---|
| S1 | ACP 规范仓库 `agentclientprotocol/agent-client-protocol`：`schema/v1/meta.json`、`meta.unstable.json`、`schema.json`、`schema.unstable.json` | HEAD `5ab891a`（2026-10-16）；Schema v1.25.0、Rust crate v1.11.0（均 2026-10-09 发布） | git clone + 脚本解析 JSON Schema |
| S2 | ACP v1 协议文档（即 agentclientprotocol.com/protocol/v1/...），仓库 `docs/protocol/v1/*.mdx` | 同 S1 | 阅读源文件 |
| S3 | ACP v2 草案：`docs/protocol/v2/*.mdx`、`schema/v2/*`、公告 `docs/announcements/acp-v2-draft.mdx` | 草案公布于 2026-07-20；Schema v2.0.0-alpha.8（2026-10-09） | 阅读源文件 |
| S4 | ACP 更新页 `docs/updates.mdx`、`CHANGELOG.md` | 截至 1.11.0（2026-10-09） | 阅读源文件 |
| S5 | RFD 集合 `docs/rfds/*.mdx` 及 `docs/docs.json` 中的生命周期分组 | 同 S1 | 阅读源文件 |
| S6 | RFD「Streamable HTTP & WebSocket Transport」`docs/rfds/streamable-http-websocket-transport.mdx` | 状态 Active | 阅读源文件 |
| S7 | RFD「v2 Prompt」`docs/rfds/v2/prompt.mdx` | 状态 Active（v2 分组） | 阅读源文件 |
| S8 | 官方 SDK：`typescript-sdk`、`rust-sdk`、`python-sdk`、`kotlin-sdk`、`java-sdk` 的 README、发布列表、目录；公告 `sdk-1-0-releases.mdx` | 见正文第 3 节 | `gh api` / `gh release list` |
| S9 | Claude 适配器 `agentclientprotocol/claude-agent-acp`：`src/acp-agent.ts`（`initialize()` 约 L2766–2970，`v1AgentApp` 约 L13270–13330）、`src/serve.ts`、`src/index.ts`、`src/hide-claude-auth.ts`、`src/session-mode.ts`、`src/session-effort.ts`、`src/diff.ts`、`src/tool-calls/*`、`README.md`、`CHANGELOG.md`、`docs/air-extensions.md`、`package.json` | HEAD `872e93f`（2026-10-15）；发布 v0.91.0（2026-10-15） | git clone + 阅读源码 |
| S10 | Claude 适配器 open issues | 2026-10-16 快照 | `gh issue list` |
| S11 | Claude Agent SDK 概览文档 code.claude.com/docs/en/agent-sdk/overview（platform.claude.com 307 跳转至此） | 2026-10-16 访问 | WebFetch |
| S12 | Codex 适配器 `agentclientprotocol/codex-acp`：`src/CodexAcpServer.ts`（`initialize` 约 L630–787）、`src/AcpAgentRouter.ts`（约 L108–182）、`src/CodexAuthMethod.ts`、`src/CodexAcpClient.ts`（`initialize`、`listSessions`）、`src/tool-calls/reporters/FileChangeReporter.ts`、`src/CodexPlanStream.ts`、`src/SteeringQueue.ts`、`src/AgentMode.ts`、`src/ModelConfigOption.ts`、`src/CollaborationModeConfig.ts`、`README.md`、`CHANGELOG.md`、`package.json` | HEAD `7c761c7`（2026-10-15）；发布 v2.2.2（2026-10-10） | git clone + 阅读源码 |
| S13 | 旧 Codex 适配器 `zed-industries/codex-acp`（Rust，已归档） | 最后发布 0.16.0（2026-06-08）；README 迁移说明提交 `af1dcbd`（2026-06-22）、`296069e`（2026-07-22） | `gh api` |
| S14 | Codex 适配器 open issues | 2026-10-16 快照 | `gh issue list` |
| S15 | `openai/codex` 仓库 app-server：`codex-rs/app-server-protocol/src/protocol/common.rs`（方法定义与 `#[experimental]` 标记）、`schema/json/*.json`、`src/protocol/v2/{thread,turn,item,remote_control}.rs`、`codex-rs/app-server/src/main.rs`（`--listen`）、`codex-rs/app-server/src/thread_state.rs`、`codex-rs/app-server/README.md`、`codex-rs/app-server-transport/src/transport/remote_control/protocol.rs` | HEAD `e790ce2`（2026-10-16）；最新发布 rust-v0.163.0（2026-10-15） | sparse clone + 脚本解析 |
| S16 | Codex app-server 官方文档 learn.chatgpt.com/docs/app-server（developers.openai.com/codex/app-server 308 跳转至此） | 2026-10-16 访问 | WebFetch |
| S17 | OpenAI 博客「Unlocking the Codex harness: how we built the App Server」https://openai.com/index/unlocking-the-codex-harness | 2026-02-04 发布；直接抓取返回 403，只拿到搜索摘要 | WebSearch（二手） |
| S18 | ACP Registry `agentclientprotocol/registry` 的 `claude-acp/agent.json`、`codex-acp/agent.json` | 2026-10-16 快照 | `gh api` |
| S19 | ACP 客户端清单 `docs/get-started/clients.mdx`，以及各客户端仓库的 GitHub 元数据 | 2026-10-16 | 阅读源文件 + GraphQL |
| S20 | Zed 外部 agent 文档 zed.dev/docs/ai/external-agents | 2026-10-16 访问 | WebFetch |
| S21 | JetBrains Air 的公开报道（blog.jetbrains.com/air 2026-03，sdtimes / adtmag / devops.com） | 原文均返回 403，只拿到搜索摘要 | WebSearch（二手） |
| S22 | Harnss README（`OpenSource03/harnss`） | 2026-10-16 | `gh api` |
| S23 | Codeg README（`spacering-net/codeg`，原 `xintaofei/codeg`）；Agmente 仓库描述（`rebornix/Agmente`） | 2026-10-16 | `gh api` |

## 摘录 A：ACP 方法清单（S1，原文）

`schema/v1/meta.json`（stable）：

```json
{
  "version": 1,
  "agentMethods": {
    "initialize": "initialize", "authenticate": "authenticate",
    "session_new": "session/new", "session_load": "session/load",
    "session_set_mode": "session/set_mode", "session_set_config_option": "session/set_config_option",
    "session_prompt": "session/prompt", "session_cancel": "session/cancel",
    "session_list": "session/list", "session_delete": "session/delete",
    "session_resume": "session/resume", "session_close": "session/close",
    "logout": "logout"
  },
  "clientMethods": {
    "session_request_permission": "session/request_permission", "session_update": "session/update",
    "fs_write_text_file": "fs/write_text_file", "fs_read_text_file": "fs/read_text_file",
    "terminal_create": "terminal/create", "terminal_output": "terminal/output",
    "terminal_release": "terminal/release", "terminal_wait_for_exit": "terminal/wait_for_exit",
    "terminal_kill": "terminal/kill",
    "elicitation_create": "elicitation/create", "elicitation_complete": "elicitation/complete"
  },
  "protocolMethods": { "cancel_request": "$/cancel_request" }
}
```

`schema/v1/meta.unstable.json` 比 stable 多出的方法：

- Agent 侧：`providers/list`、`providers/set`、`providers/disable`、`mcp/message`、`session/fork`、`nes/start`、`nes/suggest`、`nes/accept`、`nes/reject`、`nes/close`、`document/didOpen`、`document/didChange`、`document/didClose`、`document/didSave`、`document/didFocus`
- Client 侧：`mcp/message`

`schema/v2/meta.json`（v2 草案的基线）：

- Agent 侧：`initialize`、`auth/login`、`session/new`、`session/set_config_option`、`session/prompt`、`session/cancel`、`session/list`、`session/delete`、`session/resume`、`session/close`、`auth/logout`
- Client 侧：`session/request_permission`、`session/update`、`elicitation/create`、`elicitation/complete`
- 协议级：`$/cancel_request`
- v2 unstable 另有：`providers/*`、`mcp/message`、`session/fork`、`nes/*`、`document/*`

v2 去掉了 `session/load`（并入 `session/resume` 的回放选项）、`session/set_mode`（并入 config options），以及客户端的 `fs/*` 和 `terminal/*`。

## 摘录 B：`session/update` 变体（S1，脚本从 `$defs.SessionUpdate` 提取）

v1 stable（14 种）：

```
user_message_chunk, agent_message_chunk, agent_thought_chunk, tool_call, tool_call_update,
plan, available_commands_update, current_mode_update, config_option_update,
session_info_update, usage_update, notice, compaction_update, compaction_summary_chunk
```

其中 `notice` 需要客户端声明 `session.notices`，`compaction_*` 需要客户端声明 `session.compaction`。

v1 unstable 额外有：`plan_update`、`plan_removed`（plan-operations RFD）、`subagent_update`、`session_message`、`session_message_chunk`（subagents RFD）。

v2 草案：

```
user_message_chunk, user_message, agent_message_chunk, agent_message, agent_thought_chunk,
agent_thought, state_update, tool_call_content_chunk, tool_call_update, terminal_update,
terminal_output_chunk, plan_update, available_commands_update, config_option_update,
session_info_update, usage_update, notice, compaction_update, compaction_summary_chunk,
以及 "_" 前缀的自定义或未来变体
```

v1 schema 中 stable 与 unstable 的定义差异（脚本对比结果，节选）：

```
AgentCapabilities   unstable 多出: nes, positionEncoding, providers
ClientCapabilities  unstable 多出: nes, plan, positionEncodings, subagents
SessionCapabilities unstable 多出: fork, systemPrompt
NewSession/LoadSession/ResumeSessionRequest unstable 多出: systemPrompt
PromptResponse      unstable 多出: usage
McpCapabilities     unstable 多出: acp
```

## 摘录 C：RFD 生命周期（S5，`docs/docs.json` 的分组）

- Draft：session-fork、proxy-chains、mcp-over-acp、diff-delete、custom-llm-endpoint、plan-operations、end-turn-token-usage、get-auth-state、subagents、client-system-prompt
- Active：streamable-http-websocket-transport；v2 分组下有 prompt、enum-variant-extension、client-filesystem-terminal-capabilities、plan-variants、tool-call-updates、message-updates、permission-requests、required-session-methods、session-resume-replay、diff-file-states、terminal-output
- Preview：无
- To be removed：next-edit-suggestions
- Completed：introduce-rfd-process、session-config-options、session-info-update、session-list、acp-agent-registry、session-resume、session-close、logout-method、additional-directories、meta-propagation、session-delete、session-usage、message-id、model-config-category、rust-sdk-v1、request-cancellation、boolean-config-option、elicitation、auth-methods、tool-call-name、session-compaction、session-notices

稳定化日期（S4 `updates.mdx` 与 CHANGELOG）：session config options 2026-02-04；session list、session info update、registry 2026-03-09；session resume 2026-04-22；session close 2026-04-23；logout 2026-05-21；additional directories 2026-06-01；session delete、session usage、message id 2026-06-05；model_config 2026-06-24；request cancellation 2026-06-29；boolean config option 2026-07-06；elicitation 2026-07-22；terminal auth 2026-08-20（1.7.0）；tool call name 2026-09-17；session compaction、session notices 2026-10-09（1.11.0）。

## 摘录 D：claude-agent-acp 的 `initialize` 与方法注册（S9，原文节选）

```ts
// src/acp-agent.ts, initialize()
const sessionCapabilities: SubagentAwareSessionCapabilities = {
  additionalDirectories: {}, close: {}, delete: {}, fork: {}, list: {}, resume: {},
  ...(this.toolCallCapabilities.air.client ? { subagents: {} } : {}),
};
return {
  protocolVersion: 1,
  agentCapabilities: {
    _meta: { claudeCode: { promptQueueing: true }, authStatus: authStatusCapability() },
    promptCapabilities: { image: true, embeddedContext: true },
    mcpCapabilities: { http: true, sse: true },
    auth: { logout: {} },
    providers: {},
    loadSession: true,
    sessionCapabilities,
  },
  agentInfo: { name: packageJson.name, title: "Claude Agent", version: packageJson.version },
  authMethods: [ ...terminalAuthMethods, ...(supportsGatewayAuth ? [gatewayAuthMethod, gatewayBedrockAuthMethod] : []) ],
  _meta: { /* 仅对 AIR 客户端附加 jetbrains.air 能力 */ steering: { supported: true } },
};
```

认证方法（同函数）：非远程环境下，客户端声明 `auth.terminal` 后提供 `claude-ai-login`（"Claude Subscription"，args `--cli auth login --claudeai`）和 `console-login`（"Anthropic Console"，args `--cli auth login --console`）；远程环境（`SSH_*`、`NO_BROWSER` 等）下提供 `claude-login`（`--cli`）；客户端声明 `auth._meta.gateway` 时提供 `gateway`、`gateway-bedrock`。带 `--hide-claude-auth` 参数启动时隐藏订阅登录。

v1 注册的方法（`v1AgentApp`）：

```
initialize, session/new, session/load, session/fork, session/list, session/delete, session/resume,
session/close, session/set_mode, session/set_config_option, authenticate, providers/list,
providers/set, providers/disable, logout, session/prompt, session/cancel(通知),
_session/steering, _session/async_task/stop, _session/goal
另有会话索引扩展（只对声明 AIR sessionIndex 的客户端开放）：
_session/list/subscribe, _session/list/unsubscribe, _session/rename, _session/archive, _session/unarchive
Agent → Client 的扩展通知：_auth/status_update；可选 _claude/sdkMessage（session/new 的 _meta.claudeCode.emitRawSDKMessages 开启）
```

配置项（`SessionConfigOption`）：`mode`（category `mode`，取值 default「Manual」、acceptEdits、plan、auto、bypassPermissions）、模型（category `model`）、`effort`（category `thought_level`）、Fast mode（category `model_config`；客户端支持时为 boolean 类型，否则为 select）。

Claude Code 二进制的定位（`claudeCliPath()`）：优先使用环境变量 `CLAUDE_CODE_EXECUTABLE`；否则使用 `@anthropic-ai/claude-agent-sdk` 的平台 optional dependency 里带的原生 `claude` 二进制。

## 摘录 E：codex-acp 的 `initialize` 与路由（S12，原文节选）

```ts
// src/CodexAcpServer.ts, initialize()
const sessionCapabilities = { resume: {}, list: {}, close: {}, delete: {}, fork: {}, additionalDirectories: {}, subagents: {} };
return {
  protocolVersion: acp.PROTOCOL_VERSION,
  agentInfo: { name: packageJson.name, title: "Codex", version: packageJson.version },
  agentCapabilities: {
    auth: { logout: {} }, providers: {}, loadSession: true,
    promptCapabilities: { embeddedContext: true, image: true },
    sessionCapabilities,
    mcpCapabilities: { acp: false, http: true, sse: false },
    _meta: { [AUTH_STATUS_META_KEY]: authStatusCapability() },
  },
  authMethods: getCodexAuthMethods(_params.clientCapabilities),
  _meta: this.initializeExtensionsMeta(1),   // 含 steering: { supported: true }
};
```

`src/AcpAgentRouter.ts`：`acpV2.agentProtocolRouter().withV1(v1Agent).withV2(...)`，按客户端在 `initialize` 中请求的版本选择 v1 或 v2，不需要额外开关。

v1 注册的方法：

```
initialize, session/new, session/load, session/fork, session/list, session/delete, session/resume,
session/close, session/set_mode, session/set_config_option, authenticate, logout, providers/*,
session/prompt, session/cancel(通知), authentication/status, authentication/logout,
session/set_model(旧版), _session/steering, _session/async_task/stop, codex hooks list/trust,
_session/goal, _session/rename, _session/archive, _session/unarchive,
_session/list/subscribe, _session/list/unsubscribe
```

认证方法（`src/CodexAuthMethod.ts`）：`api-key`（总是提供；读取 `CODEX_API_KEY` 或 `OPENAI_API_KEY`，也可以在 `_meta["api-key"].apiKey` 中传入）；`chat-gpt`（未设置 `NO_BROWSER` 时提供）；`chat-gpt-device-code`（客户端支持 URL 模式 elicitation 时提供）；`gateway`（客户端声明 `auth._meta.gateway` 时提供）。

与 app-server 握手（`src/CodexAcpClient.ts`）：`capabilities: { experimentalApi: true, requestAttestation: false }`，即适配器主动启用了 app-server 的实验 API。

会话列表（`listSessions`）：调用 `thread/list`，`sourceKinds: ["cli","vscode","exec","appServer","unknown"]`，`useStateDbOnly: true`。

文件变更（`FileChangeReporter.ts` 的注释原文）："An update gets one ACP diff per Codex hunk: `oldText` and `newText` hold the changed lines and the context lines of the hunk, not the whole file. ... With the AIR `diffPatch` capability, the diff is a Git patch ... On ACP v2, each file gets one diff with a Git patch."

## 摘录 F：Codex app-server 方法（S15，脚本按 `#[experimental(...)]` 标记分类）

客户端请求，stable（节选，共约 105 个）：

```
initialize; thread/start|resume|fork|archive|unarchive|delete|unsubscribe|name/set|metadata/update|
compact/start|shellCommand|revert|list|loaded/list|read|turns/list|items/list|inject_items;
thread/goal/set|get|clear; thread/attachment/*; threadSection/*;
turn/start, turn/steer, turn/interrupt, review/start;
model/list, modelProvider/capabilities/read, permissionProfile/list, experimentalFeature/*;
skills/*, hooks/list, plugin/*, marketplace/*, app/*;
fs/readFile|writeFile|createDirectory|getMetadata|readDirectory|remove|copy|watch|unwatch;
command/exec|exec/write|exec/terminate|exec/resize; fuzzyFileSearch; gitDiffToRemote;
mcpServer/oauth/login|resource/read|tool/call, mcpServerStatus/list, config/mcpServer/reload;
account/login/start|cancel, account/logout, account/read, account/rateLimits/read, account/usage/read, account/gatewayOAuth/*;
config/read|value/write|batchWrite, configRequirements/read; externalAgentConfig/*; feedback/upload; windowsSandbox/*
```

客户端请求，experimental（需要 `initialize.capabilities.experimentalApi: true`）：

```
thread/queue/add|list|update|delete|reorder|start; thread/settings/update; turn/settings/update;
thread/backgroundTerminals/list|clean|terminate; process/spawn|writeStdin|kill|resizePty;
project/list|read|create|import|update|move|delete; thread/search, thread/searchOccurrences;
thread/readState/update; thread/items/read; thread/prediction/request; thread/memoryMode/set; memory/*;
remoteControl/enable|disable|status/read|pairing/start|pairing/status|client/list|client/revoke;
environment/add|info|status; collaborationMode/list; thread/realtime/*; userVerification/*;
fuzzyFileSearch/sessionStart|sessionUpdate|sessionStop; server/diagnostics; rollout/compress 等
```

服务端请求（stable）：`item/commandExecution/requestApproval`、`item/fileChange/requestApproval`、`item/tool/requestUserInput`、`mcpServer/elicitation/request`、`item/permissions/requestApproval`、`item/tool/call`、`account/chatgptAuthTokens/refresh`、`attestation/generate`；旧版的 `applyPatchApproval`、`execCommandApproval` 仍在 JSON Schema 中。

服务端通知（stable，节选）：`thread/started|status/changed|archived|deleted|closed|reverted|name/updated|tokenUsage/updated|compacted`、`turn/started|completed|diff/updated|plan/updated`、`item/started|completed`、`item/agentMessage/delta`、`item/plan/delta`、`item/reasoning/*`、`item/commandExecution/outputDelta`、`item/fileChange/patchUpdated`、`serverRequest/resolved`、`account/updated`、`account/rateLimits/updated`、`model/rerouted`、`warning`、`deprecationNotice`、`fs/changed`、`remoteControl/status/changed`。experimental 通知包括 `thread/queue/changed`、`thread/readState/changed`、`thread/settings/updated`、`process/outputDelta`、`thread/realtime/*` 等。

传输（`codex-rs/app-server/src/main.rs`）："Transport endpoint URL. Supported values: `stdio://` (default), `unix://`, `unix://PATH`, `ws://IP:PORT`, `off`."

线程队列类型（`v2/thread.rs`）：`ThreadQueueAddParams { thread_id, input: Vec<UserInput>, client_user_message_id }`；`ThreadQueueUpdateParams` 用于编辑排队中的输入；`ThreadQueueReorderParams { queued_submission_ids }` 用于重排；`ThreadQueueStartParams { queued_submission_id: Option }` 用于手动启动。

远程控制（`app-server-transport/.../remote_control/protocol.rs`）：默认连接 `wss://chatgpt.com/backend-api/wham/remote/control/server`，并通过 `.../enroll`、`.../refresh` 注册和刷新；只接受 chatgpt.com、chatgpt-staging.com 或 localhost。

多连接（`app-server/src/thread_state.rs`）：`subscribed_connection_ids(thread_id) -> Vec<ConnectionId>`，一个线程可以同时被多个连接订阅。

## 摘录 G：关键原文引用

- 传输（S2，`transports.mdx`）："The protocol currently defines the following transport mechanisms ... 1. stdio ... 2. Streamable HTTP (draft proposal in progress)"；"Agents and clients **SHOULD** support stdio whenever possible."
- 远程传输 RFD 在 v1 下的可靠性说明（S6）："**In-flight messages are not replayed.** There is no message sequencing or stream resumption, so server→client messages emitted while a client was disconnected are not redelivered on reconnect." v2 计划提供 "Stream resumability"、"Standardized keepalive"。
- v2 prompt RFD（S7）："This RFD does not specify queueing, steering, or whether agents insert new prompts while busy."
- v2 草案公告（S3）："**v2 is a Draft** ... various pieces can, and will, change before stabilization ... gate your implementation behind the version negotiation **AND** feature flags."
- Agent SDK 政策（S11）："Unless previously approved, Anthropic does not allow third party developers to offer claude.ai login or rate limits for their products, including agents built on the Claude Agent SDK." 品牌："Not permitted: 'Claude Code' or 'Claude Code Agent'"。许可："Use of the Claude Agent SDK is governed by Anthropic's Commercial Terms of Service"。
- app-server 文档（S16，WebFetch 摘要中的引文）："Codex app-server is the interface Codex uses to power rich clients (for example, the Codex VS Code extension)."；"The app-server command and WebSocket transport are experimental and aren't supported for production workloads."；"Some app-server methods and fields are intentionally gated behind `experimentalApi` capability."
- 旧 Codex 适配器 README（S13）："Development has moved to agentclientprotocol/codex-acp. The new adapter is built on the new Codex App Server, and we are pooling implementation and maintenance work across teams there."
- Claude 适配器 CHANGELOG（S9）：0.17.0 "Rename from `@zed-industries/claude-code-acp` to `@zed-industries/claude-agent-acp` to align with the current branding guidelines"；0.24.0 "Rename from `@zed-industries/claude-agent-acp` to `@agentclientprotocol/claude-agent-acp`. We are moving this to the main ACP org to better allow multiple teams to contribute and maintain this adapter."
