# T269 notes

## 本机运行（darwin-arm64，2026-11-23）

- `fetch`：npm 的 `latest` 是 0.171.0，基线是 0.163.0。codex 平台包和 codex-acp 2.2.2 的 tarball 都对上了 integrity。
- `check`（记录之前）：退出码 1，31 个方法的参数变了，提示 "not reviewed yet"。没有方法消失，也没有方法变成 experimental。
- **0.169.0 之后 npm 上的版本**（时间是 UTC）：

| 版本 | 发布时间 |
|---|---|
| 0.169.1 | 11-17 22:26 |
| 0.170.0 | 11-18 03:45 |
| 0.171.0 | 11-20 00:51（任务书写的是 11-21） |

- **隔离**：
  - codex 只运行了 `app-server generate-json-schema`，每次的环境里只有 `HOME`、`CODEX_HOME`，都指向各自工作目录里的空目录，跑完只多了 `tmp/`。
  - 没有读写 `~/.codex`，没有登录，没有发 prompt。
  - 外发请求只到 npm，User-Agent 是 `cupper-research`。
  - 下载的包和脚本分开放在临时目录，不进仓库，用完已删除。

## 新变化出自哪一版

从 npm 取了 0.169.0、0.169.1、0.170.0 的平台包，核对 integrity，同样只运行 `generate-json-schema`，用 `drift.ts` 逐版两两比较：

| 比较 | 结果 |
|---|---|
| 0.169.0 → 0.169.1 | 没有变化 |
| 0.169.1 → 0.170.0 | 只改了 `AgentMessageDisplay`（`agentMessage.moderation.display`），新增 `AnswerAnnotation`、`AnswerReplacement`、`BlockReason`、`HelplineSubject`、`TrustedContacts`。涉及 `item/*`、`thread/started`、`turn/*` |
| 0.170.0 → 0.171.0 | 其余全部，见下表 |

和 0.169.0 那条记录逐行比：
- **多出 6 个方法**：`mcpServer/elicitation/request`、`mcpServer/oauth/login`、`mcpServer/startupStatus/updated`、`mcpServerStatus/list`、`thread/realtime/item/started`、`thread/realtime/item/completed`；
- **内容变了的 8 个方法**：`account/updated`、`error`、`item/completed`、`item/started`、`thread/list`、`thread/started`、`turn/completed`、`turn/started`；
- **其余 17 个与记录逐行相同**。

任务书点名的几项里，`Thread.attachedToThreadId`、`ThreadItem` 的 `moderation` 和 `imageGeneration` 新属性、`UserInput.audio` 去掉 `url`、`AdditionalContextEntry.name` 都是以前的记录里已经有、审过的；`TrustedContacts` 是 0.170.0 新加的。

## 逐条审阅（codex-acp 2.2.2，`package/dist/index.js`，只读）

| 变化 | 版本 | 涉及的方法 | 适配器怎么用 | 影响 |
|---|---|---|---|---|
| `AgentMessageDisplay` 改成按 `mode` 分的三种，`version` 为 1：`show` 可带 `annotation`（求助热线说明）和 `trustedContacts`（`check`/`skip`）；`hide` 带 `replacement`（用求助热线提示替换回答）；`block` 带 `reason`（`protection` 或 `cyber`）。去掉了 `action`、`blockType`、`checkTrustedContacts`、`cyberTrustedAccess`。schema 写明："Chat 怎样展示一个回放的回答" | 0.170.0 | `item/*`、`thread/started`、`turn/*`，以及各个读历史的响应 | 实时按增量渲染；`session/load` 回放时 `createHistoryUpdates` 只取 `id`、`text`、`phase`。两处都不读 `moderation`；源码里没有这些新名字，也没有去掉的那几个 | **只影响显示**：`hide`、`block` 的回答也照常显示原文 |
| `CodexErrorInfo` 加了 `bioPolicy`，与 `cyberPolicy` 并列 | 0.171.0 | `error`、`thread/started`、`turn/*` | `sessionFailureKind` 查 `STRING_CODEX_ERROR_CATEGORIES`：`cyberPolicy` → `policy_denied`，`bioPolicy` 不在表里 → `provider_error` | 不出错；**归类不准**，本该和 `cyberPolicy` 一样算策略拒绝 |
| `AccountUpdatedNotification` 加了可选的 `reason`（`workspaceRoutingReady`、`unknown`，"同一账号的初次路由发现完成时设置"） | 0.171.0 | `account/updated` | `applyAccountUpdated` → `accountFromUpdated` 只读 `authMode`、`planType`；`logout` 等下一条 `account/updated` | 无 |
| `ThreadItem` 的 `mcpToolCall` 加了可选的 `environmentId` | 0.171.0 | `item/*`、`thread/started`、`turn/*` | 渲染工具调用时不读这个字段 | 无 |
| `McpServerElicitationRequestParams`、`McpServerStatusUpdatedNotification` 加了可选的 `environmentId`（"注册范围，不一定是服务器的执行环境"） | 0.171.0 | `mcpServer/elicitation/request`、`mcpServer/startupStatus/updated` | 征询请求按 `threadId`、`turnId` 分派给会话的处理器，源码里 `environmentId` 只出现在权限请求的处理里（另一个方法，以前已审），征询不读它；`McpStartupTracker.record` 按服务器名、再按 `threadId` 记启动状态，不看 `environmentId` | 不出错；只有同一线程里同名服务器注册在多个环境时，后到的状态会覆盖先到的。适配器从不设置环境，cupper 默认碰不到 |
| `McpServerOauthLoginParams`、`ListMcpServerStatusParams` 加了可选的 `environmentId`（省略时沿用旧的按名字选择） | 0.171.0 | `mcpServer/oauth/login`、`mcpServerStatus/list` | 前者只发 `name`、`threadId`；后者（`listMcpServerStatus`）只发 `cursor`、`detail`、`threadId` | 无 |
| `ThreadListParams` 加了可选的 `includedThreadIds` | 0.171.0 | `thread/list` | 三处调用都不发 | 无 |
| `ThreadRealtimeItem` 多了 `frame`、`sessionEndInputSaved`、`turnInputAccepted` 三种，以及 `observedAtMs`；新增 `ThreadRealtimeFrameChannels`、`ThreadRealtimeHandoffMode`、`ThreadRealtimeVoiceTranscript` | 0.171.0 | `thread/realtime/item/started`、`thread/realtime/item/completed` | 这两个通知在忽略的列表里，直接返回空 | 无 |

**去掉的字段**（任务书要求特别看）：
- `audio.url`、`input_audio.audio_url`：以前的记录已经审过，现在仍然成立。适配器的 `userInputToContentBlocks` 仍按 `url` 显示音频（fileId 形式显示不对）；`input_audio.audio_url` 没有用到。
- `AgentMessageDisplay` 去掉的 4 个字段、`Project` 去掉的 `position`、服务端请求 `currentTime/read`：适配器都没有用到。

## 补充：响应 schema（工具不比）

把 `exp` 下的 `*Response.json` 和 `v2/*Response.json` 去掉 description、title 后，比较 0.169.0 和 0.171.0：

- **变了 24 个**：
  - 16 个只因为 `ThreadItem`（moderation、mcpToolCall）、`CodexErrorInfo` 和上面那几个新定义而变，结论同上；
  - `ThreadTimelineListResponse` 还多了 realtime 的几个定义，适配器不调用；
  - `ListMcpServerStatusResponse` 的 `McpServerStatus` 加了可选的 `environmentId`，适配器只收集列表；
  - `Project*Response` 的 `Project` 去掉了必填的 `position`，适配器不调用 `project/*` 方法；
  - `ConfigRequirementsReadResponse`（`OtelRequirements`）、`UserVerificationVerifyResponse` 有变，适配器不调用。
- **新增 2 个**：`ThreadRealtimeListResponse`、`ThreadSearchCancelResponse`，属于新方法，2.2.2 不用。
- **去掉 1 个**：服务端请求 `currentTime/read` 的响应。codex 不会再发这个请求，适配器本来就不处理它。

## 旧记录里的影响

仍然成立：
- `imageGeneration` 遇到 fileId 形式时，`imageResult` 抛 TypeError；
- `session/load` 回放 fileId 形式的音频时显示不对。

## 规模

只改了 `testdata/codex-drift/reviewed.json`（记录数据）和这份笔记，都不计入规模，代码改动为 0。
