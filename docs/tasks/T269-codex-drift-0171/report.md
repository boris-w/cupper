# T269 汇报

- 执行者：子 agent（CI 与工具）
- 分支：`chore/T269-codex-drift-0171`
- 完成日期：2026-11-23
- PR：https://github.com/boris-w/cupper/pull/128
- CI：https://github.com/boris-w/cupper/actions/runs/45125856941 。合入 main 后的新头 `49a3b7b5` 上，8 个检查全部通过。
  - 第一次跑被 `license` 挡住，原因是当天新出的 RUSTSEC-2026-0592（`url` 2.5.8），与本任务无关。T271 修好后，编排者把 main 合进了分支。
- 分支上的 nightly：https://github.com/boris-w/cupper/actions/runs/45117142724 ，结论 success（`b8be56c0`）。其中 `codex-drift (ubuntu-24.04)`（job 137502453362）通过。判通过要求漂移和记录逐行相同，所以 linux-x64 上的漂移和本机 darwin-arm64 上记下的一致。

## 做了什么

1. **确认 npm 上的新版本**：
   - npm 的 `latest` 是 0.171.0，基线是 0.163.0。
   - 0.169.0 之后有三个版本：0.169.1（11-17）、0.170.0（11-18）、0.171.0（UTC 11-20；任务书写的是 11-21）。
   - 本机 `check`（记录之前）退出码 1：31 个方法的参数变了，没有方法消失，也没有方法变成 experimental。
2. **分清新变化出自哪一版**：从 npm 取了 0.169.0、0.169.1、0.170.0 的平台包，核对 integrity，同样只跑 `generate-json-schema`，用 `drift.ts` 逐版两两比较：
   - 0.169.0 → 0.169.1：没有变化；
   - 0.169.1 → 0.170.0：只改了 `agentMessage.moderation` 里的 `AgentMessageDisplay`，新增 `TrustedContacts` 等 5 个定义；
   - 0.170.0 → 0.171.0：其余全部。
   - 和 0.169.0 的记录逐行比：多出 6 个方法（4 个 MCP、2 个 realtime），8 个方法的内容变了，其余 17 个逐行相同。
   - 任务书点名的几项里，只有 `TrustedContacts` 是新的；`attachedToThreadId`、`moderation`、`imageGeneration` 的新属性、`audio.url`、`AdditionalContextEntry.name` 都是以前的记录里已经审过的。
3. **逐条查对 codex-acp 2.2.2 的源码**（只读）：

| 变化 | 版本 | 适配器的处理 | 影响 |
|---|---|---|---|
| `AgentMessageDisplay` 改成按 `mode` 分的 `show`、`hide`、`block` 三种（`hide` 用求助热线提示替换回答，`block` 因防护或网络安全原因拦下；去掉了 `action` 等 4 个旧字段） | 0.170.0 | 实时渲染和 `session/load` 回放都不读 `moderation` | **只影响显示**：被隐藏或拦下的回答也照常显示原文 |
| `CodexErrorInfo` 加了 `bioPolicy`，与 `cyberPolicy` 同类 | 0.171.0 | 映射表里只有 `cyberPolicy`（归为 `policy_denied`），`bioPolicy` 落到 `provider_error` | 不出错；**归类不准** |
| `account/updated` 加了可选的 `reason` | 0.171.0 | `accountFromUpdated` 只读 `authMode`、`planType` | 无 |
| `mcpToolCall`、MCP 征询请求、MCP 启动状态通知加了可选的 `environmentId` | 0.171.0 | 都不读；`McpStartupTracker` 按"服务器名 + threadId"记状态 | 不出错；只有同一线程里同名服务器注册在多个环境时，后到的状态会覆盖先到的。适配器从不设置环境 |
| `mcpServer/oauth/login`、`mcpServerStatus/list` 加了可选的 `environmentId` | 0.171.0 | 不发，codex 照旧按名字选择 | 无 |
| `thread/list` 加了可选的 `includedThreadIds` | 0.171.0 | 不发 | 无 |
| realtime 条目多了三种和 `observedAtMs` | 0.171.0 | 这两个通知直接忽略 | 无 |

   **去掉的字段**（任务书要求特别看）：
   - `AgentMessageDisplay` 的 4 个旧字段、响应里 `Project` 的 `position`、服务端请求 `currentTime/read`：适配器都没有用到。
   - `audio.url`、`input_audio.audio_url`：结论同以前的记录。

4. **补比响应 schema**（工具不比）：
   - 变了 24 个，大多是上表的定义带来的；另有 `McpServerStatus` 的 `environmentId`、`Project` 去掉 `position`、`OtelRequirements` 等，适配器不读或不调用。
   - 新增 2 个，属于新方法。
   - 去掉 1 个，即 `currentTime/read` 的响应，适配器本来就不处理这个请求。
5. **写入记录**：用 `review <dir> T113 <结论>` 写进 `testdata/codex-drift/reviewed.json` 的 0.171.0 条目，结论逐条写了上面的内容。0.165.0–0.169.0 的记录、基线和已验证范围都没有动。
6. **`notes.md`**：写了版本表、逐版比较、逐条审阅表（附源码里的函数名）和响应比较。

## 关键取舍

- **逐版两两比较**：这次跨了三个版本，逐版比较才能确认 0.169.1 没有协议变化、每处变化各出自哪一版，记录里写得准。
- **单列给 T113 的只有两处**：`moderation` 的展示指示不执行，以及 `bioPolicy` 归类不准。两处都不出错；其余变化都是可选字段，适配器不发也不读，写"无影响"。
- **MCP 的 `environmentId` 写明了覆盖的边界情况**：适配器从不设置环境，cupper 默认碰不到，所以不算影响；但写进记录，T113 升级适配器时能看到。

## 验证方式

- **PR #128 上 8 个检查全部通过**（run 45125856941，`49a3b7b5`）。
- **分支上的 nightly `codex-drift` 判通过**：run 45117142724，job 137502453362。
- **本机**：
  - 记录之后 `check` 退出码 0；`cargo xtask ci codex-drift --in nightly` 通过（2 passed）。
  - `cargo xtask ci testdata-scan` 5 项通过；prettier 通过。
  - `cargo xtask size` 为 +0 −0，代码改动为 0。
- **隔离**：
  - codex 只运行了 `app-server generate-json-schema`，每次的环境里只有 `HOME`、`CODEX_HOME`，都指向各自工作目录里的空目录，跑完只多了 `tmp/`。
  - 没有登录，没有发 prompt，没有读写 `~/.codex`。
  - 外发请求只到 npm，User-Agent 是 `cupper-research`。
  - 下载的包和临时脚本分开放在临时目录，用完已删除。
- **对照验收标准**：
  - [x] 记录已写入 `reviewed.json`，结论逐条说明了对 codex-acp 2.2.2 的影响。
  - [x] 分支上的 nightly `codex-drift` 判通过。
  - [x] PR 上 8 个检查全部通过，链接见上。

## 提交列表

```
49a3b7b5 Merge branch 'main' into chore/T269-codex-drift-0171   （编排者合入 main，带进 T271）
b8be56c0 docs(task): T269 notes: codex 0.170.0 and 0.171.0's drift, item by item
13151852 test(codex-drift): the drift of codex 0.171.0, reviewed for T113
```

## 遗留问题 / 范围外发现

1. **给 T113**：
   - **新增**：2.2.2 不处理 `moderation` 的 `hide`、`block`，被隐藏或拦下的回答在实时和 `session/load` 回放时都照常显示原文。升级适配器时，要定 cupper 是否执行这些展示指示。
   - **新增**：`bioPolicy` 应和 `cyberPolicy` 一样归为策略拒绝，在适配器的映射表里补上即可。0.168.0、0.169.0 新增的 `modelNotFound`、`policyCapacityUnavailable`、`inferenceHookDenied` 也还落在 `provider_error`。
   - **仍然成立**：imageGeneration 遇到 fileId 形式时，`imageResult` 抛 TypeError；`session/load` 回放 fileId 形式的音频时显示不对。
   - 用 `write` 重写基线后，0.165.0–0.171.0 六条记录都会失效。
2. **工具的盲点依旧**：变体里再嵌一层的 `anyOf`，摘要看不到（T246 遗留）。这次 `AgentMessageDisplay` 的三种模式靠摘要的变体描述就能看出来，没有受影响。
3. worktree 保留着。
