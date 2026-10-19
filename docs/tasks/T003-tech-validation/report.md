# T003 汇报

- 执行者：子 agent（原型实测工程师）
- 分支：`spike/T003-tech-validation`
- 完成日期：2026-10-19

## 做了什么

### K1 结论（先看这里）

| 项 | 结论 | 对驱动层的含义 |
|---|---|---|
| **Q8（Claude）** | **通过**。在锁定版本 0.91.0 上跑了 3 轮，全部通过；候选版本 0.93.0 跑 1 轮，也通过。上游 #830（一个会话的审批挂起时，同一进程上的其他会话操作被挡住）没有复现 | 按原设计写 T050 和 T055，**不需要派 T057**。ADR-09"每种 agent 共用一个适配器进程"的前提成立 |
| **Q9** | **三项 SDK 3.3.0 都直接支持**：`_session/steering` 用 SDK 的派生宏声明成请求类型，发出后能收到响应并对上号；`_auth/status_update` 由类型化的通知处理器收到；`$/cancel_request` 由 SDK 自己处理，会先把对应请求的 `Responder::cancellation()` 标记为已取消，再交给处理器 | T050 的扩展请求走 SDK，**线路分接层不需要在 stdin 侧自己发请求、关联响应**。分接层只剩三件事：编号并记录原始帧；结构化脱敏（包括 `_auth/status_update` 里的账号字段）；K19 的枚举容错改写。另外 T050 必须处理 SDK 的两个坑，见下文 |
| **SP-4b 遗留第 7 条** | **Claude Code 拉起工具命令时会 setsid**。每条 Bash 命令都跑在 CLI 新建的会话里：`zsh -c` 是新会话的首进程，也是新进程组的组长。它和适配器既不在同一会话，也不在同一进程组。CLI 本身和它拉起的 MCP、hooks 留在适配器的组里 | 血缘检查（5.8.4）对 Claude 的工具命令只剩"祖先链"一个信号可用；按进程组清理（5.6.8、K21）杀不掉正在运行的命令。建议见下文 |
| **版本复核** | 两个适配器、CLI 和 Electron 的锁定版本都不用改；Electron 起始版本可以顺手换成 44.9.0；测量闸门的负载数字不用改 | 详见下面两节 |

### Q8 逐轮数据

剧本：一个适配器进程，每轮先**并发**新建两个会话。两个会话用同一个临时仓库作为 cwd，都显式设为 `default` 模式、haiku、最低推理强度。然后两个会话**同时开始**，各自依次发 3 个 prompt：

1. 回显一个随机令牌；
2. 用 Bash 运行 `sleep 3; echo <令牌>`；
3. 用 Write 新建文件，内容是令牌。这一步需要审批，回答 `allow_once`。

每个 prompt 的令牌都不同。只要任何一帧里出现另一个会话的令牌，就算串话。

| 轮 | 一轮用时 | 两边同时在途 | 返回 `end_turn` | 回显正确令牌 | 单个 prompt | 审批归属正确 | A/B 更新交替 | 串话 | 回合外的内容更新 | 文件 | 判定 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 13.4 s | 11.3 s | 6/6 | 6/6 | 1.6–7.1 s | 2/2 | 7 次 | 0 | 0 | 正确 | 通过 |
| 2 | 12.6 s | 10.8 s | 6/6 | 6/6 | 1.6–7.1 s | 2/2 | 12 次 | 0 | 0 | 正确 | 通过 |
| 3 | 13.6 s | 10.4 s | 6/6 | 6/6 | 1.5–8.2 s | 2/2 | 10 次 | 0 | 0 | 正确 | 通过 |
| 候选 0.93.0 | 12.1 s | 11.4 s | 6/6 | 6/6 | 1.8–6.3 s | 2/2 | 14 次 | 0 | 0 | 正确 | 通过 |

另外补测了 hold：会话 A 的 Write 审批挂起 15 秒。这期间，会话 B 的 prompt 在 1.5 s 内正常返回，`session/new` 用了 0.8 s，`session/list` 用了 19 ms，`session/close` 正常。A 放行后正常结束，两个版本表现相同。

### Q9 的细节和 T050 必须处理的两个坑

- 线上实测：发出 `session/cancel` 后 3–4 ms，适配器发来 `$/cancel_request`，其中的 `requestId` 正是挂起的那个审批请求；再过 5 ms，prompt 返回 `cancelled`。
- 能力位 `steering.supported` 在 `initialize` 响应顶层的 `_meta` 里，不在 `agentCapabilities._meta` 下。SDK 解析后仍然保留这个字段。
- **坑 1：没有处理器的请求永远收不到响应。** agent 发来的未知请求如果带 `params.sessionId`，Client 角色的默认处理器会把它标成"稍后重试"，放进 `pending_messages`，再也不回，agent 那边就一直挂着。同样带 `sessionId` 的未知通知会一直堆在内存里。`q9-probe/tests/unhandled.rs` 用假 agent 复现了这一点，也验证了解法：在处理器链的最后注册一个无类型的 `on_receive_dispatch`，请求回 method not found，通知直接丢弃，响应照常转发。
- **坑 2：丢弃在途的 prompt 会取消回合。** 如果在 SDK 收到响应之前丢弃 `SentRequest` 或它的 future，SDK 会自动发出 `$/cancel_request`。用 timeout 或 `select!` 包住等待时很容易踩到。claude-agent-acp 收到后会取消这个回合，实测 5 ms 后返回 `cancelled`。驱动层只想放弃等待时，要先调用 `detach()`。K1 规避里"上一个 prompt 一直不返回"的兜底，要显式发 `session/close`，不能靠丢弃 future。
- 建议修订设计 5.6.10 第 3 条（Q9 已有结论），以及 5.6.1 表里分接层"把 SDK 不处理的扩展消息交给 AgentRuntime"的描述。

### SP-4b 第 7 条：进程树（脱敏后，进程号换成了角色）

| 进程 | 父进程 | 进程组 | 会话 |
|---|---|---|---|
| adapter（node，按宿主的方式 setsid 拉起） | 客户端 | 自己的 | 自己的 |
| claude CLI ×2 | adapter | adapter 的 | adapter 的 |
| zsh `-c` ×2 | 各自的 CLI | **新组（组长）** | **新会话（首进程）** |
| sleep ×2 | zsh | 跟随 zsh | 跟随 zsh |
| 用户配置的 MCP 和 hooks | CLI | adapter 的 | adapter 的 |

两个版本共 23 次采样，结果一致。

杀进程组的补测：让 agent 运行 `sleep 20`，然后对适配器的进程组发 SIGKILL。1 秒后，适配器、CLI 和 MCP 都退出了，**`zsh` 和 `sleep 20` 仍在运行**（已被 launchd 收养）。脚本随后用 SIGTERM 结束了它们。

建议：

- **5.8.4 血缘检查**：对 Claude 的工具命令，"同一会话"和"同一进程组"两个信号永远命中不了，只剩祖先链。命令一旦经 nohup、双重 fork 或 disown 脱离父进程，三个信号全部失效。把这个结论写进 5.8.4，T038 按此设定血缘信号的期望；主要防线仍是"写操作必须由用户确认"。
- **5.6.8 和 K21 资源回收**：关闭顺序里"按进程组清理"不够。可以在关闭前枚举适配器的全部后代，按各自的进程组逐个发信号；或者先 `session/close`，让 CLI 自己正常退出。假 agent 测不出这个问题，建议在 T066 的兼容剧本里加一个"关闭时有命令正在运行"的场景。

### 版本复核与建议

| 组件 | 锁定或起始版本 | 当天最新 | 建议 |
|---|---|---|---|
| claude-agent-acp | 0.91.0 | 0.93.0（10-16） | **M1a 保持 0.91.0**：SP-1 和 T003 的录制都基于它，0.92 和 0.93 的改动 M1a 用不到。升级放到 T066 的兼容运行。0.93.0 新增了 `/remote-control` 命令，作用是把会话交给 claude.ai/code 或手机 App 继续操作；升级前需要安全评审，并由宿主拦截这条命令。Q8 和 Q9 在 0.93.0 上都通过 |
| codex-acp | 2.2.2 | 2.2.2 | 不变 |
| Electron | 44.8.0 | 44.9.0（10-17），45 还在 beta | T074 起始版本改为 44.x 当时的最新补丁版本（今天是 44.9.0） |
| Claude Code CLI | 已验证到 2.1.296 | latest 2.1.303，stable 2.1.292 | 由 T066 扩展已验证范围 |
| codex CLI | 已验证到 0.163.0 | 0.163.0 | 不变 |
| ACP Rust SDK | =3.3.0 | 3.4.0（schema 1.12.0、协议版本协商修复） | T050 保持 3.3.0；以后升级时重跑探针和 `tests/unhandled.rs` |
| Node 24 LTS | 24.22.0 | 24.22.0 | 不变 |

**测量闸门的负载数字不用改。** 统计范围是 SP-1 的全部录制加上本次的录制，按宿主 40 ms 写入合并折算成线路帧：

- **内容类更新**：每条流最高 14 帧/s（40 ms 合并下的理论上限是 25 帧/s）、14.4 KB/s。每帧大小 p50 0.4 KB、p95 3.6 KB、最大 5.8 KB。现在的"每秒 25 帧 × 约 2 KB = 50 KB/s"偏保守。
- **唯一的例外**是 Claude 的 `available_commands_update`。每个会话开始时和每个回合开始时，它都会推一次完整的命令列表，本机为 22–29 KB，大小取决于用户装了多少 skill。建议宿主对 `CommandsChanged` 去重、不放进会话流；如果要放进会话流，就在闸门负载里加上"每个回合 1 个约 32 KB 的大帧"。

### 产出

- `spikes/T003-tech-validation/`：
  - `q8.ts`、`common.ts`：Q8 剧本和共用部分，复用 SP-1 的 `acp-client.ts`；
  - `q9-probe/`：Rust 探针和测试；
  - `config-hash.ts`、`load-rates.ts`、`sanitize-t003.ts`；
  - `README.md`；
  - `recordings/`：脱敏后的录制和逐项结果。
- `docs/tasks/T003-tech-validation/notes.md`（逐轮数据、试错记录）、`data/load-rates.json`。
- 遗留清单：已写入主仓库的 `.private/cleanup/T003-leftovers.md`，不入库。

## 关键取舍

- **两个会话用同一个 cwd**：两个会话共用一个 Claude 项目目录，条件比分开用 worktree 更苛刻。每轮新建会话，顺带测 `session/new` 的并发；所有轮次都在**同一个**长期运行的适配器进程里，符合 ADR-09。
- **用令牌判串话**：比较输出语义不可靠，令牌可以逐帧机器判定。同时检查审批的 `sessionId` 和工具参数、写出的文件、回合外的更新。
- **范围外的小补测**（共约 13 个 haiku 回合）：hold 对应 #830；杀进程组对应第 7 条的后果；候选版本 0.93.0 用于版本建议。这些都直接回答本任务的问题，成本很小。
- **Q9 探针中间夹一层线路分接**：用来对照"线上有什么"和"SDK 处理器拿到了什么"，也顺带验证了"分接层加 SDK"这个结构可行。
- **拉起适配器**：用一行 Perl 先 setsid 再 exec node，模拟宿主的 `POSIX_SPAWN_SETSID`，没有改 SP-1 的代码。
- **脱敏**：先跑 SP-1 原样的 `sanitize.ts`，再跑第二遍：会话 ID 换成 `session-NN`，从 claude.ai 同步的命令和插件命令换成占位名，进程标签一律换成角色名。

## 验证方式

| 验收标准 | 情况 |
|---|---|
| Q8 有逐轮数据和明确判定 | 通过。数据见上表，逐 prompt 数据在 `recordings/results/q8-claude-*.results.json` |
| Q9 三项各有结论，写明线路分接层的职责 | 三项都支持；分接层的职责和两个 SDK 坑见上文 |
| SP-4b #7 有结论并附进程树（脱敏） | 有，见上表和 notes |
| 版本建议、负载数字是否调整 | 有；负载数字不用改，附一条关于命令列表大帧的建议 |
| 配置文件的前后哈希对比 | `~/.claude/settings.json`：未变。`~/.claude/settings.local.json`：前后都不存在。`~/.claude.json` 顶层的 `mcpServers`、`bypassPermissionsModeAccepted`、`autoModeEnvSetup` 以及名字含 permission、autoMode、trust 的字段：未变。58 个已有项目的 `allowedTools`、`hasTrustDialogAccepted`、`mcpServers`、`enabled/disabledMcpServers`、`enabled/disabledMcpjsonServers`、`hasClaudeMdExternalIncludesApproved`：全部未变。没有新增项目条目 |
| 遗留清单已写入 `.private/`，仓库里没有个人信息 | 已写入。两遍脱敏脚本的自带复扫都是 "scan clean"；另用 Python 对全部入库文件逐一扫描用户名、家目录、`/var/folders`、主机名、账号和组织词、邮箱、本次全部 22 个会话 ID，没有命中 |

- 安全约束：模型只用 haiku；权限模式每次都显式设为 `default`，发 prompt 前会再检查一遍；审批只选过 `allow_once`；剧本固定，不让模型联网；临时目录建在每个用户自己的 `$TMPDIR` 下；没有运行 Codex；外发请求（npm、crates.io、GitHub、nodejs.org）都没有带账号，GitHub 和 crates.io 的 User-Agent 用的是 `cupper-research`。
- 构建：`cargo build`、`cargo clippy --all-targets` 无告警，`cargo fmt` 已格式化，`cargo test` 2/2 通过。
- 额度：约 45 个 haiku 回合，其中约 10 个在开头几秒就被取消。

## 提交列表

```
4317380 docs(task): add T003 working notes and load-rate data
66f81c2 test(spike): add sanitized T003 recordings and results
41526ee chore(spike): add T003 second-pass sanitizer and load-rate extractor
84a10ed chore(spike): add T003 Q9 probe for ACP SDK extension messages
73008a0 chore(spike): add T003 Q8 concurrency harness and config hash check
```

## 遗留问题 / 范围外发现

1. **`/remote-control`（claude-agent-acp 0.93.0 起）**：一条只含 `/remote-control` 的 prompt 会把会话接到 claude.ai/code 或手机 App。这等于在宿主之外多开了一个控制通道，审批和审计都绕过了 cupper。建议交安全工程师评审：升级前宿主要拦截这条命令，并从命令列表里去掉。
2. **SP-1 已入库的录制有脱敏漏洞**：SP-1 客户端在 `stop()` 里记录的 `procsAtExit` 没有被 SP-1 的 `sanitize.ts` 映射，用户 MCP 服务器的命令行原样留在录制里（具体命令行已在存档时由编排者去掉）。另外，SP-1 的录制里仍有原始会话 ID。建议编排者决定是否用本次的第二遍脚本重新处理 SP-1 的录制。我没有改 SP-1 的文件。
3. **计划与任务书的目录名不一致**：计划 `03-plan.md` 关于 T003 的说明写的是代码放在 `spikes/T003-driver-preflight/`，任务书写的是 `spikes/T003-tech-validation/`。我按任务书执行，计划需要改一处。
4. **上游 #1011（未关闭）**：长期运行的适配器在反复 `session/load` 或 resume 同一个会话后，会累积多个孤儿 `claude` 子进程；修复 PR #1220 也还没合并。这和 K6 的"下次发送时自动 resume"加上共享适配器的组合直接相关，建议 T063 和 T066 观察多次 resume 之后的进程数。
5. **SIGKILL 的副作用**：被 SIGKILL 杀掉的 CLI 会留下 `~/.claude/shell-snapshots/` 里的一个快照文件。量很小，已列入遗留清单。
6. **Q8 的覆盖范围有限**：只测了 2 个会话、haiku 的短回合、同一个 cwd；没有测 6 个会话（NFR-11）用真实 CLI 并发，也没有覆盖思考流。建议 T066 加一个 6 会话的场景。
7. **试错**：
   - 第一次 hold 失败：Claude Code 把 `sleep; echo` 归为只读命令，不发审批。改成挂起 Write 的审批后重跑，结果单独记录；三轮主数据不受影响。
   - Q9 探针一开始读错了 `steering.supported` 的位置，后来又有一次只跑了 clippy、用旧二进制跑了两次，前后多花了约 9 个 haiku 回合。入库的是修正并重新构建之后的两次运行，结论与前几次相同。
8. **本机遗留**：一个临时目录（约 1.3 GB，内有未脱敏的原始录制和本地词表，建议删除）；Claude Code 写入的 3 个项目目录（19 个 transcript）、`session-env/` 下 22 个目录、`/private/tmp/claude-<uid>/` 下的 2 个目录和 1 个文件、1 个 shell 快照。没有残留进程。清单在 `.private/cleanup/T003-leftovers.md`，这些内容我都没有删除。