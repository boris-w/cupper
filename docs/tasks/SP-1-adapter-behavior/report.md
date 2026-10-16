# SP-1 汇报

- 执行者：子 agent（协议工程师）
- 分支：`spike/SP-1-adapter-behavior`
- worktree：`$HOME/.../cupper/.claude/worktrees/agent-a5a4196fd0c78a95c`（编排者存档时去掉了本机用户名）
- 完成日期：2026-10-16

> 编排者存档说明：正文按子 agent 的回复原样保留。只有两节做了脱敏："对用户配置的改动"和"本机遗留"。原文列出了本机临时目录的完整路径，以及会话和线程 ID，这里只保留摘要。完整清单存放在不入库的 `.private/cleanup/SP-1-leftovers.md`，供用户清理时使用。

## 做了什么

我用 TypeScript 手写了一个最小 ACP 客户端，直接收发换行分隔的 JSON-RPC，没有依赖 SDK。用它对三种配置跑了固定剧本：
- `claude-agent-acp` 0.91.0，配本机 Claude Code 2.1.296；
- `codex-acp` 2.2.2，配它自带的 codex 0.160.1（下称 codex-bundled）；
- `codex-acp` 2.2.2，用 `CODEX_PATH` 指向本机的 codex 0.159.2（下称 codex-local）。

两个适配器都锁定了当天 npm 的 `latest`。剧本包括 brief 的 12 个场景，另外补了几个：
- `s02b`：明确要求用计划工具；
- `s02c`、`s02d`：Claude 的任务工具和思考流；
- `s04b`：拒绝命令审批；
- `s07b`：CLI 空闲时被杀后的恢复。

全部帧都录了下来，脱敏后共 48 个 JSONL。

物料：
- `spikes/SP-1-adapter-behavior/`：
  - `acp-client.ts`：客户端；
  - `scenarios.ts`：剧本；
  - `run.ts`：入口；
  - `sanitize.ts`：脱敏脚本；
  - `inspect.ts`：打印录制的时间线；
  - `summarize.ts`：从录制里提取数据；
  - `README.md`；
  - `recordings/{claude,codex-bundled,codex-local}/*.jsonl`：脱敏后的录制。
- `docs/tasks/SP-1-adapter-behavior/notes.md`：每个场景的详细行为、试错记录、对用户配置的改动、本机遗留。
- `docs/tasks/SP-1-adapter-behavior/data/summary.json`：延迟、`stopReason`、各类 update 的数量、内存采样。

**适用范围**：本机的 Codex 不是用 ChatGPT 登录的，走的是用户在 `~/.codex/config.toml` 里配置的一个自定义 Responses API 网关。这个网关在测试中经常过载重连，所以 Codex 侧的回合普遍偏慢，模型目录也是网关提供的。两个 CLI 都会加载用户自己配置的 MCP 服务器、hooks 和 notify，这些进程都算进了内存数据。

### 核心结论（先看这里）

1. **排队必须由宿主实现，绝不能并发发送 `session/prompt`。**
   - Claude 会把第二条消息并进当前回合，第一条消息自己的回复永远不会生成。
   - Codex 下第一个 `session/prompt` 永远不返回，我等到客户端 180 秒超时为止，bundled 和 local 都一样。
2. **Codex 的 #556 稳定复现。** 不管是文件修改还是命令，ACP 层唯一的 reject 选项都是 `cancel`，选了它整个回合就以 `cancelled` 结束。Claude 拒绝后回合继续。
3. **#600 没有复现，但 Codex 取消后命令不会被终止。**
   - 回合进行中杀掉 app-server，10 ms 内返回错误 1001，`data.restartable` 为 true，下一个回合会自动重启 app-server。
   - Codex 取消回合后，正在跑的命令继续运行，它的输出和 `completed` 会在下一个回合里推过来。
   - Claude 的 CLI 崩溃后，适配器回答 "Session not found"，需要宿主调用 `session/resume`（约 0.8 秒）。
4. **回放不能当作历史来源。**
   - 两边回放都没有时间戳，Claude 是 #1307、Codex 是 #617，都复现。
   - Claude 执行 `/compact` 之后，回放只剩一条压缩摘要和最后一条消息（#1297 复现）。Codex 回放完整。
5. **会有不属于任何 prompt 的 `session/update`。**
   - Claude 插话时，如果命令正在执行，命令会被移到后台；命令结束后 Claude 会自己开一个回合，它的输出在 prompt 已经返回之后才推过来。
   - 空闲时插话会启动一个没有对应 prompt 的新回合。
   - Codex 取消后遗留的工具输出，会在下一个回合里补发。
   - 所以事件模型不能用"一次 prompt 请求到它的响应"来界定回合。
6. **配置项、模式和斜杠命令都在 `session/new` 的响应里一次给全**，斜杠命令在响应后 2–5 ms 到达。只建会话、不发 prompt，两边都不会留下空会话。但预建有资源成本：Claude 每个会话一个 CLI 进程，约 250 MB；Codex 每个线程会拉起一套用户 MCP 服务器，而且 `session/close` 之后不释放。
7. **Codex 在 v1 下其实有运行状态。** 它通过 `session_info_update._meta.codex.threadStatus` 推送，取值有 `active`、`idle`、`systemError`，`active` 还带 `waitingOnApproval` 或 `waitingOnUserInput` 标记。Claude 没有对应的信号。
8. **codex-bundled 与 codex-local 在全部场景里协议行为一致**，没有因为版本不一致而出现实验 API 报错。

### 结果矩阵（12 个场景 × 2 个适配器）

Codex 列的结论对 bundled 和 local 都成立，个别不同的地方单独注明。录制文件位于 `spikes/SP-1-adapter-behavior/recordings/<adapter>/`。

| # | 场景 | Claude（claude-agent-acp 0.91.0） | Codex（codex-acp 2.2.2） | 录制 |
|---|---|---|---|---|
| 1 | `initialize`、配置项和命令下发的时机、预建会话 | `initialize` 约 0.15 秒。能力里有 `loadSession`、image、http 和 sse 的 MCP、list/resume/close/delete/fork、`_meta.claudeCode.promptQueueing`、`_meta.steering.supported`。<br>`session/new` 约 0.7–0.9 秒（要启动 CLI），响应里带 mode（5 种）、model、effort（`thought_level`）。斜杠命令在响应后约 2 ms 到达，每个回合开始时会再推一次。<br>只建会话不发 prompt：不写 transcript，不出现在列表里，重启后也看不到。只留下 `~/.claude/session-env/<id>` 目录。 | `initialize` 约 0.1–0.3 秒，本机冷启动时约 4.3 秒。能力多了 `subagents`；MCP 是 http（不支持 sse）；有 `steering.supported`，没有排队标记。<br>`session/new` 约 0.1–0.3 秒，响应里带 mode（4 种）、`collaboration_mode`、model、`reasoning_effort`，以及旧的 `models`。斜杠命令在响应后约 5 ms 到达。<br>只建会话不发 prompt：不写 rollout 文件，不出现在列表里。 | `s01-init-session` |
| 2 | 普通回合 | 消息 chunk 带 `messageId`。<br>haiku 下即使 effort 设为 high 也**没有思考流**。<br>工具调用先以 pending 出现，参数流式补全，最后变为 completed 或 failed，`_meta.claudeCode` 里有原始结果。<br>diff 是**带上下文的片段**，不是整个文件；`_meta` 里另有 `structuredPatch`。<br>**拿不到 `plan`**：TodoWrite 和 Task 系列工具在 SDK 会话里都不可用，设了 `CLAUDE_CODE_ENABLE_TASKS` 也不行。<br>`usage_update` 每次调用模型后都发 `{used,size}`，回合结束时带 `cost`，`_meta` 里有 5 小时和 7 天的额度窗口。回合结束后推 AI 生成的标题。 | 有 `agent_thought_chunk`（推理摘要）。<br>工具调用一出现就是 in_progress，参数完整。<br>diff **每个 hunk 一条**，带 `_meta.kind`（add 或 update）。模型有时改用 shell 运行 `apply_patch`，这时**完全没有 diff**。<br>本环境里 `update_plan` 不可用，所以没有 `plan`。<br>`usage_update` 只有 `{used,size}`。<br>先推一个由 prompt 截断成的标题，约 5 秒后再推 AI 标题（codex-acp 为此额外调用一次模型）。<br>有 `threadStatus` 运行状态信号。 | `s02-normal-turn`、`s02b-plan`、`claude/s02c-*`、`claude/s02d-*` |
| 3 | 执行命令 | 输出在命令结束时通过 `_meta.terminal_output` **一次性给出**，不流式；stdout 和 stderr 合在一起，第一行是 "Exit code N"。<br>退出码在 `_meta.terminal_exit.exit_code`，非 0 时工具状态为 `failed`。<br>`ls` 不需要审批，`sh -c` 需要。 | 一般命令的 `terminal_output` 是**逐行增量**推送的，退出码同样在 `_meta.terminal_exit.exit_code`。codex-local 的一次运行里，第一行输出丢失了。<br>`ls` 这类读取命令显示为 `kind: read`，没有终端相关的 meta，输出放在 `content` 文本里，退出码放在 `rawOutput.exit_code`。 | `s03-exec`、`s07-cancel` |
| 4 | 审批：允许一次和拒绝一次 | 被拒绝的工具调用变为 `failed`，**回合继续**，最后 `end_turn`。Bash 命令被拒绝也一样（s04b）。<br>"总是允许"类选项在 Write 上只在本会话有效，在 Bash 上会持久写入规则，但两者的 kind 都标为 `allow_always`。 | 选项是 `allow_once`、`allow_for_session` 或 `accept_execpolicy_amendment`（后者是持久的 exec policy），以及 `cancel`。<br>拒绝时只能选 `cancel`，**整个回合以 `cancelled` 结束**。文件修改和命令都这样（#556）。 | `s04-approval`、`s04b-reject-command` |
| 5 | 结构化提问和计划确认 | AskUserQuestion 走 `elicitation/create` 的 form 模式：每个问题一个 `question_N` 字段（`oneOf` 列出选项），另有一个 `question_N_custom` 自由文本字段。<br>计划确认：ExitPlanMode 会发一个 `request_permission`，kind 是 `switch_mode`，有 5 个选项。只有 `exit-plan-default` 是 allow_once；其余几个会切到 auto 或 bypass 模式，或者清空上下文。<br>批准后依次推 `current_mode_update`、`config_option_update`、`plan_update`（unstable，Markdown 格式）。计划文件会写到 `~/.claude/plans/`。 | request_user_input 走 `elicitation/create` 的 form 模式，同时线程状态带 `waitingOnUserInput`。<br>plan 模式下推 3 次 `plan_update`（Markdown）。<br>之后发 "Implement this plan?" 的 `request_permission`，选项是 `implement_plan` 和 `revise_plan`。批准后 `collaboration_mode` 切回 default，并**在同一个 `session/prompt` 里**继续实施。 | `s05-questions` |
| 6 | 回合中再发 prompt（排队）和 steering | 回合中再发 prompt：第二条并入当前回合；第一个 prompt 以 `end_turn` 返回，usage 全是 0，**它自己的回复丢失**。<br>steering：在工具调用刚出现时插话，返回 `injected`，回合没有失败，但原来的工具调用被丢弃，回合结束时才补发 `failed`。在命令执行中插话，返回 `injected`，Bash 被移到后台，回复在 prompt 返回之后才到。<br>空闲时插话：带 `idleBehavior: promptRequired` 返回 `promptRequired`；不带时返回 `startedNewTurn`。 | 回合中再发 prompt：第二个接管回合；**第一个 prompt 永远不返回**（我等到 180 秒超时）。<br>steering：两个时机都返回 `injected`，回合干净地结束。<br>空闲时插话：**忽略 `idleBehavior`**，直接返回 `startedNewTurn`。 | `s06-queue-steer` |
| 7 | 取消 | 命令执行中取消：约 20 ms 返回 `cancelled`，命令被终止。<br>审批挂起时取消：适配器先发 `$/cancel_request` 撤回审批请求，约 5 ms 后返回 `cancelled`。<br>回合中途 CLI 崩溃：约 8 ms 内返回 -32603，说明是 CLI 进程意外退出；之后的请求都返回 "Session not found"，需要显式 `session/resume`（s07b 验证有效）。 | 命令执行中取消：约 6–9 ms 返回 `cancelled`，但**命令没有被终止**，它的输出和 `completed` 后来才到。<br>审批挂起时取消：约 5–9 ms 返回，**不会撤回**审批请求，客户端要自己丢弃。<br>回合中途 app-server 被杀：约 10 ms 内返回错误 1001，`restartable` 为 true；下一个回合自动重启，会话可以透明地继续。CLI 在空闲时被杀也一样。 | `s07-cancel`、`s07b-*`、`claude/s07x-*` |
| 8 | 会话列表 | 一次返回 179 个会话，没有分页。字段是 `{sessionId,cwd,title,updatedAt}`。只列出至少有一个回合的会话。按 `cwd` 过滤时会把符号链接视为同一路径。 | 共 136–140 个，分 6 页，游标是 `updatedAt` 的时间字符串，字段相同。按 `cwd` 过滤**必须传真实路径**，否则得到 0 条；过滤后的空页也带 `nextCursor`。 | `s08-list` |
| 9 | 加载、回放、恢复 | 回放是压缩的（每条消息一个 chunk，工具调用直接以最终状态出现），在响应之前推完，约 0.9 秒。**没有时间戳**（#1307）。**`/compact` 之后的回放只剩摘要和最后一条消息**（#1297）。<br>SIGKILL 适配器后重启，`session/resume` 不回放，上下文可以接上。 | 回放同样压缩，约 0.1 秒，**没有时间戳**（#617）。压缩之后回放仍然完整。<br>崩溃重启后 `session/resume` 正常。 | `s09-load-resume` |
| 10 | 错误 | 未登录时推 `_auth/status_update {kind:"none"}`。`session/new` 成功，`session/prompt` 返回 -32000 "Authentication required"。<br>登录时，`_auth/status_update` 会带上**邮箱和组织**。 | 未登录时推同样的状态，`session/new` 直接返回 -32000。<br>模型不被支持时，错误以 `agent_message_chunk` 文本（原始 JSON）出现，线程状态变为 `systemError`，`stopReason` 仍是 `end_turn`。<br>上游过载时推 `session_info_update._meta.codex.error`，里面有 "Reconnecting... n/5" 和 `willRetry`。<br>没有遇到 429 或额度用尽。 | `s10-unauthenticated`、`codex-bundled/s10x-model-unsupported` |
| 11 | 图片输入 | 8×8 的红色 PNG，模型回答 "Red"。 | 同样回答 "Red"。 | `s11-image` |
| 12 | 进程和内存（RSS） | 1 个适配器进程，约 80–120 MB。**每个会话 1 个 CLI 进程**，约 220–290 MB，再加上该会话的用户 MCP。3 个空闲会话合计约 1.34 GB。`session/close` 之后全部释放。<br>关闭 stdin 后适配器约 5 ms 就退出，不等子进程；子进程大约 1 秒后自己退出。 | 1 个适配器进程，加 1 个 node 包装进程，加 **1 个 app-server 服务所有线程**（约 80–150 MB）。**每个线程各自一套用户 MCP**，本机约 400–480 MB。3 个空闲会话合计 1.54–1.63 GB。**`session/close` 之后 MCP 不释放**。<br>关闭 stdin 后约 2 秒退出。 | `s12-resources`，另见 s06、s07 中的运行采样 |

### R02 矩阵中 🟡 项和已知 issue 的实测结论

R02 矩阵里的 🟡 项：

| R02 项 | 实测结论 |
|---|---|
| 9 加载回放 | Claude 不完整（#1297 复现），没有时间戳（#1307 复现）。Codex 小会话回放完整，没有时间戳（#617 复现）。大会话的问题（#516/#539）没有测。 |
| 18 单次编辑的 diff | 两边都**不给整个文件**：Claude 给带上下文的片段，Codex 每个 hunk 一条。Codex 还可能完全没有 diff（模型通过 shell 运行 `apply_patch` 时）。R02 说 Claude 给"全文 diff"，这一点需要更正。 |
| 20 命令输出 | 两边都能拿到输出和退出码，但位置不统一。Claude 的输出不流式，Codex 逐行流式。 |
| 32 排队 | 两边都不是"排到下一回合"，详见核心结论第 1 条。 |
| 33 steering | 两边都可用。Claude 有副作用（工具调用被丢弃、命令被移到后台、产生不属于任何 prompt 的输出）。Codex 不支持 `promptRequired`。 |
| 34 运行状态 | Codex 在 v1 下通过 `_meta.codex.threadStatus` 提供，R02 写的"仅 v2"需要更正。Claude 没有。 |
| 22 计划 | R02 写的"TodoWrite 转 `plan`"在这个 CLI 版本的 SDK 会话里**拿不到**；两边能稳定拿到的只有 plan 模式的 `plan_update`（unstable）。 |
| 16 思考流 | Claude 在 haiku 下**没有观察到**；Codex 有。 |

已知 issue：

| issue | 结论 |
|---|---|
| #556（Codex 拒绝审批结束回合） | **复现，而且每次都这样**，文件修改和命令都是。 |
| #600（Codex 中途退出后 prompt 不返回） | **没有复现**，杀掉 app-server 后会快速返回 1001 错误。但并发 prompt 会导致 prompt 永远挂起，这是另一个原因。 |
| #1267（Claude 在工具流式输出时插话会让回合失败） | 没有以"回合失败"的形式复现，副作用见上表第 6 行。 |
| #1297、#1307、#617 | 都复现。 |

没有测的（不在剧本内，或者成本不划算）：#3 v2 协商、#5 API key、#11 删除即归档（#537）、#13 分叉、#14 AIR 扩展、#30 MCP sse、#38 子代理、#1269 客户端读得慢会卡住会话、#1280 自主回合中插话。

### 对 ADR 的建议

**ADR-06：事件模型与存储**

1. **宿主自己持久化带序号和时间戳的事件日志，以它为唯一的历史来源。** 适配器的回放只用于导入 CLI 里原有的会话，并且标注为"可能不完整"（PS-5）。理由：回放没有时间戳；Claude 压缩之后会丢历史；回放是压缩过的，不包含审批请求和 usage。
2. **事件流以会话为单位，不以 prompt 请求为单位。** 回合由宿主推导：
   - 回合的开始和结束依据 `session/prompt` 的请求和响应；
   - Codex 有 `threadStatus` 时优先用它；
   - 不属于任何 prompt 的 update，挂到一个"自发回合"上；
   - 回合结束时还没有终结的工具调用，统一标为"已放弃"。
3. **工具调用按 `toolCallId` upsert。** 命令输出和退出码由一层按适配器区分的提取逻辑来取：`_meta.terminal_exit`、`rawOutput.exit_code`，或者从 `content` 文本里取。拿不到时降级显示（MS-3）。diff 只用来展示过程，回合级的变更视图交给 git（SP-5）。
4. **定义统一的错误事件**，覆盖三类错误：
   - JSON-RPC 错误：-32000 认证、-32603、1001 可重启；
   - 以文本形式出现的错误：原始 JSON 放在 `agent_message_chunk` 里，同时线程状态为 `systemError`；
   - 可自动重试的上游错误：`_meta.codex.error` 且 `willRetry` 为 true。
5. **对路径做规范化。** 适配器返回的是真实路径，Codex 按 `cwd` 过滤时也要求真实路径，所以宿主存储 `cwd` 时一律先做 realpath。
6. **隐私。** `_auth/status_update` 带邮箱和组织，转发给移动端或写日志时要做最小化处理（SEC）。

**ADR-07：Agent 驱动路径**

- **Claude 用 claude-agent-acp。** 审批、提问、计划确认、取消、恢复都能用。它的短板宿主都能补上：自存历史、崩溃后自动 resume、自己实现排队。没有发现非改走 Agent SDK 不可的问题；主要问题在 CLI 本身。
- **Codex 首期用 codex-acp，并在宿主层规避它的问题**：
  - 自己实现队列，只在一个回合结束后再发下一条；
  - 超时兜底（CP-7）；
  - 路径规范化；
  - 取消之后，丢弃迟到的工具输出。
- **#556 是唯一会直接影响用户体验的点。** 建议把它写进 AP-2 的文案，例如"拒绝将结束本回合"。如果"拒绝后让 agent 换个做法"被定为 P0，就提前做 SP-3（直连 app-server）。
- **steering 作为可选的增强能力**：只在回合运行中才发，并且按返回的 `outcome` 处理 `startedNewTurn` 这种情况。

**ADR-09：适配器进程拓扑**

- **每种 agent 在每台 Mac 上一个适配器进程，多个会话共用**，不要每个会话起一个适配器。理由：Claude 本来就是每个会话一个 CLI 进程；Codex 本来就共享一个 app-server；每个会话再加一个适配器进程，会额外多出约 60–120 MB 的 node 开销。
- **Claude**：会话不用时及时 `session/close` 来释放 CLI。AG-4 的"新会话页"不建议常驻一个预建的会话，改为缓存每种 agent 最近一次的能力描述（这些信息都来自 `session/new` 的响应，静态且便宜），在用户真正发出第一条消息时再建会话。
- **Codex**：用户 MCP 服务器不会随 `session/close` 释放，宿主要监控适配器的 RSS，在空闲或超过阈值时回收（重启）适配器进程。会话可以通过 `session/resume` 恢复，代价很小。
- **关闭顺序**：先对所有会话调用 `session/close`，再关闭 stdin，再等待进程组退出，最后对整个进程组发 SIGTERM 或 SIGKILL 兜底。适配器要放在独立的进程组里启动，因为 Claude 适配器退出时不等子进程，它的 `auth status` 探测进程可能会短暂残留。
- **NFR-11 的粗略估算**：6 个会话时，只算 agent 本身，Claude 约 1.6 GB，Codex 约 0.3 GB。用户每配置一套 MCP，每个会话就要再加几百 MB，所以容量提示和会话回收需要做成产品功能。

**对 PRD 条目的影响**：
- AP-2："总是允许"类选项的 kind 区分不了"仅本会话"和"持久写入"，需要按 `optionId` 维护一张对照表，例如 Claude Write 的 `allow-with-updates` 只在本会话有效，Bash 的是持久的；Codex 的 `allow_for_session` 只在本会话有效，`accept_execpolicy_amendment` 是持久的。
- AP-5：两边的 form 都能用；Codex 的线程状态里有"等待输入"标记可以直接用。
- CP-6：只能由宿主排队。
- CP-7：两边取消都很快，但 Codex 取消后命令不会停。
- MS-3：退出码都拿得到。
- PS-5：回放受限，不能依赖。
- PS-8：两边重启后都能 resume。

## 关键取舍

- **客户端用 TS 手写 JSON-RPC，不用 SDK**：
  - 目的是录下线上的原样帧，包括 `_auth/status_update`、`$/cancel_request`、各种 `_meta`；
  - 需要故意制造并发和时序（并发 prompt、在工具调用刚出现时插话、挂起审批、杀掉子进程），手写更容易控制；
  - Node 24 能直接运行 `.ts`，不需要安装依赖或编译。

  宿主按 ADR-02 用 Rust，录制文件本身与语言无关，可以直接给假 agent 和回归测试用。
- **模型和推理强度**：Claude 用 haiku、effort low；Codex 原计划用 `gpt-5.4-mini`，但网关拒绝了它，改用 `gpt-5.6-luna`（codex-acp 生成标题时自己也用这个模型），推理强度 low。只有 s02d 为了观察思考流，有一个回合用了 effort high。
- **权限模式**：只用 Claude 的 `default`、`plan`，以及 Codex 的 `workspace-write`、`read-only`。后两者都比 codex-acp 默认的 Auto review 严格。代码在每次发 prompt 之前都会检查模式和模型。
- **另外补的几个场景**（s02b/c/d、s04b、s07b）用来回答矩阵里留下的空白。作废的运行有两种处理：能说明问题的保留下来，作为补充样本（`claude/s07x`、`codex-bundled/s10x`）；没有信息量的直接删掉，在 notes 里说明。
- **没有测的项目**：大会话回放、#1269、分叉、删除，都不在剧本内，或者成本不划算，已在遗留问题中列出。

## 验证方式

- 运行：`node run.ts --base <tmp> --adapter <claude|codex-bundled|codex-local> --scenarios ...`，每个场景结束后用 `inspect.ts` 逐帧核对时间线。
- 脱敏：`sanitize.ts` 自带的复扫结果是 "scan clean"。另外对录制、notes 和 data 独立 grep 了用户名、邮箱、家目录、`/var/folders`、临时目录 ID、主机名、组织名、个人 skill 名和 token 样式，没有命中。唯一的邮箱字符串是 `run.ts` 里的占位地址 `sp1@example.invalid`。
- 调用次数（大约）：Claude 31 个回合，codex-bundled 约 25 个，codex-local 约 21 个，都包含作废的运行。此外还有 CLI 自己的后台调用（生成标题、网关过载时的重试），录制里看不到。

对照验收标准：

| 验收标准 | 情况 |
|---|---|
| 12 个场景 × 2 个适配器的结果矩阵，写明实际行为并引用录制文件 | ✅ 见上文 |
| 逐条说明 R02 的 🟡 项目，以及 #556、#600、#1267、#1297 等已知问题的实测结论 | ✅ 见上文，没测的单独列出 |
| 给 ADR-06、ADR-07、ADR-09 的建议 | ✅ |
| 录制文件已脱敏，并附脱敏脚本 | ✅ `sanitize.ts`，README 里写了规则 |
| 列出本机遗留的会话记录和临时目录 | ✅ 见下文 |

## 提交列表

```
2e56b2d docs(task): add SP-1 working notes and extracted data
1d23035 test(spike): add sanitized SP-1 adapter recordings
8a81b26 chore(spike): add SP-1 recording sanitizer and summary extractor
25675e8 refactor(spike): simplify SP-1 quiet wait and rollout lookup
7725365 chore(spike): add SP-1 ACP probe client and scenario scripts
<编排者存档提交>
```

## 对用户配置的改动（编排者脱敏摘要）

- `~/.claude/settings.json` 和 `~/.codex/hooks.json`：实测前后的 sha256 一致。
- `~/.codex/config.toml`：sha256 与实测前不一致，修改发生在实测期间。经子 agent 核查：
  - 文件里没有本次任何临时目录的信任项；
  - 本次实测没有调用 codex-acp 中唯一会写配置的代码路径；
  - 本次实测从来没有选过 `allow_always` 类的选项。

  无法确定是谁改的。同一时段本机的 Codex 桌面应用和 SP-4b 都在运行。按要求，子 agent 没有复制、编辑或回滚这个文件。
- Claude Code 在 plan 模式下往 `~/.claude/plans/` 写了一个计划文件。这是数据文件，不是配置。

## 本机遗留的会话记录和临时目录（编排者脱敏摘要）

- 一个临时目录，包括适配器的安装、测试仓库、**未脱敏的原始录制**、空配置目录、本地脱敏词表和运行日志。
- Claude Code：一个测试仓库对应的 project 目录（13 个 transcript，加一个空的 memory 目录）；`session-env/` 下 17 个目录；1 个计划文件；一处后台任务输出。
- Codex：`~/.codex/sessions/2026/10/16/` 下 24 个 rollout 线程。它们也会出现在 Codex 桌面应用的历史里。
- 没有残留进程。

具体路径和 ID 见不入库的 `.private/cleanup/SP-1-leftovers.md`，是否清理由用户决定。

## 遗留问题 / 范围外发现

1. **`~/.codex/config.toml` 的改动来源不明**，详见"对用户配置的改动"一节，需要用户自己核对。另外建议以后的实测在开始前保存一份私有副本，不入库，以便事后比对。
2. **Codex 的结论受自定义网关影响**：模型目录、`gpt-5.4-mini` 不可用、频繁过载重连、`update_plan` 不可用，都可能是这个环境特有的。用 ChatGPT 登录的 Codex 在计划和模型方面可能表现不同，建议在用户的另一台标准环境上抽查 s02b 和 s06。
3. **Claude 的 `plan` 和思考流在这个 CLI 版本上没拿到**：TodoWrite 和 Task 工具在 SDK 会话里不可用，haiku 也没有思考流。需要确认是 CLI 的设定还是模型的原因。如果产品需要 todo 计划，可能要依赖 plan 模式的 `plan_update`（unstable）。
4. **没有测的项目**：大会话回放（#516/#539）、客户端读得慢会卡住会话（#1269）、自主回合中插话（#1280）、`session/fork`、`session/delete`（#537）、v2 协商、API key 认证。如果 ADR-06 需要这些依据，可以在 SP-4a 的宿主骨架上补测。
5. **范围外发现**：
   - 两个 CLI 都会加载用户全局配置的 MCP、hooks、插件和 notify。"使用本机 CLI"意味着 cupper 的会话会继承用户的整套工具，包括会访问外部系统的 MCP。这涉及 SEC 和隐私边界，建议在 PRD 或 ADR-08 里明确：是否需要限制，以及如何告知用户。
   - Codex 每个线程的 MCP 不随 `session/close` 释放，可以考虑反馈给上游。
   - 本次没有遇到 HTTP 429；遇到的上游错误是 "servers are currently overloaded" 导致的流断开，Codex 自动重试最多 5 次后成功。我没有切换过账号或认证方式。
