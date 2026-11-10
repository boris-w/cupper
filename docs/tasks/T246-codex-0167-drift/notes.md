# T246 notes

## 本机运行（darwin-arm64，2026-11-09）

- `fetch`：npm 的 `latest` 是 0.167.0，基线是 0.163.0。codex 平台包和 codex-acp 2.2.2 的 tarball 都对上了 integrity。
- `check`（记录之前）：退出码 1，20 个方法的参数变了，提示 "not reviewed yet"。没有方法消失，也没有方法变成 experimental。
- 隔离：codex 运行时的环境里只有 `HOME`、`CODEX_HOME`，都指向工作目录里的空目录，跑完只多了 `tmp/arg0-macos`。没有读写 `~/.codex`，没有登录，没有发 prompt。外发请求只到 npm，User-Agent 是 `cupper-research`。

## 哪些是 0.167.0 新出现的

和 0.166.0 那条记录逐行比，0.167.0 多出这些：

```
initialize: params changed
  InitializeCapabilities changed +property requiredHumanApproval
item/commandExecution/requestApproval: params changed
  CommandExecutionApprovalKind changed +enum humanCommand
thread/resume: params changed
  ContentItem changed -input_audio: property audio_url -input_audio: required audio_url
item/*、thread/started、turn/*、turn/start、turn/steer:
  UserInput changed -audio: property url -audio: required url
```

工具只拿最新版本和基线比。照 T209 的做法，我又从 npm 取了 0.166.0 的平台包，核对了 integrity，同样只运行 `generate-json-schema`（`env -i`，只给隔离的 `HOME`、`CODEX_HOME`），再用 `drift.ts` 的 `summarize`、`compare` 比较 0.166.0 和 0.167.0。结果正好是上面这 4 处。`ThreadItem`、`ReviewDelivery` 的定义没有变（哈希相同），所以任务书举的 `imageGeneration` 去掉 `result`、`ReviewDelivery` 去掉 `detached`、`thread/fork`、`thread/list`、`turn/start`、`process/exited` 的新属性，0.166.0 就已经有了，T209 都审阅过。

## 逐条审阅（codex-acp 2.2.2，`package/dist/index.js`）

| 变化 | 涉及的方法 | 适配器怎么用 | 影响 |
|---|---|---|---|
| `InitializeCapabilities` 加了可选的 `requiredHumanApproval`（要求 `experimentalApi`） | `initialize` | `initialize` 只发 `{ experimentalApi: true, requestAttestation: false }` | 无 |
| `CommandExecutionApprovalKind` 加了 `humanCommand`（schema 写明：客户端声明了 `requiredHumanApproval` 才会发，只能回 `accept`、`decline`、`cancel`） | `item/commandExecution/requestApproval` | 适配器不声明这个能力，codex 不会发；`handleCommandExecution` 也不读 `kind`，选项来自 `availableDecisions` | 无 |
| `ContentItem` 的 `input_audio` 变体：`audio_url` 挪进一层 `anyOf`，和 `file_id` 二选一 | `thread/resume`（参数 `history` 里的 `ResponseItem`） | `resumeSession`、`loadSession` 只发 `excludeTurns`、`config`、`cwd`、`modelProvider`、`threadId`，不发 `history`；源码里没有 `input_audio`、`audio_url`。`displayBlock` 里的 `inputAudio`/`audioUrl` 是动态工具输出的另一个类型，没有变 | 无 |
| `UserInput` 的 `audio` 变体：`url` 挪进一层 `anyOf`，和 `fileId` 二选一 | `turn/start`、`turn/steer`（发出）；`item/*`、`thread/started`、`turn/*`（收到） | 发出：`buildPromptItems`（`session/prompt` 和 steer 都用它）把 ACP 的 `audio` 块直接丢掉，不发音频。收到：实时的 `userMessage` 在 `createItemEvent`、`completeItemEvent`（`item/started`、`item/completed`）里都被忽略。只有 `session/load` 回放历史时，`createUserMessageUpdates` → `userInputToContentBlocks` 的 `case "audio"` 调用 `formatUriAsLink("audio", input2.url)`；`name` 非空，直接返回 `` `[@audio](${uri})` ``，fileId 形式会显示成 `[@audio](undefined)`。不抛错，同一条消息里的其余内容照常回放。同一个函数里，`image` 已经按 `"url" in input2` 区分了 `fileId` | 只影响显示：别处（Codex 的其他客户端）写下的 fileId 形式音频，经 `session/load` 回放时显示不对 |

和 `imageGeneration` 一样，摘要只看定义本身和第一层 `oneOf`/`anyOf`，看不到变体里再嵌的一层，所以显示成 `-audio: property url`（T209 遗留第 3 条）。

对 cupper：`session/load` 只在 PS-5 导入 CLI 原有会话时用（ADR-06），M1 不实现；平时重新接上会话用 `session/resume`，带 `excludeTurns: true`，不回放。所以 M1 里碰不到。

npm 上最新的 codex-acp 是 2.6.0（依赖 `@openai/codex ^0.166.0`），只读看了一眼：`audio` 仍是同样写法，也没有 `requiredHumanApproval`、`humanCommand`。给 T113 参考。

## 重点两项（任务书点名）

- **`imageGeneration.result`**：0.167.0 的 `imageGeneration` 变体仍是 `required: [id, status, type]`，外加一层 `anyOf`：`ResultThreadItem`（要求 `result`）或 `FileIdThreadItem`（要求 `fileId`）。和 0.166.0 完全相同。影响同 T209 的结论：2.2.2 的 `imageResult` 直接调用 `item.result.trim()`，遇到 fileId 形式会抛 TypeError，实时通知里这张图丢失，`session/load` 回放时整个加载失败。0.167.0 没有带来新的影响。
- **`detached`**：`ReviewDelivery` 只剩 `inline`，和 0.166.0 相同。`runReview` 固定发 `delivery: "inline"`，无影响。

## 补充：响应 schema（工具不比）

把 `v2/*Response.json` 和顶层的服务端请求响应去掉 description 后比较 0.166.0 和 0.167.0：

- 变了的 21 个里，20 个只变了 `UserInput`（就是上面的 audio 变体），涉及 `thread/read`、`thread/turns/list`、`thread/items/list`、`thread/resume` 等。适配器读这些响应里的 `userMessage`，也是经 `createUserMessageUpdates` 回放，结论同上。
- `configRequirements/read` 的 `OtelRequirements` 加了可选的 `log_sse_deltas`：适配器不调用这个方法。
- 新增 `ThreadGuardianReviewResponse`：新方法，2.2.2 不用。

## 规模

只改了 `testdata/codex-drift/reviewed.json`（记录数据）和这份笔记，都不计入规模。
