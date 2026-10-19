# T003 工作笔记

- 执行者：子 agent（原型实测工程师）
- 日期：2026-10-19
- 代码与录制：`spikes/T003-tech-validation/`（说明见其中的 `README.md`）

## 环境与运行清单

| 项 | 取值 |
|---|---|
| 机器 | Apple Silicon 的 Mac，macOS 26 |
| Node | v24.13.0（运行适配器和 TS 脚本） |
| Claude Code | 2.1.296（Homebrew 安装的原生二进制，经 `CLAUDE_CODE_EXECUTABLE` 传给适配器） |
| 适配器 | `@agentclientprotocol/claude-agent-acp` 0.91.0（锁定版本，内含 ACP TS SDK 1.8.0、claude-agent-sdk 0.3.300）；0.93.0（候选，ACP TS SDK 1.9.0、claude-agent-sdk 0.3.300） |
| Rust | cargo 1.96.0；`agent-client-protocol =3.3.0`（schema 1.11.0） |
| 配置 | 真实配置（U-R4）：CLI 会加载用户全局的 MCP、hooks 和插件，进程树里能看到它们 |

| 运行 | 内容 | 模型调用（haiku） |
|---|---|---|
| `q8-claude-0.91.0` | 3 轮 × 2 个会话 × 3 个 prompt；之后第一次 hold 尝试（失败，见试错记录） | 18 + 1 |
| `q8-claude-0.91.0-hold` | hold（A 的 Write 审批挂起 15 秒，B 发 1 个 prompt）+ killgroup（`sleep 20`） | 3 |
| `q8-claude-0.93.0` | 候选版本：1 轮 + hold | 6 + 2 |
| `q9-probe` | 每次 3 个短回合（数数并插话、写文件后取消、数数后丢弃请求）；共跑 5 次，前 3 次的探针有一个字段读错，见试错记录 | 15 |

合计约 45 个 haiku 回合，其中 10 个在开头几秒就被取消。另有 CLI 自己的后台调用（例如生成标题），录制里看不到。

## Q8：同一个适配器进程上两个会话并发

剧本（`q8.ts`）：一个适配器进程，每轮先并发 `session/new` 两个会话（同一个临时仓库），各自显式设为 `default` 模式、haiku、最低推理强度；然后两个会话**同时**开始，各自顺序发 3 个 prompt：

1. 只回显一个随机令牌；
2. 用 Bash 运行 `sleep 3; echo <令牌>`，再回显输出；
3. 用 Write 新建 `<a|b>-r<轮次>.txt`，内容是令牌，再回复 `done <令牌>`（需要审批，答 `allow_once`）。

令牌每个 prompt 不同，所以任何一帧里出现另一个会话的令牌就是串话。判定逐项检查：所有 prompt 返回 `end_turn`；一轮在 5 分钟内结束；没有串话；审批请求的 `sessionId` 是正在发 prompt 的会话，且工具参数里是本会话的令牌；文件内容正确；会话没有 prompt 在途时不收到内容类更新。

| 轮 | 两个 `session/new` 并发耗时 | 一轮用时 | 两边同时在途 | 返回 `end_turn` | 回显正确令牌 | 单个 prompt 耗时 | 审批 | A/B 更新交替次数 | 串话 | 文件 | 判定 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 0.97 s | 13.4 s | 11.3 s | 6/6 | 6/6 | 1.6–7.1 s | 2 | 7 | 0 | A、B 正确 | 通过 |
| 2 | 0.86 s | 12.6 s | 10.8 s | 6/6 | 6/6 | 1.6–7.1 s | 2 | 12 | 0 | A、B 正确 | 通过 |
| 3 | 0.89 s | 13.6 s | 10.4 s | 6/6 | 6/6 | 1.5–8.2 s | 2 | 10 | 0 | A、B 正确 | 通过 |
| 候选 0.93.0 | 1.10 s | 12.1 s | 11.4 s | 6/6 | 6/6 | 1.8–6.3 s | 2 | 14 | 0 | A、B 正确 | 通过 |

- 审批只来自第 3 个 prompt（Write）。Claude Code 把 `sleep 3; echo X` 归为只读命令，不发审批。
- "A/B 更新交替次数"是适配器 stdout 上内容类更新在两个会话之间切换的次数，说明两个回合确实在同一条管道上交织进行。
- 回合外的内容类更新：0 次（四次运行都是）。
- 逐 prompt 的数据见 `recordings/results/q8-claude-0.91.0.results.json`、`q8-claude-0.93.0.results.json`。

**hold（上游 #830：一个会话的审批挂起会不会挡住同一进程上的其他会话操作）**

| 指标 | 0.91.0 | 0.93.0 |
|---|---|---|
| A 的 Write 审批挂起时长 | 15.0 s | 15.0 s |
| 期间 B 的 prompt | `end_turn`，1.5 s | `end_turn`，1.5 s |
| 期间 `session/new` | 成功，0.81 s | 成功，0.76 s |
| 期间 `session/list` | 成功，19 ms | 成功，17 ms |
| 期间 `session/close` | 成功 | 成功 |
| 放行后 A | `end_turn`，文件正确 | `end_turn`，文件正确 |

#830（2026-07 已关闭）在锁定版本上没有复现。

## Q9：ACP Rust SDK 3.3.0 的扩展消息

探针（`q9-probe/`）在适配器的 stdio 和 SDK 之间加了一层按行转发的分接，记录每一帧，用来对照 SDK 处理器看到的内容。两个适配器版本结果相同（`recordings/results/q9-probe-claude-*.findings.json`）。

| 项 | 做法 | 结果 |
|---|---|---|
| 发送 `_session/steering` | 用 SDK 的 `#[derive(JsonRpcRequest)]` 声明 `method = "_session/steering"` 的请求和响应类型，`cx.send_request(..).block_task()` | 空闲时（带 `_meta.steering.idleBehavior: promptRequired`）返回 `{outcome: promptRequired, reason: noRunningTurn}`；回合中返回 `{outcome: injected}`，回合以 `end_turn` 结束，回复里有插话要求的单词 |
| 能力位 | `initialize` 响应顶层的 `_meta.steering.supported`（不在 `agentCapabilities._meta` 下） | 线上为 true；SDK 解析成 `InitializeResponse` 再序列化后仍保留 |
| 收到 `_auth/status_update` | 用 `#[derive(JsonRpcNotification)]` 声明 `method = "_auth/status_update"`，`on_receive_notification` 注册 | 处理器收到，`kind: account`。这条通知不带 `sessionId` |
| 收到 `$/cancel_request` | Write 的审批请求挂住不答，1 秒后发 `session/cancel` | 线上 3–4 ms 后适配器发出 `$/cancel_request`，`requestId` 等于审批请求的 id，再过 5 ms prompt 返回 `cancelled`；SDK 先把该请求的 `Responder::cancellation()` 标记置为已取消（`cancelled().await` 返回），再交给处理器链，`CancelRequestNotification` 的处理器也收到了；文件没有写出；之后再回 `Cancelled` 也能正常发出 |
| 丢弃在途的 prompt 请求 | 收到第一段输出后 `drop` 掉 `session/prompt` 的 `SentRequest` | SDK 自动发出 `$/cancel_request`（`requestId` 是该 prompt 的 id）；适配器把它当作取消回合，5–6 ms 后返回 `stopReason: cancelled`，usage 全为 0 |

依据的 SDK 源码（crate `agent-client-protocol` 3.3.0）：

- `src/concepts/cancellation.rs`：`$/cancel_request` 的双向支持；"Dropping a SentRequest before the SDK receives a response also sends `$/cancel_request`"；`SentRequest::detach()` 关闭这一行为。
- `src/jsonrpc/incoming_actor.rs` 的 `dispatch_dispatch`：先 `cancel_if_requested` 更新标记，再走用户处理器、动态处理器、角色默认处理器。
- `src/role/acp.rs`：Client 角色的默认处理器对带 `sessionId` 的未处理消息返回 `retry: true`；`incoming_actor.rs` 把它们放进 `pending_messages`，只在新增动态处理器时重试。
- claude-agent-acp 0.91.0 `dist/acp-agent.js` 的 `runPromptWithCancellation`：`session/prompt` 的 abort 信号（收到 `$/cancel_request` 时触发）会走 `agent.cancel`。

`q9-probe/tests/unhandled.rs` 在内存管道上用假 agent 验证了上面第 3 条：

- 只注册类型化处理器时，agent 发来的未知请求如果带 `params.sessionId`，**永远收不到响应**；不带 `sessionId` 的收到 "method not found"。
- 在最后加一个 `on_receive_dispatch`（`Dispatch<UntypedMessage, UntypedMessage>`：请求回 method not found，通知丢弃，响应照常转发）之后，两者都收到 "method not found"。

## SP-4b 遗留第 7 条：工具命令的会话和进程组

适配器按宿主的方式拉起（自成会话和进程组，pid = pgid = sid）。每秒采样一次适配器的整棵进程树，用 `ps` 取 pgid、用 getsid(2) 取会话 ID（macOS 的 `ps -o sess` 恒为 0）。下表是第 1 轮两个会话同时运行 `sleep 3` 时的一次采样，进程号换成了角色：

| 进程 | 父进程 | 进程组 | 会话 | 组长 | 会话首进程 |
|---|---|---|---|---|---|
| adapter (node) #1 | 拉起它的客户端 | 自己的 | 自己的 | 是 | 是 |
| claude CLI #1 | adapter | adapter 的 | adapter 的 | 否 | 否 |
| zsh #1（`zsh -c source <快照> …`） | claude CLI #1 | 自己的（新组） | 自己的（新会话） | 是 | 是 |
| sleep #1 | zsh #1 | zsh #1 的 | zsh #1 的 | 否 | 否 |
| claude CLI #2 | adapter | adapter 的 | adapter 的 | 否 | 否 |
| zsh #2 | claude CLI #2 | 自己的（新组） | 自己的（新会话） | 是 | 是 |
| sleep #2 | zsh #2 | zsh #2 的 | zsh #2 的 | 否 | 否 |
| 用户配置的 MCP 和 hooks（6 个） | 各自的 CLI | adapter 的 | adapter 的 | 否 | 否 |

全部 23 次采样（两个适配器版本）都一样：

- CLI 进程，以及 CLI 拉起的 MCP 服务器、hooks，都留在适配器的会话和进程组里；
- **Bash 工具的命令，每次都在一个新会话里运行**：CLI 拉起的 `zsh` 是新会话的首进程，也是新进程组的组长（等价于拉起时调用了 setsid），命令是它的子进程。

**killgroup**（`q8-claude-0.91.0-hold.results.json` 的 `killGroup`）：让 agent 运行 `sleep 20`，确认 `sleep` 在运行后，对适配器的进程组发 SIGKILL（设计 5.6.8 关闭顺序的最后一步）。1 秒后，适配器、CLI、MCP 都已退出，**`zsh` 和 `sleep 20` 还活着**（不在适配器的组和会话里，已被 launchd 收养）。脚本随后用 SIGTERM 结束了它们。另外，这一步之后 `~/.claude/shell-snapshots/` 多了一个快照文件，时间与此吻合，很可能是被杀的 CLI 来不及清理留下的（其余运行过 Bash、正常关闭的会话都没有留下快照）。

## 版本复核（2026-10-19 查询）

| 组件 | 设计锁定或起始 | 当天最新 | 说明 |
|---|---|---|---|
| claude-agent-acp | 0.91.0（2026-10-15） | 0.93.0（2026-10-16 19:21Z）；预览 0.93.1-preview.2 | 0.92.0：AIR 工具调用加 MCP 字段、新增 `_meta.promptSuggestions`、ACP TS SDK 升到 1.9.0（v2 规范澄清）、子 agent 的修复。0.93.0：**新增 `/remote-control` 命令**（"Continue this session from claude.ai/code or the Claude mobile app"，出现在 `available_commands_update` 里，单独一行 `/remote-control` 的 prompt 会被适配器拦下执行）、fork 列表的修复。claude-agent-sdk 仍是 0.3.300。Q8、Q9 在 0.93.0 上都通过 |
| codex-acp | 2.2.2 | 2.2.2；预览 2.2.3-preview.15 | 无变化 |
| Electron | 44.8.0（2026-10-16） | 44.9.0（2026-10-17）；45.0.0-beta.5 | 44 线的补丁版本 |
| Claude Code CLI | 已验证范围上限 2.1.296 | latest 2.1.303（2026-10-18），stable 2.1.292 | 本机 2.1.296 |
| codex CLI | 已验证范围上限 0.163.0 | 0.163.0；alpha 0.164.0-alpha.3 | 无变化 |
| ACP Rust SDK | =3.3.0（2026-10-09） | 3.4.0（2026-10-16）：schema 升到 1.12.0、v2 生命周期对齐、协议版本协商的修复 | Q9 只在 3.3.0 上验证 |
| Node 24 LTS | 24.22.0 | 24.22.0（2026-10-14） | 无变化 |
| electron-builder | 26.x | latest 26.15.3；`v26` 标签 26.17.0 | — |
| electron-updater | 6.8.x（源码核对的是 6.8.10） | latest 6.8.9；`v26` 标签 6.8.10 | npm 的 `latest` 标签落后于 `v26` 标签 |

## 测量闸门的负载数字（设计 5.7.1）

`load-rates.ts` 读 SP-1 的全部录制（claude、codex-bundled、codex-local）和本次的 Q8 录制，按会话流统计；线路帧按宿主 40 ms 写入合并折算（第一条更新开一批，40 ms 内同一流的更新并入同一批）。字节数用 ACP 的 JSON 长度近似。完整结果：`data/load-rates.json`。

| 口径 | 原始更新峰值 | 合并后线路帧峰值 | 字节峰值 | 每帧大小 p50 / p95 / 最大 |
|---|---|---|---|---|
| 内容类更新（消息、思考、工具调用、计划） | 55 条/s（codex s05） | **14 帧/s** | **14.4 KB/s** | 0.4 KB / 3.6 KB / 5.8 KB |
| 全部更新（加上命令列表、配置、标题、用量） | 55 条/s | 14 帧/s | 45 KB/s（脱敏后的录制）；本机原始录制 56 KB/s | 0.5 KB / 22.6 KB / 25.5 KB（原始 28.7 KB） |
| 历史回放（`session/load`，宿主不用） | 18–20 条/s | 1 帧/s | 8–14 KB/s | — |

- 40 ms 合并下每条流的理论上限就是 25 帧/s，实测最高 14 帧/s。
- 每帧约 2 KB 对内容类更新偏保守（p95 3.6 KB，但平均远小于 2 KB；每条流 25 × 2 KB = 50 KB/s，实测最高 14.4 KB/s）。
- 唯一超出的是 Claude 的 `available_commands_update`：每个会话开始和每个回合开始各推一次完整列表，本机约 22–29 KB（大小取决于用户装了多少 skill 和插件）。

## 配置文件的前后对比

`config-hash.ts`，运行前（第一次拉起适配器之前）和运行后（所有真实运行结束、确认没有残留进程之后）各取一次：

| 字段 | 结果 |
|---|---|
| `~/.claude/settings.json` | 未变 |
| `~/.claude/settings.local.json` | 前后都不存在 |
| `~/.claude.json` 顶层：`mcpServers`、`bypassPermissionsModeAccepted`、`autoModeEnvSetup`、`autoPermissionsNotificationCount`、`hasResetAutoModeOptInForDefaultOffer`、`hasSeenAutoModeEntryWarning`、`hasSeenAutoModeOutsideReadPrompt` | 全部未变 |
| `~/.claude.json` 每个已有项目（58 个）的 `allowedTools`、`hasTrustDialogAccepted`、`mcpServers`、`enabledMcpServers`、`disabledMcpServers`、`enabledMcpjsonServers`、`disabledMcpjsonServers`、`hasClaudeMdExternalIncludesApproved` | 全部未变 |
| `~/.claude.json` 的项目条目 | 没有新增，也没有删除（经 SDK 建的会话不写项目条目） |
| 测试仓库里的 `.claude/settings.local.json` | 没有生成 |

## 本机遗留（摘要）

完整清单（含路径和会话 ID）在不入库的 `.private/cleanup/T003-leftovers.md`：

- 一个临时目录：两个版本的适配器安装、3 个测试仓库、未脱敏的原始录制和 stderr 日志、脱敏中间产物、配置哈希快照、本地词表、cargo 构建目录。
- Claude Code：3 个项目目录（19 个 transcript，各带空的 memory 目录）；`session-env/` 下 22 个目录（19 个有 transcript 的会话，加 3 个只建未发 prompt 的会话）；`/private/tmp/claude-<uid>/` 下 2 个后台任务输出目录和 1 个 `cache-break-state` 文件；1 个 shell 快照（见 killgroup）。
- 没有残留进程。

## 试错记录

1. **第一次 hold 失败**：原来让 A 运行 `sleep 8; echo <令牌>` 并挂起它的审批，但 Claude Code 把这类命令归为只读，不发审批，脚本等了 120 秒后报错退出（`q8-claude-0.91.0.results.json` 里的 `error` 就是这次）。三轮主数据不受影响。改成挂起 Write 的审批后重跑，单独记为 `q8-claude-0.91.0-hold`，并加上 killgroup 步骤。
2. **Q9 探针读错了能力位的位置**：一开始在 `agentCapabilities._meta` 下找 `steering.supported`，得到 null；它实际在 `initialize` 响应的顶层 `_meta`。改完后有一次只跑了 `cargo clippy` 没有 `cargo build`，用旧二进制又跑了两次。最后重新构建，两个版本各跑一次，入库的是这两次。前三次的扩展消息结论与最后两次相同。
3. **脱敏**：SP-1 的 `sanitize.ts` 只映射 `procs`、`tree.procs`、`victim.label` 下的进程标签，SP-1 客户端在 `stop()` 里记录的 `procsAtExit` 没有被映射，原样留下了用户 MCP 服务器的命令行。本次的第二遍脱敏把所有带 `pid` 或 `role` 的对象都换成角色名，并处理了从 claude.ai 同步的命令和插件命令。SP-1 已入库的录制里有同样的问题，记在汇报的遗留问题里。
4. SP-1 客户端的 `spawn` 没有 setsid 选项，用一行 Perl（`POSIX::setsid` 后 exec node）补上，进程号不变。
