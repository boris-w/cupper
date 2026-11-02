# T209 notes

## 本机运行（darwin-arm64，2026-10-30）

- `fetch`：npm 的 `latest` 是 0.166.0，基线是 0.163.0。codex-acp 2.2.2 的 tarball 与 `runtime/manifest.json` 的 integrity 一致。有一次连 npm 超时，curl 自己的重试接上了。
- `check`（记录之前）：退出码 1，17 个方法的参数变了，提示 "not reviewed yet"。没有方法消失，也没有方法变成 experimental。
- 和 10-29 的 nightly（run 40967230840，linux-x64）比：作业日志只显示最后 39 行，这 39 行和本机的输出逐行相同。
- 隔离：codex 运行时环境里只有 `HOME`、`CODEX_HOME`，都指向工作目录里的空目录。跑完只多了 `tmp/arg0-macos`。没有读写 `~/.codex`，没有登录，没有发 prompt。

## 哪些是 0.166.0 新出现的

工具只拿最新版本和基线比。为了分清哪些是 0.166.0 新出现的，又从 npm 取了 0.165.0、0.163.0 的平台包，核对了 integrity，同样只运行 `generate-json-schema`（`env -i`，只给隔离的 `HOME`、`CODEX_HOME`），然后用 `drift.ts` 的 `summarize`、`compare` 两两比较：

- 0.163.0 → 0.165.0：和 `reviewed.json` 里 0.165.0 的记录逐行相同；
- 0.163.0 → 0.166.0：和 `check` 的输出逐行相同；
- 0.165.0 → 0.166.0：下面这些。0.165.0 已有的定义在 0.166.0 里没有别的变化（比如 `PlanType` 的哈希没变）。

```
error: params changed
  CodexErrorInfo changed +enum policyCapacityUnavailable
hook/completed: params changed
  AbsolutePathBuf removed
  HookRunSummary changed
  LegacyAppPathString added
hook/started: params changed
  AbsolutePathBuf removed
  HookRunSummary changed
  LegacyAppPathString added
hooks/list: params changed
  HooksListParams changed +property threadId
item/completed: params changed
  ThreadItem changed +imageGeneration: property generationId +property completedAtMs +property startedAtMs
item/started: params changed
  ThreadItem changed +imageGeneration: property generationId +property completedAtMs +property startedAtMs
process/exited: params changed
  ProcessExitedNotification changed +property outputDrainTimedOut
review/start: params changed
  ReviewDelivery changed -enum detached
thread/started: params changed
  CodexErrorInfo changed +enum policyCapacityUnavailable
  ThreadItem changed +imageGeneration: property generationId +property completedAtMs +property startedAtMs
turn/completed、turn/started：同 thread/started
turn/start、turn/steer: params changed
  AdditionalContextEntry changed +property name
```

任务书举的例子里，`HookEventName` 的 `preModelRequest`、`postModelRequest` 和 `turn/start` 的 `responsesapiClientMetadataPatch`，0.165.0 就已经有了，那条记录里也写着。`AbsolutePathBuf removed` 才是 0.166.0 新出现的。

## 逐条审阅（codex-acp 2.2.2，`package/dist/index.js`）

| 变化 | 涉及的方法 | 适配器怎么用 | 影响 |
|---|---|---|---|
| `CodexErrorInfo` 加了字符串 `policyCapacityUnavailable` | `error`、`thread/started`、`turn/started`、`turn/completed` | `sessionFailureKind` 查 `STRING_CODEX_ERROR_CATEGORIES`，查不到就归为 `provider_error`；客户端不支持 typed failures 时，照 `message` 原文发文本 | 无：不出错，只是类别不细（0.165.0 的 `modelNotFound` 也一样） |
| `HookRunSummary.sourcePath` 从 `AbsolutePathBuf` 换成 `LegacyAppPathString`，两者都是 `{"type":"string"}`；`AbsolutePathBuf` 在这两个方法的闭包里不再出现，别处仍在用 | `hook/started`、`hook/completed` | 这两个通知在 "ignored events" 里，直接返回 null；源码里只出现这一次 | 无：线上仍是字符串，适配器也不读 |
| `HooksListParams` 加了可选的 `threadId` | `hooks/list` | `listCodexHooks` 只发 `{ cwds: [cwd] }` | 无 |
| `ThreadItem` 顶层加了可选的 `startedAtMs`、`completedAtMs`（所有项都有）；`imageGeneration` 加了可选的 `generationId` | `item/*`、`thread/started`、`turn/*` | 源码里没有这三个字段名，也没有把整个 item 转发出去的地方 | 无 |
| `ProcessExitedNotification` 加了可选的 `outputDrainTimedOut` | `process/exited` | 在 "ignored events" 里 | 无 |
| `ReviewDelivery` 去掉了 `detached`，只剩 `inline` | `review/start` | `runReview` 固定发 `delivery: "inline"`，`/review` 的几种命令都走这里 | 无 |
| `AdditionalContextEntry` 加了可选的 `name` | `turn/start`、`turn/steer` | `sendPrompt`、`steerTurn`、标题生成都不发 `additionalContext` | 无 |

## 更正：0.165.0 的 imageGeneration

0.165.0 的记录和 T114 的汇报写的是"`imageGeneration` 去掉了 `result`、改成 `inputImages`，图像生成的结果会丢失，不会崩溃"。看了完整的定义，实际是：

- `inputImages` 是另外新增的可选字段，和 `result` 无关；
- `result` 没有删掉，而是挪进了变体里的一层 `anyOf`：要么是内联的 `{result}`（`ResultThreadItem`），要么是附件库的 `{fileId}`（`FileIdThreadItem`）。schema 的说明是 "Either inline generated-image data or its durable attachment-store identity"，迁移期写下的 rollout 两者都带。
- 工具的摘要只看定义本身和第一层 `oneOf`/`anyOf`/`allOf`，看不到变体里的这一层，所以显示成 `-imageGeneration: property result`。哈希照样发现了变化，不会漏报，只是描述会误导。

对 codex-acp 2.2.2 的影响比 0.165.0 那条记录写的重：

- `imageResult(item)` 直接调用 `item.result.trim()`。收到 `fileId` 形式（没有 `result`）时会抛 TypeError。`standardResult` 里的 `rawOutput.result` 变成 `undefined`，这只是次要的。
- 实时的 `item/completed`：异常被 `enqueueSessionNotification` 捕获，只记一条 "Error handling Codex session notification" 日志。这个工具调用收不到完成的更新，停在 `item/started` 时发的 `in_progress`；`activeImageGenerationItems` 也没有别处清理。图片丢失。
- `session/load` 回放历史：`createHistoryUpdates` 对 `imageGeneration` 调 `ImageGenerationReporter.whole`，同样会抛错。`loadSessionOnce` 捕获后关闭会话，再把错误抛出去，整个加载失败。
- 对 cupper：`session/resume` 带 `excludeTurns: true`，不回放，不受影响；cupper 只在 PS-5 导入 CLI 里原有的会话时用 `session/load`（ADR-06）。
- 收到 `{result}` 形式时，行为和以前一样。

npm 上最新的 codex-acp 2.5.0（依赖 `@openai/codex ^0.165.0`）把这里改成了 `"result" in item && item.result.trim() !== ""`，`rawOutput` 也改为二选一地带上 `result` 或 `fileId`。这说明 `fileId` 形式确实会出现。codex 什么时候发这种形式，要等 T113 用真实 Codex 跑的时候确认。

## 补充：响应 schema（工具不比）

用的是 `v2/<Name>Response.json` 这个命名惯例：127 个方法里有 39 个是请求，其中 29 个能找到对应的文件。去掉 description 后，比较 0.165.0 → 0.166.0：

- 变了的 11 个（`thread/start`、`thread/resume`、`thread/fork`、`thread/unarchive`、`thread/list`、`thread/read`、`thread/turns/list`、`thread/items/list`、`turn/start`、`review/start`、`hooks/list`），变化都在 `ThreadItem`、`CodexErrorInfo`，和上面一样；
- 另外 `hooks/list` 的 `HookMetadata` 加了可选的 `environmentId`。适配器只读 `key`、`currentHash`、`trustStatus`、`source`。

## 规模

只改了 `testdata/codex-drift/reviewed.json`（记录数据）和这份笔记，都不计入规模。
