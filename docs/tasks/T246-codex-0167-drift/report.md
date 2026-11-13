# T246 汇报

- 执行者：子 agent（工程师）
- 分支：`chore/T246-codex-0167-drift`
- 完成日期：2026-11-09

**CI 状态（交接时）**：
- PR #90 https://github.com/boris-w/cupper/pull/90 ，运行 https://github.com/boris-w/cupper/actions/runs/42599032727 ：
  - protocol、testdata-scan、ts 通过；
  - `license` 失败：`libsqlite3-sys`（经 rusqlite 0.40.2）的 RUSTSEC-2026-0459，和本任务无关，T249 在修；
  - compat-framework、e2e、rust、ui 还在排队。
- 分支上的 nightly https://github.com/boris-w/cupper/actions/runs/42599030369 ：`codex-drift (ubuntu-24.04)`（job 129098651244）是 `fetch ok`、`drift ok`、`2 passed, 0 failed`。判通过要求漂移和记录逐行相同，所以 linux-x64 上的漂移和本机 darwin-arm64 上记下的一致。这个 run 里别的作业与本任务无关，有的还在排队。

## 做了什么

1. **本机跑 `fetch`、`check`**（darwin-arm64）：
   - npm 的 `latest` 是 0.167.0，基线是 0.163.0；两个 tarball 的 integrity 都对上了。
   - 记录之前 `check` 退出码 1：20 个方法的参数变了，提示 "not reviewed yet"。没有方法消失，也没有方法变成 experimental。
2. **分清 0.167.0 新出现了什么**：
   - 和 0.166.0 那条记录逐行比，多出 4 处。
   - 我又从 npm 取了 0.166.0，同样只跑 `generate-json-schema`，用 `drift.ts` 和 0.167.0 两两比较，结果正好也是这 4 处。
   - `ThreadItem`、`ReviewDelivery` 的哈希没有变。任务书举的 `imageGeneration` 去掉 `result`、`detached`，以及 `thread/fork`、`thread/list`、`turn/start`、`process/exited` 的新属性，0.166.0 就有了，T209 已经审阅过。
3. **逐条查 codex-acp 2.2.2 的源码**：

| 0.167.0 的新变化 | 涉及的方法 | 适配器的处理 | 影响 |
|---|---|---|---|
| `InitializeCapabilities` 新增可选的 `requiredHumanApproval` | `initialize` | 只发 `experimentalApi`、`requestAttestation` | 无 |
| `CommandExecutionApprovalKind` 新增 `humanCommand` | `item/commandExecution/requestApproval` | 客户端先声明 `requiredHumanApproval` 才会收到这种类型，适配器不声明；它也不读 `kind`，选项按 `availableDecisions` 给 | 无 |
| `ContentItem` 的 `input_audio`：`audio_url` 改成和 `file_id` 二选一 | `thread/resume`（参数里的 `history`） | 不发 `history` | 无 |
| `UserInput` 的 `audio`：`url` 改成和 `fileId` 二选一 | `turn/start`、`turn/steer` 发出；`item/*`、`thread/started`、`turn/*` 收到 | 发送时 `buildPromptItems` 把 ACP 的 audio 块丢掉；实时的 userMessage 忽略。只有 `session/load` 回放历史时，`userInputToContentBlocks` 的 `case "audio"` 照 `input2.url` 显示 | 只影响显示：fileId 形式的音频显示成 `[@audio](undefined)`，不抛错，同一条消息里的其余内容照常 |

4. **任务书点名的两项**：
   - **`imageGeneration.result`**：0.167.0 的变体和 0.166.0 完全相同，仍是 `ResultThreadItem`（要求 `result`）和 `FileIdThreadItem`（要求 `fileId`）二选一。影响同 T209 的结论：2.2.2 的 `imageResult` 直接调用 `item.result.trim()`，遇到 fileId 形式会抛 TypeError，实时通知里丢图，`session/load` 回放时整个加载失败。没有新影响。
   - **`detached`**：`ReviewDelivery` 只剩 `inline`，和 0.166.0 相同；`runReview` 固定发 `inline`，无影响。
5. **写入记录**：用 `review <dir> T113 <结论>` 写进 `testdata/codex-drift/reviewed.json`。结论逐条写了上面的影响，以及和 0.166.0 相比多出的部分。0.165.0、0.166.0 的记录、基线、已验证范围都没有动。
6. **`notes.md`**：写了本机运行情况、两两比较的结果、逐条审阅表（附源码里的函数名），并补比了一遍响应 schema（工具不比这部分）：
   - 20 个响应只变了 `UserInput` 的 audio 变体，结论同上；
   - `configRequirements/read` 的 `OtelRequirements` 加了 `log_sse_deltas`，适配器不调用这个方法；
   - 新增的 `ThreadGuardianReviewResponse` 属于新方法，2.2.2 不用。

### 音频显示那一处，用户什么时候会碰到
三条要同时满足：
- cupper 在驱动 Codex；
- 本机 codex 是 0.167.0 或更新的版本；
- 经 `session/load` 回放一个会话，里面有 Codex 的其他客户端以 fileId 形式写下的音频输入。

cupper 只在 PS-5 导入 CLI 原有会话时用 `session/load`（ADR-06），M1 不实现。平时重新接上会话用 `session/resume`，带 `excludeTurns: true`，不回放。所以 M1 里碰不到。按任务书我只把它单列出来，没有改适配器或宿主。

## 关键取舍

- **另取 0.166.0 两两比较**：工具只拿最新版本和基线比，看不出哪些是 0.167.0 新出现的。多下载约 140 MB，换来逐行确认，而不是靠对比两份文字报告去猜。
- **音频那处写成"只影响显示"，没有写"无影响"**：确实不出错，也不改变适配器发出的请求，但回放出来的内容是错的，所以如实单列。
- **只读看了 codex-acp 2.6.0**（npm 最新，依赖 `@openai/codex ^0.166.0`）：audio 仍是同样写法，也没有 `requiredHumanApproval`、`humanCommand`。留给 T113 升级时参考，没有升级。

## 验证方式

- **本机**：
  - 记录之后 `check` 退出码 0，并打印审阅结论。
  - `cargo xtask ci codex-drift --in nightly` 判通过（fetch ok、drift ok，2 passed）。
  - `cargo xtask ci testdata-scan` 5 项全部通过；Prettier 检查 `reviewed.json` 和 `notes.md` 都通过。
  - `cargo xtask size` 是 +0 −0（记录数据和文档都不计入）。
- **隔离**：
  - codex 只运行了 `app-server generate-json-schema`，环境里只有 `HOME`、`CODEX_HOME`，都指向工作目录里的空目录，跑完只多了 `tmp/arg0-macos`。
  - 没有登录，没有发 prompt，没有读写 `~/.codex`。
  - 外发请求只到 npm，User-Agent 是 `cupper-research`。
- **对照验收标准**：
  - [x] 记录已写入 `reviewed.json`，结论逐条说明了对 codex-acp 2.2.2 的影响。
  - [x] 分支上的 `codex-drift` 判通过：run 42599030369，job 129098651244。
  - [ ] PR #90 的 8 个检查：`license` 因 RUSTSEC-2026-0459 失败（与本任务无关），等 T249 合并后重开 PR 再跑；其余已经出结果的都通过了。

## 提交列表

```
6d757c88 docs(task): T246 notes: codex 0.167.0's drift, item by item
f3614878 test(codex-drift): the drift of codex 0.167.0, reviewed for T113
```

## 遗留问题 / 范围外发现

1. **给 T113**：
   - 2.2.2 的 `userInputToContentBlocks` 处理 audio 时不区分 `fileId`（同一个函数里 image 已经区分了），fileId 形式会显示成 `[@audio](undefined)`；codex-acp 2.6.0 仍未修。
   - 扩展已验证范围或升级适配器时一起看。用 `write` 重写基线后，0.165.0–0.167.0 三条记录都会失效。
2. **工具的盲点又出现一次**（T209 遗留第 3 条）：`facets` 看不到变体里再嵌一层的 `anyOf`。所以 `audio` 的 `url`、`input_audio` 的 `audio_url` 显示成"去掉了必填字段"，实际是改成了二选一。不会漏报，但描述会误导。要不要补，另开任务定。
3. **test-repeat 跑 vitest 时误报**：已立为 T247，我接下来做。
