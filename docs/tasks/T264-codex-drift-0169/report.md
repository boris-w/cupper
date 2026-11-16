# T264 汇报

- 执行者：子 agent（CI 与工具）
- 分支：`chore/T264-codex-drift-0169`
- 完成日期：2026-11-16
- PR：https://github.com/boris-w/cupper/pull/117
- CI：https://github.com/boris-w/cupper/actions/runs/43790575411 。`1775adb4` 上 8 个检查全部通过。
- 分支上的 nightly：https://github.com/boris-w/cupper/actions/runs/43790575268 ，结论 success。其中 `codex-drift (ubuntu-24.04)`（job 133047821875）是 `fetch ok`、`drift ok`、`2 passed, 0 failed`。判通过要求漂移和记录逐行相同，所以 linux-x64 上的漂移和本机 darwin-arm64 上记下的一致。

## 做了什么

1. **确认 npm 上的新版本**：
   - npm 的 `latest` 是 0.169.0，基线是 0.163.0。
   - 0.168.0 之后只有 0.169.0 一个新版本，两版都是 11-13 发布的，相隔约 5 小时。
   - 本机 `check`（记录之前）退出码 1：25 个方法的参数变了，没有方法消失，也没有方法变成 experimental。漂移和 0.168.0 的记录不同，T261 的沿用逻辑不适用。
2. **分清新出现的变化**：
   - 照 T246、T259 的做法，从 npm 另取了 0.168.0 的平台包，核对 integrity，同样只跑 `generate-json-schema`，再用 `drift.ts` 和 0.169.0 两两比较。
   - 结果和"0.169.0 的漂移对 0.168.0 那条记录逐行比"的差别一致：
     - 多出 4 个方法：`account/gatewayOAuth/changed`、`model/list`、`thread/settings/update`、`thread/settings/updated`；
     - 另有 10 个方法的内容变了；
     - 其余 11 个与 0.168.0 的记录逐行相同。
   - 任务书提到的 `responsesapiClientMetadataPatch`，在 0.167.0 的记录里就有了，不是这次新出现的。
3. **逐条查对 codex-acp 2.2.2 的源码**（只读）：

| 0.169.0 的新变化 | 涉及的方法 | 适配器的处理 | 影响 |
|---|---|---|---|
| `agentMessage` 加了可选的 `moderation`（新定义 `AgentMessageModeration`、`AgentMessageDisplay`；schema 写明是"给 Chat 回放历史时应用的展示指示"） | `item/*`、`thread/started`、`turn/*` 及各个读历史的响应 | 实时按增量渲染；`session/load` 回放时只取 `id`、`text`、`phase`。两处都不读 `moderation` | **只影响显示**：被标记的回答照常显示原文 |
| `CodexErrorInfo` 加了 `inferenceHookDenied` | `error`、`thread/started`、`turn/*` | `sessionFailureKind` 把不认识的字符串一律归为 `provider_error` | 不出错；归类偏笼统 |
| `FunctionCallOutputContentItem` 的 `input_audio`：`audio_url` 改成和 `file_id` 二选一 | `functionCallOutput` 条目、`turn/start` 的 `toolOutput`、`thread/resume` 的 `history` | 实时和回放都忽略 `functionCallOutput` 条目；`toolOutput`、`history` 都不发 | 无 |
| `GatewayOAuthChangedNotification` 加了 `loginId` | `account/gatewayOAuth/changed` | 忽略这个通知 | 无 |
| `ModelListParams` 加了可选的 `modelProvider` | `model/list` | `listModels` 只发 `cursor`、`limit` | 无 |
| `ThreadItemOutputDeferral` 加了 `mcpToolCall` | `thread/items/list` | 不发 `deferOutput` | 无 |
| `ThreadListParams` 加了可选的 `includeAttachedThreads`（只影响按标题搜索） | `thread/list` | 不按标题搜索，也不发这一项 | 无 |
| `ThreadSettingsUpdateParams` 加了可选的 `environments`（省略时保留原选择，除非同时给了 `cwd`） | `thread/settings/update` | `setCollaborationMode` 只发 `threadId`、`collaborationMode`，不带 `cwd` | 无，环境选择不变 |
| `ThreadSettings` 加了 `environments` | `thread/settings/updated` | 存下后只读 `collaborationMode.mode` | 无 |
| `Thread` 加了 `attachedToThreadId` | `thread/started` 及返回 `Thread` 的响应 | 不读 | 无 |

4. **补比响应 schema**（工具不比；去掉 description、title 后比较 0.168.0 和 0.169.0）：
   - 变了 22 个，其中 18 个只因为上表的定义而变；
   - 另有 4 处可选字段：`account/read` 的 `userId`，gatewayOAuth 两个响应的 `prompt`、`supportsDeviceAuth`，`thread/queue/*` 的 `responsesapiClientMetadata`。适配器不读或不调用。
   - 新增 2 个，属于新方法；没有删除的，也没有改成必填的。
5. **写入记录**：用 `review <dir> T113 <结论>` 写进 `testdata/codex-drift/reviewed.json` 的 0.169.0 条目，结论逐条写了上面的内容。
   - 初稿里我把 `toolOutput` 误写成 `toolOutputs`，核对 schema 后重写了这一条。
   - 0.165.0–0.168.0 的记录、基线和已验证范围都没有动。
6. **`notes.md`**：写了版本情况、两两比较、逐条审阅表（附源码里的函数名）、响应比较，以及 codex-acp 的最新版本：仍是 2.6.0，依赖 `@openai/codex ^0.166.0`，早于 0.169.0。

## 关键取舍

- **`moderation` 写成"只影响显示"，单列给 T113**：不出错，也不改变适配器发出的请求。但 codex 明确把它定为展示指示，适配器不执行，被标记的回答会照常显示原文，所以如实单列，不写"无影响"。
- **`inferenceHookDenied` 写成"归类偏笼统"**：它会和其他不认识的错误码一样归为 `provider_error`，界面给出的是通用的错误类别。不算故障，但和 0.168.0 的两个新错误码一样，值得 T113 升级时一起看。
- **只取 0.168.0 两两比较**：两版之间没有别的版本，一次比较就能确认新变化全部来自 0.169.0。

## 验证方式

- **本机**：
  - 记录之后 `check` 退出码 0，并打印审阅结论。
  - `cargo xtask ci codex-drift --in nightly` 通过（2 passed）。
  - `cargo xtask ci testdata-scan` 5 项通过；prettier 检查 `reviewed.json` 和 `notes.md` 都通过。
  - `cargo xtask size` 是 +0 −0，代码改动为 0。
- **隔离**：
  - codex 只运行了 `app-server generate-json-schema`，每次的环境里只有 `HOME`、`CODEX_HOME`，都指向各自工作目录里的空目录，跑完只多了 `tmp/`。
  - 没有登录，没有发 prompt，没有读写 `~/.codex`。
  - 外发请求只到 npm，User-Agent 是 `cupper-research`。
  - 下载的包和临时脚本分开放在临时目录，用完已删除。
- **对照验收标准**：
  - [x] 记录已写入 `reviewed.json`，结论逐条说明了对 codex-acp 2.2.2 的影响。
  - [x] 分支上的 nightly `codex-drift` 判通过：run 43790575268，job 133047821875。
  - [x] PR #117 上 8 个检查全部通过，链接见上。

## 提交列表

```
1775adb4 docs(task): T264 notes: codex 0.169.0's drift, item by item
fc1f444f test(codex-drift): the drift of codex 0.169.0, reviewed for T113
```

## 遗留问题 / 范围外发现

1. **给 T113**：
   - **新增**：2.2.2 不处理 `agentMessage.moderation`，被标记的回答在实时和 `session/load` 回放时都照常显示原文，不执行展示指示（`action`、`blockType` 等）。升级适配器时，要定 cupper 是否执行这些指示。
   - **新增**：`inferenceHookDenied` 和 0.168.0 的 `modelNotFound`、`policyCapacityUnavailable` 一样，被归为笼统的 `provider_error`。需要更细的错误类别时，在适配器的映射表里补上。
   - **仍然成立**：imageGeneration 遇到 fileId 形式时，`imageResult` 抛 TypeError；`session/load` 回放 fileId 形式的音频时显示不对。
   - 用 `write` 重写基线后，0.165.0–0.169.0 五条记录都会失效。
2. **工具的盲点又出现一次**（T246 遗留第 2 条）：`FunctionCallOutputContentItem` 的 `input_audio` 显示成"去掉了必填字段"，实际是改成了二选一。不会漏报，但描述会误导。
3. worktree 保留着。
