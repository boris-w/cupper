# T102 工作笔记

- 执行者：子 agent（原型实测工程师）
- 日期：2026-10-28
- 代码：`spikes/T102-codex-validation/`；脱敏后的录制和结果：本目录的 `data/`

## 开工前的行数估计（停止线 500，硬停止线 600）

`cargo xtask size` 不计 `docs/`，所以录制和结果放在本目录的 `data/` 下，只有原型代码计入。

| 模块 | 内容 | 估计 |
|---|---|---|
| `q8.ts` | Codex 的拉起方式和安全闸门、Q8 三轮、hold、killgroup、进程树采样（复用 SP-1 的 `acp-client.ts` 和 T003 的 `common.ts`） | 270 |
| `extra.ts` | Q31（声明 AIR 客户端前后的 `phase`、`session/load` 回放）和 Q25（计划确认选 `revise_plan`） | 100 |
| `config-hash.ts` | `~/.codex/config.toml` 按表、`hooks.json` 按顶层键的前后哈希，`[projects]` 新增项单独统计 | 80 |
| `README.md` | 运行和脱敏步骤 | 40 |
| 合计 | | 约 490 |

脱敏直接复用 SP-1 的 `sanitize.ts` 和 T003 的 `sanitize-t003.ts`（两遍），敏感词清单作为 `--terms` 传入，不另写脚本。

## 代码与运行

原型代码在 `spikes/T102-codex-validation/`，产品代码不依赖它。ACP 客户端是 SP-1 的 `acp-client.ts`，配置跟踪和 `ps` 采样来自 T003 的 `common.ts`，都原样复用；为了守住规模，没有单独写 README，运行方法记在这里。

| 文件 | 作用 |
|---|---|
| `codex.ts` | 按宿主的方式拉起 codex-acp（一行 Perl 先 setsid 再 exec node，`CODEX_PATH` 指向原生 codex）；安全闸门；进程树采样，只记角色 |
| `q8.ts` | Q8 三轮和 hold，进程树采样（SP-4b 第 7 条） |
| `extra.ts` | Q31（普通客户端与 AIR 客户端各一个回合，AIR 会话再 `session/load` 回放）、Q25（模式取值，计划确认答 `revise_plan`） |
| `config-hash.ts` | `~/.codex/config.toml`（按表）和 `hooks.json`（按顶层键）的前后哈希，`[projects]` 新增项只给条数和是否属于本次的临时目录 |
| `sanitize-t102.ts` | 脱敏的第 0 遍：用户 MCP 服务器的名字（从录制里的 MCP 工具调用取出，任何拼写）和 AIR 命令列表里的 `skillPath` 换成占位 |

```sh
BASE=$(mktemp -d "${TMPDIR%/}/cupper-t102.XXXXXX"); chmod 700 $BASE
mkdir -p $BASE/adapters/codex-2.2.2 $BASE/private
(cd $BASE/adapters/codex-2.2.2 && npm init -y && npm install --ignore-scripts @agentclientprotocol/codex-acp@2.2.2)
CODEX=<本机 codex 的原生二进制>   # npm 全局安装时在 @openai/codex 包内的 @openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/bin/codex

node config-hash.ts snapshot $BASE/private/config-before.json $BASE
node q8.ts --base $BASE --codex $CODEX --rounds 3 --hold yes --tag run2
node extra.ts --base $BASE --codex $CODEX
node config-hash.ts snapshot $BASE/private/config-after.json $BASE
node config-hash.ts compare $BASE/private/config-before.json $BASE/private/config-after.json
```

脱敏在 T003 的两遍之前加了一遍：结果文件包成一行 JSONL（`{"seq":1,"t":0,"dir":"note","note":"results","data":<结果>}`）放进 `stage0/results/`，录制放进 `stage0/codex/`，先跑 `sanitize-t102.ts $BASE/stage0`，再依次跑 SP-1 的 `sanitize.ts` 和 T003 的 `sanitize-t003.ts`（本机敏感词清单作为 `--terms` 传给后两遍），输出就是本目录的 `data/`。stderr 日志和进程标签（`$BASE/private/*.proc-labels.json`）不入库。

安全约束在代码里的落实：
- 会话的 `cwd` 一律是 `$BASE/repos/` 下新建的一次性 git 仓库；
- 每个会话建好后显式设定模型（`gpt-5.6-luna`）、推理强度（`low`）、权限模式（`read-only`）和协作模式；发 prompt 之前再检查一遍，不满足就抛错，不发送；
- 审批只选 `allow_once`；只有 Q25 的计划确认选 `revise_plan`（kind 为 `reject_once`）；代码里没有选择持久选项的路径；
- 剧本固定，只让模型回显令牌、运行 `sleep`、写一个文件、写一段计划，不让模型联网；
- 传给适配器的环境变量只保留一个小白名单，外加 `CODEX_PATH`；不设 `CODEX_HOME`，不读写 `auth.json`。

## 环境

| 项 | 取值 |
|---|---|
| 机器 | Apple Silicon 的 Mac，macOS 26 |
| Node | v24.19.0（运行适配器和脚本） |
| 适配器 | `@agentclientprotocol/codex-acp` 2.2.2（运行时清单锁定的版本；内含 ACP TS SDK 1.9.0） |
| codex CLI | 本机 npm 全局安装的 codex-cli 0.159.2，经 `CODEX_PATH` 直接给原生二进制（与宿主 ADR-08 的做法相同）；适配器自带的 0.160.1 没有使用 |
| 模型 | `gpt-5.6-luna`，推理强度 `low` |
| 权限模式 | 全部 `read-only`，协作模式 `default`（Q25 的计划会话为 `plan`） |
| 配置 | 真实配置（U-R4）：Codex 走用户的自定义网关，会加载用户自己的 MCP、插件和 skill |
| 同时在运行 | Codex 桌面应用。它也是 `~/.codex/config.toml` 的写者，本次前后哈希一致，没有干扰 |

当天的最新版本（只记录，不升级）：codex-acp `latest` 2.3.0（2026-10-26 发布，依赖 `@openai/codex ^0.164.0`、ACP TS SDK `~2.0.2`），`preview` 2.3.1-preview.8；codex CLI `latest` 0.165.0，`alpha` 0.166.0-alpha.3。0.165.0 已经超出清单的已验证上限 0.163.0。

## 运行清单与额度

| 运行 | 内容 | prompt 数 |
|---|---|---|
| `q8-codex-2.2.2-run1` | 3 轮 + hold；判定条件有误，作废（见试错记录第 1 条） | 20 |
| `q8-codex-2.2.2-run2` | 改正后重跑：3 轮 + hold，Q8 的正式数据 | 20 |
| `extra-codex-2.2.2-plain` | 普通客户端：Q31 一个回合；Q25 计划会话一个回合 | 2 |
| `extra-codex-2.2.2-air` | AIR 客户端：Q31 一个回合，随后关闭会话并 `session/load` 回放 | 1 |

合计 43 个 `gpt-5.6-luna` 低推理强度回合；另有 codex-acp 为每个发过 prompt 的会话生成标题的调用（同一个模型，录制里只看得到结果）。网关这次没有过载，四次运行都没有出现重连（`_meta.codex.error` 为 0）。

## Q8：同一个 codex-acp 进程上两个会话并发

剧本同 T003：一个适配器进程，每轮先并发 `session/new` 两个会话（同一个临时仓库），各自显式设为 `read-only`、`gpt-5.6-luna`、`low`、协作模式 `default`；两个会话同时开始，各自顺序发 3 个 prompt：

1. 只回显一个随机令牌；
2. 用 shell 运行 `sleep 3; echo <令牌>`，再回显输出（read-only 下不需要审批）；
3. 用 apply_patch 新建 `<a|b>-r<轮次>.txt`，内容是令牌，再回复 `done <令牌>`（read-only 下要审批，答 `allow_once`）。

判定逐项检查：6 个 prompt 都返回 `end_turn`；一轮在 5 分钟内结束；没有串话（任何一帧里出现另一个会话的令牌）；审批请求的 `sessionId` 是正在发 prompt 的会话，且请求里是本会话的文件；文件内容正确。另外记录回合外的内容类更新。

**正式数据（run2）**

| 轮 | 两个 `session/new` 并发 | 一轮用时 | 两边同时在途 | `end_turn` | 回显令牌 | 单个 prompt | 审批 | A/B 更新交替 | 串话 | 回合外更新 | 重连 | 文件 | 判定 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 0.13 s | 26.9 s | 25.7 s | 6/6 | 6/6 | 5.0–11.0 s | 2，归属正确 | 12 | 0 | 0 | 0 | A、B 正确 | 通过 |
| 2 | 0.10 s | 37.9 s | 28.6 s | 6/6 | 6/6 | 4.0–23.4 s | 2，归属正确 | 11 | 0 | 0 | 0 | A、B 正确 | 通过 |
| 3 | 0.11 s | 24.9 s | 24.5 s | 6/6 | 6/6 | 3.8–10.6 s | 2，归属正确 | 16 | 0 | 0 | 0 | A、B 正确 | 通过 |

**作废的 run1**（供对照）：三轮的 18 个 prompt 全部 `end_turn`，串话 0，回合外更新 0，重连 0，用时 35.5 s、34.8 s、30.5 s。判定为 FAIL 的原因都在脚本和模型一侧：第 1、2 轮是"审批请求里有本会话的令牌"这条检查写错了（Codex 的文件修改审批只带文件路径，不带内容），审批本身的 `sessionId` 和文件都是对的；第 3 轮两个会话的模型都没有用 apply_patch，而是调用了用户配置的一个 MCP 工具去写文件，被 read-only 沙箱拒绝（EPERM），没有发审批，属于"模型不配合"，按 5.9.5 记为不确定。

**hold**（一个会话的审批挂起时，另一个会话是否照常运行）

| 指标 | run2 | run1 |
|---|---|---|
| A 的文件修改审批挂起时长 | 15.0 s | 15.0 s |
| 期间 B 的 prompt | `end_turn`，4.6 s | `end_turn`，4.8 s |
| 期间 `session/new` | 成功，92 ms | 成功，94 ms |
| 期间 `session/list` | 成功，15 ms | 成功，20 ms |
| 期间 `session/close` | 成功 | 成功 |
| 放行后 A | `end_turn`，文件正确 | `end_turn`，文件正确 |

- 内容类更新在两个会话之间切换了 42 次（run2 全程），说明两个回合确实在同一条 stdout 上交织进行。
- 全部 213 个 `agent_message_chunk` 都没有 `_meta`（run1 是 207 个），见 Q31。
- 逐 prompt 的数据在 `data/results/q8-codex-2.2.2-run2.results.json`。

## SP-4b 遗留第 7 条：Codex 工具命令的会话和进程组

适配器按宿主的方式拉起（pid = pgid = sid）。每 0.5 秒采样一次适配器的整棵进程树，pgid 来自 `ps`，会话 ID 来自 getsid(2)。run2 有 25 个采样，其中 21 个带着正在运行的 `sleep`（共 31 个 `sleep` 进程实例）；run1 有 26 个采样（22 个带着 `sleep`，也是 31 个实例），结论相同。下表的进程号换成了角色：

| 进程 | 父进程 | 进程组 | 会话 | 组长 | 会话首进程 |
|---|---|---|---|---|---|
| adapter（node，setsid 拉起） | 客户端 | 自己的 | 自己的 | 是 | 是 |
| codex app-server | adapter | adapter 的 | adapter 的 | 否 | 否 |
| `zsh -c 'sleep 3; echo …'` | app-server | **自己的（新组）** | **自己的（新会话）** | 是 | 是 |
| sleep | zsh | 跟随 zsh | 跟随 zsh | 否 | 否 |
| 用户配置的 MCP 服务器（node、npm 等） | app-server | **自己的（新组）** | adapter 的 | 是 | 否 |
| MCP 服务器的子进程 | MCP 服务器 | 跟随 MCP 服务器 | adapter 的 | 否 | 否 |
| 桌面应用插件的客户端 | app-server | adapter 的 | adapter 的 | 否 | 否 |
| app-server 拉起的 git | app-server | adapter 的，或自己的（两种都见过） | adapter 的 | — | 否 |

- **工具命令每次都在新会话里**：app-server 直接拉起 `zsh -c`（树里没有 sandbox-exec 之类的中间进程；沙箱如果是 exec 时套上的，进程号不变，从进程树看不出来），zsh 是新会话的首进程、新进程组的组长，31 个实例无一例外。和 Claude 一样，"同一会话""同一进程组"两个信号对 Codex 的工具命令永远命中不了，只剩祖先链（zsh 的父进程就是 app-server）。
- run1 有一次采样里 zsh 的会话 ID 读成了 -1：进程在 `ps` 和 getsid 之间退出了，`ps` 的状态仍是 `Ss`（会话首进程），不影响结论。
- **MCP 服务器自成进程组但留在适配器的会话里**（相当于 setpgid，不是 setsid），所以"同一会话"信号能识别它们，"同一进程组"不能。按进程组清理（5.6.8）对它们同样无效。
- **会话关闭后 MCP 进程不释放**：run2 每轮开始时（两个新会话刚建好、还没发 prompt）树里有 13、32、49 个进程，每轮多出约 18 个；三轮之后适配器关闭 stdin 时有 78 个进程（run1）。SP-1 已经观察到"Codex 关闭会话后不释放 MCP"，这次量化了。
- 没有做杀进程组的补测（任务书没有要求）。从上表可以直接推出：对适配器的进程组发 SIGKILL 只能杀掉 adapter、app-server 和留在组里的少数子进程，正在运行的工具命令和 MCP 服务器都不在这个组里。

## Q31：`agentMessage.phase` 是否经 `_meta` 透出

源码（codex-acp 2.2.2 `dist/index.js`）：
- `rememberAgentMessagePhase` 在 app-server 的 `item/started`、`item/completed`（`agentMessage`）里记下 `item.phase`；`createTextEvent` 把它交给 `createMessagePhaseMeta(phase, airClient)`；
- `createMessagePhaseMeta` 只在 `airClient` 为真时返回 `{jetbrains: {air: {version: 1, phase}}}`（`airOnlyMeta`）；
- `airClient` 由 `isAirClient(clientCapabilities)` 决定：`clientCapabilities._meta.jetbrains.air` 是一个对象就算；
- 计划模式下的计划文本（`planMessageChunk`）和 `session/load` 回放里的 agentMessage 走同一个函数，计划文本固定为 `final_answer`。

实测（`data/results/extra-codex-2.2.2.results.json`）：

| 客户端 | 回合内的消息 | `phase` | 回放（`session/load`） |
|---|---|---|---|
| 普通客户端（cupper 现在的声明） | 2 条消息（14、7 个 chunk） | **没有 `_meta`** | 没测 |
| 声明 `_meta.jetbrains.air: {version: 1, capabilities: []}` | 2 条消息（10、7 个 chunk） | 第 1 条 `commentary`，第 2 条 `final_answer`，每个 chunk 都带 | 同样两条消息，各 1 个 chunk，`phase` 与回合内一致 |

字段位置：`session/update` → `params.update`（`sessionUpdate: "agent_message_chunk"`）→ `_meta.jetbrains.air.phase`，同一层还有 `version: 1`；消息由 `params.update.messageId`（app-server 的 item id）区分。

声明 AIR 之后，codex-acp 还会改变别的输出：模式选项带上 `_meta.jetbrains.air.kind`（见 Q25），命令列表的每一项带上 `skillPath`（用户 skill 和插件的本机路径，入库前已替换成占位），工具调用的渲染换成 AIR 的那一套（源码 `AcpToolCallRenderer.render`：不再补 `applyStandardFields`）。这次唯一的 `exec_command` 工具调用在两种客户端下逐字段相同（因为客户端声明了 `terminal_output`），但没有覆盖文件修改、MCP、搜索等。

## Q25（初查）：Codex 权限模式的取值和档位

| `value` | `name` | AIR `kind` | 审批策略 / 审批者 / 沙箱（源码 `AgentMode`） | 5.6.5 档位表 |
|---|---|---|---|---|
| `read-only` | Read-only | `standard` | on-request / user / read-only，无网络 | `normal` |
| `workspace-write` | Workspace access | `standard` | on-request / user / workspace-write，无网络 | `normal` |
| `agent` | Auto review | `auto_review` | on-request / auto_review / workspace-write，无网络 | `elevated` |
| `agent-full-access` | Full access | `full_access` | never / user / danger-full-access | `full_access` |

- 两种客户端拿到的取值和顺序相同，只是普通客户端没有 `kind`。和 5.6.5 档位表一致，没有新的取值。
- 新会话的默认模式是 `agent`（`DEFAULT_AGENT_MODE`），除非设了环境变量 `INITIAL_AGENT_MODE`（已在禁用清单里）。模式只改会话状态，不写用户配置。
- **`revise_plan` 结束回合**：计划会话（`read-only` + 协作模式 `plan`）的回合完成后，适配器在同一个 `session/prompt` 里发审批请求"Implement this plan?"，`toolCall.kind` 为 `switch_mode`，选项是 `implement_plan`（allow_once）和 `revise_plan`（reject_once）。答 `revise_plan` 后 1 ms prompt 返回 `end_turn`，之后 5 秒内这个会话没有任何更新，协作模式仍是 `plan`。源码一致：只有选 `implement_plan` 才会切到 `default` 并在同一个 prompt 里接着跑"Implement the approved plan."；其他回答（包括取消和出错）都直接结束。

## 配置文件的前后对比

`config-hash.ts`，第一次拉起适配器之前（11:42）和全部运行结束、确认没有残留进程之后（12:00）各取一次：

| 文件 | 结果 |
|---|---|
| `~/.codex/config.toml` | 未变（文件整体哈希一致，所以没有逐表列出） |
| `~/.codex/hooks.json` | 未变 |
| 新增的 `[projects]` 信任项 | 0 条 |

codex-acp 把会话目录的信任（`projects.<cwd>.trust_level = "trusted"`）放在每个线程的配置覆盖里传给 app-server（源码 `createSessionConfig`），不写进用户的 `config.toml`，与实测一致。

## 试错记录

1. **run1 的判定写错了**：照搬 T003 的"审批请求里带着本会话的令牌"，但 Codex 的文件修改审批（`kind: edit`，"Edit files"）只有 `locations` 里的文件路径，没有内容，于是两轮"审批归属"都判成了不对。改成"带着本会话的令牌，或者写的是本会话的文件"（文件名每个会话每轮都不同）。
2. **run1 第 3 轮模型不配合**：两个会话都选了用户配置的一个 MCP 工具去写文件，被沙箱拒绝。prompt 改成"只用 apply_patch"后重跑，三轮都用了 apply_patch。两处修改后重跑一次（Q8 的第 1 次重跑），没有再重跑。
3. **等待脚本自己匹配了自己**：一开始用 `pgrep -f "q8.ts --base"` 等运行结束，结果匹配到了等待循环自己的命令行，一直等到超时；运行本身不受影响，改成等输出文件里的退出标记。
4. **`config-hash.ts` 的末尾换行**：自测时发现文件末尾的空行算在最后一张表里，在文件末尾追加新表会让原来最后一张表误报"变化"。去掉末尾换行后再切分，用合成的 TOML 验证了表、`mcp_servers.*`、`projects.*` 和 hooks 顶层键的增删改都能正确报出。正式的前后快照文件整体哈希一致，不受影响。
5. **脱敏**：SP-1 和 T003 的两遍脚本漏了两处 Codex 特有的内容。一是 AIR 录制里命令列表每一项的 `_meta.jetbrains.air.skillPath`，会带出用户装了哪些插件和 skill（脚本只换名字和描述，保留了 `_meta`）；二是用户配置的 MCP 服务器名，Codex 的工具调用写成标题 `mcp.<服务器>.<工具>` 和 `rawInput.server`，模型的思考文本和 `_meta` 键名（驼峰形式）里也有，而脚本只认 `mcp__<服务器>__<工具>` 这种 Claude 的写法。一开始用 sed 只处理了 `skillPath`；编排者指出 MCP 名也要在入库前换掉，于是写了 `sanitize-t102.ts`，从录制里的 MCP 工具调用取出服务器名，按蛇形、短横线、驼峰、空格分隔或连写、任意大小写的写法全文替换，`skillPath` 也收进来。脱敏后按同样的写法全文搜索取出的名字，`data/`、代码和本笔记里都是 0 处。
6. **规模**：第一版代码写完是 688 行，越过了硬停止线。去掉没有要求的杀进程组补测、把运行说明从 README 挪到这里、合并重复代码之后是 499 行。补上 `sanitize-t102.ts` 后预计越过停止线 500，先报给编排者，编排者同意把停止线改为 540（硬停止线 600 不变），最终 530 行。
