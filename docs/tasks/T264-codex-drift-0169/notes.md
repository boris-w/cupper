# T264 notes

## 本机运行（darwin-arm64，2026-11-16）

- `fetch`：npm 的 `latest` 是 0.169.0，基线是 0.163.0。codex 平台包和 codex-acp 2.2.2 的 tarball 都对上了 integrity。
- 0.168.0 之后 npm 上只有 0.169.0 一个新版本，两版都是 11-13 发布的，相隔约 5 小时。
- `check`（记录之前）：退出码 1，25 个方法的参数变了，提示 "not reviewed yet"。没有方法消失，也没有方法变成 experimental。0.168.0 的记录不适用，T261 的沿用逻辑也不适用，因为漂移不同。
- **隔离**：
  - codex 只运行了 `app-server generate-json-schema`，每次的环境里只有 `HOME`、`CODEX_HOME`，都指向各自工作目录里的空目录，跑完只多了 `tmp/`。
  - 没有读写 `~/.codex`，没有登录，没有发 prompt。
  - 外发请求只到 npm，User-Agent 是 `cupper-research`。
  - 下载的包和脚本分开放在临时目录，不进仓库，用完已删除。

## 哪些是新出现的

我从 npm 另取了 0.168.0 的平台包，核对 integrity，同样只运行 `generate-json-schema`，再用 `drift.ts` 和 0.169.0 两两比较。结果与"0.169.0 的漂移对 0.168.0 那条记录逐行比"的差别一致：
- **多出 4 个方法**：`account/gatewayOAuth/changed`、`model/list`、`thread/settings/update`、`thread/settings/updated`；
- **内容变了的 10 个方法**：`error`、`item/completed`、`item/started`、`thread/items/list`、`thread/list`、`thread/resume`、`thread/started`、`turn/completed`、`turn/start`、`turn/started`；
- **其余 11 个与 0.168.0 的记录逐行相同**。

任务书提到的 `responsesapiClientMetadataPatch`（`turn/start`、`turn/steer`）在 0.167.0 的记录里就有，不是新的。

## 逐条审阅（codex-acp 2.2.2，`package/dist/index.js`，只读）

| 变化 | 涉及的方法 | 适配器怎么用 | 影响 |
|---|---|---|---|
| `ThreadItem` 的 `agentMessage` 加了可选的 `moderation`，新定义 `AgentMessageModeration`（`moderated` 必填，`display` 可选）和 `AgentMessageDisplay`（`action` 必填，另有 `blockType`、`checkTrustedContacts`、`cyberTrustedAccess`）。schema 写明这是"给 Chat 回放历史时应用的展示指示"，未知的 `action` 留给产生它的客户端处理 | `item/*`、`thread/started`、`turn/*`；响应里的 `thread/read`、`thread/turns/list`、`thread/resume` 等 | 实时的回答经增量渲染；`session/load` 回放时，`createHistoryUpdates` 的 `agentMessage` 分支只取 `id`、`text`、`phase`。两处都不读 `moderation`，源码里也没有这个字段 | **只影响显示**：被标记的回答照常显示原文，不执行展示指示。cupper 只在 PS-5 导入时用 `session/load`，M1 不实现 |
| `CodexErrorInfo` 加了 `inferenceHookDenied`（"工作区的推理钩子明确拒绝了这次请求"） | `error`、`thread/started`、`turn/*` | `sessionFailureKind`：不在 `STRING_CODEX_ERROR_CATEGORIES` 里的字符串一律归为 `provider_error`。0.168.0 的 `modelNotFound`、`policyCapacityUnavailable` 也是这样 | 不出错；归类偏笼统，不会归成 `policy_denied` |
| `FunctionCallOutputContentItem` 的 `input_audio`：`audio_url` 改成和 `file_id` 二选一（变体里的 anyOf，摘要看不到这一层） | `FunctionCallOutputBody` 只出现在 `functionCallOutput` 条目、`turn/start` 的 `toolOutput`、`thread/resume` 的 `history` | 实时和回放都忽略 `functionCallOutput` 条目（`createItemEvent`、`completeItemEvent`、`createHistoryUpdates` 都返回空）；`toolOutput`、`history` 都不发 | 无 |
| `GatewayOAuthChangedNotification` 加了 `loginId` | `account/gatewayOAuth/changed` | 这个通知直接忽略 | 无 |
| `ModelListParams` 加了可选的 `modelProvider` | `model/list` | `listModels` 只发 `cursor`、`limit` | 无 |
| `ThreadItemOutputDeferral` 加了 `mcpToolCall` | `thread/items/list` | 不发 `deferOutput`（见 T259） | 无 |
| `ThreadListParams` 加了可选的 `includeAttachedThreads`（"按标题搜索时包含挂在父线程下的线程"） | `thread/list` | 三处调用都不按标题搜索，也不发这一项 | 无 |
| `ThreadSettingsUpdateParams` 加了可选的 `environments`。schema 写明：省略或为 null 时保留原来的选择，除非同时给了 `cwd`，那时改用该目录的默认环境 | `thread/settings/update` | 只有 `setCollaborationMode` 调用，只发 `threadId`、`collaborationMode`，不带 `cwd` | 无，环境选择不变 |
| `ThreadSettings` 加了 `environments`（新定义 `ThreadEnvironment`） | `thread/settings/updated` | 存进 `threadSettings` 后只读 `collaborationMode.mode` | 无 |
| `Thread` 加了 `attachedToThreadId` | `thread/started`（以及各个返回 `Thread` 的响应） | 源码里没有这个字段 | 无 |

## 补充：响应 schema（工具不比）

把 `exp` 下的 `*Response.json` 和 `v2/*Response.json` 去掉 description、title 后，比较 0.168.0 和 0.169.0：

- **变了 22 个**：
  - 18 个只因为 `ThreadItem`、`Thread`、`CodexErrorInfo`、`FunctionCallOutputContentItem` 和两个新定义而变，结论同上。
  - 另外 4 处：
    - `GetAccountResponse`（`account/read`）加了可选的 `userId`，适配器不读；
    - `GatewayOAuthLoginResponse`、`GatewayOAuthReadResponse` 加了 `prompt`、`supportsDeviceAuth`，适配器不调用这两个方法；
    - `QueuedSubmission`（`thread/queue/*`）加了 `responsesapiClientMetadata`，适配器不调用。
- **新增 2 个**：`GetAccountSecuritySetupResponse`、`ThreadPrepareApprovedPlanResponse`，属于新方法，2.2.2 不用。
- 没有删除的，也没有改成必填的。

## 旧记录里的影响

仍然成立：
- `imageGeneration` 遇到 fileId 形式时，`imageResult` 抛 TypeError；
- `session/load` 回放 fileId 形式的音频时显示不对。

## 参考

npm 上最新的 codex-acp 仍是 2.6.0（11-08，依赖 `@openai/codex ^0.166.0`），早于 0.169.0，不会处理 `moderation`。没有下载。

## 规模

只改了 `testdata/codex-drift/reviewed.json`（记录数据）和这份笔记，都不计入规模，代码改动为 0。
