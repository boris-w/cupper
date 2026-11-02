# T209 汇报

- 执行者：子 agent（工程师）
- 分支：`chore/T209-codex-0166-drift`
- 完成日期：2026-10-30

**CI 状态（交接时）**：
- PR #23 https://github.com/boris-w/cupper/pull/23 ：3ffecd0 上的 8 个检查全部通过（run 41002502707）。
- 分支上手动触发的 nightly https://github.com/boris-w/cupper/actions/runs/41002501699 ：整个 run 判 success。其中 `codex-drift (ubuntu-24.04)`（job 123889909118）是 `fetch ok`、`drift ok`、`2 passed, 0 failed`。

## 做了什么

1. **本机跑 `fetch`、`check`**（darwin-arm64）：
   - npm 的 `latest` 是 0.166.0，基线是 0.163.0。
   - 记录之前 `check` 退出码 1：17 个方法的参数变了，提示 "not reviewed yet"。没有方法消失，也没有方法变成 experimental。
   - 10-29 nightly（run 40967230840，linux-x64）的日志只显示最后 39 行，这 39 行和本机输出逐行相同。
2. **分清 0.166.0 新出现了什么**：工具只拿最新版本和基线比。我另外从 npm 取了 0.165.0、0.163.0，同样只跑 `generate-json-schema`，再用 `drift.ts` 两两比较。
   - 0.163→0.165 和 0.165.0 那条记录逐行相同；0.163→0.166 和 `check` 的输出逐行相同。
   - 0.165→0.166 多出 7 处变化，见下表。
   - 任务书举的例子里，`HookEventName` 的 `pre/postModelRequest` 和 `turn/start` 的 `responsesapiClientMetadataPatch` 在 0.165.0 就有了。0.166.0 新出现的是 `AbsolutePathBuf removed`。
3. **逐条查 codex-acp 2.2.2 的源码**，7 处对它都没有影响：

| 0.166.0 的变化 | 涉及的方法 | 适配器的处理 | 影响 |
|---|---|---|---|
| `CodexErrorInfo` 新增 `policyCapacityUnavailable` | `error`、`thread/started`、`turn/*` | 不认识的错误码归为 `provider_error`，或者照原文显示 | 无 |
| `HookRunSummary.sourcePath` 的类型从 `AbsolutePathBuf` 换成 `LegacyAppPathString` | `hook/started`、`hook/completed` | 线上两者都是字符串，适配器也忽略这两个通知 | 无 |
| `HooksListParams` 新增可选的 `threadId` | `hooks/list` | 只发 `cwds` | 无 |
| `ThreadItem` 新增可选的 `startedAtMs`、`completedAtMs`；`imageGeneration` 新增 `generationId` | `item/*` 等 | 不读，也不把整个 item 转发出去 | 无 |
| 新增 `outputDrainTimedOut` | `process/exited` | 忽略这个通知 | 无 |
| `ReviewDelivery` 去掉了 `detached` | `review/start` | 固定发 `inline` | 无 |
| `AdditionalContextEntry` 新增可选的 `name` | `turn/start`、`turn/steer` | 不发 `additionalContext` | 无 |

4. **更正 0.165.0 关于 imageGeneration 的结论**：
   - `result` 没有被删掉，而是和附件库的 `fileId` 二选一。这是变体里嵌的一层 `anyOf`，工具的摘要看不到这一层，所以显示成了 `-property result`。`inputImages` 是另外新增的字段。
   - 收到 `fileId` 形式时，2.2.2 的 `imageResult` 会调用 `item.result.trim()`，抛出 TypeError。
     - 实时通知：异常被适配器的通知队列捕获，只记一条日志。这个工具调用收不到完成的更新，图片丢失。
     - `session/load`：回放到这一项时整个加载失败，会话被关闭。
   - npm 上最新的 codex-acp 2.5.0 已经改成先判断 `"result" in item`，说明 `fileId` 形式确实会出现。
5. **写入记录**：用 `review <dir> T113 <结论>` 写进 `testdata/codex-drift/reviewed.json`。结论里写了上面的逐条影响和这条更正。0.165.0 的记录、基线和已验证范围都没有动。
6. **`notes.md`**：写了逐条审阅表、源码位置、上面的更正，还有一项补充：用到的请求的响应 schema（工具不比这部分）。0.165→0.166 的响应只有同样的 `ThreadItem`、`CodexErrorInfo` 变化，另外 `HookMetadata` 新增可选的 `environmentId`，适配器不读。

### 用户在什么情况下会碰到 imageGeneration 的问题

要同时满足下面三条：
1. **cupper 在驱动 Codex**：现在的宿主不会。M1a 里 Codex 的请求直接返回 `unavailable`（"only Claude runs in this version"），所以目前没有用户会碰到。Codex 要等 T111 才接进来。
2. **本机 codex 是 0.165.0 或更新的版本**：npm 上最新的已经是 0.166.0，用户正常升级就会落进这一档。
   - 宿主按 `runtime/manifest.json` 的已验证范围（0.150.1–0.163.0，低于 0.140.0 为 unsupported）分类，超过上限判为 `newer_untested`。
   - 设计 5.6.6 和 AG-5 对 `newer_untested` 的处理是"提示，不阻止"，所以 T111 接入 Codex 之后，这些版本能照常使用。
   - 不过 `cli_versions` 现在只读 `cli.claude`，Codex 的分类要由 T111 接上。
   - 0.163.0 及以下总是带 `result`，没有这个问题。
3. **一个回合里模型生成了图片，并且 codex 发的是 `fileId` 形式**：codex 什么时候发这种形式，要由 T113 用真实 Codex 确认。

碰到时的表现：
- 图片丢失，这个图像生成的工具调用收不到完成更新。宿主在回合结束时会把它标成 `abandoned`（coalescer 的规则），回合其余部分不受影响。
- `session/load` 回放失败只会在 PS-5 导入 CLI 原有会话时出现。设计 06 写明 M1 不实现这个功能，所以 M1 里碰不到。
- cupper 平时重新接上会话用的是 `session/resume`。适配器带 `excludeTurns: true` 调用，不回放历史，不受影响。

## 关键取舍

- **另取 0.165.0、0.163.0 两两比较**：工具只能和基线比，看不出哪些是 0.166.0 新增的。同样的隔离方式多下载约 280 MB，换来的是能逐行确认，而不是靠对着两份文字报告猜。
- **更正写在 0.166.0 的结论里，不改 0.165.0 的记录**：匹配只看基线和漂移原文，改旧记录的文字不影响判定，但那属于范围外的修改。交接事项和要不要补改旧记录由编排者定。
- **看了 codex-acp 2.5.0 的源码**：只读不升级，用来佐证 `fileId` 形式确实会出现，也给 T113 升级时参考。

## 验证方式

- 本机：
  - 记录之后 `check` 退出码 0，并打印审阅结论。
  - `cargo xtask ci codex-drift --in nightly` 判 pass（fetch ok、drift ok）。
  - `cargo xtask ci testdata-scan` 5 项全部通过。
  - Prettier 检查 `reviewed.json` 通过。
  - `cargo xtask size` 是 +0 −0（记录和文档都不计入）。
- 隔离：codex 运行时环境里只有 `HOME` 和 `CODEX_HOME`，都指向 scratchpad 里的空目录，跑完只多了 `tmp/arg0-macos`。没有登录，没有发 prompt，没有读写 `~/.codex`。外发请求只有 npm（User-Agent 是 `cupper-research`）。
- 对照验收标准：
  - [x] 记录已写入 `reviewed.json`，结论逐条说明了对 codex-acp 2.2.2 的影响。
  - [x] 分支上的 `codex-drift` 判通过：run 41002501699，job 123889909118，linux-x64 上的漂移和 darwin-arm64 上的记录逐行一致。
  - [x] PR #23 的 8 个检查全部通过（run 41002502707）。

## 提交列表

```
3ffecd0 docs(T209): the review of codex 0.166.0's drift, item by item
d4ce388 test(codex-drift): the drift of codex 0.166.0, reviewed for T113
```

## 遗留问题 / 范围外发现

1. **给 T113**：
   - 用真实 Codex 确认图像生成发的是 `result` 还是 `fileId` 形式；
   - 扩展已验证范围或升级适配器时，codex-acp 2.5.0（依赖 `@openai/codex ^0.165.0`）已经修好这处；
   - 用 `write` 重写基线后，0.165.0、0.166.0 两条记录都会失效。
2. **需要修正的旧描述**：0.165.0 的记录和 `handoff-notes.md` 里 T113 那一行写的"去掉了 result、改成 inputImages，不会崩溃"不准确。实际是 `result`/`fileId` 二选一，遇到 `fileId` 形式会抛 TypeError。编排者说交接事项由他改；0.165.0 记录的文字要不要补改，也请编排者定。
3. **工具的盲点**（`tools/codex-drift` 的 `facets`）：只看定义本身和第一层 `oneOf`/`anyOf`/`allOf`，看不到变体里再嵌的一层。哈希照样能发现变化，不会漏报，但描述会误导。要不要补，另开任务定。
4. **工具不比响应 schema**：这次手动比了一遍，没有发现新的影响。
