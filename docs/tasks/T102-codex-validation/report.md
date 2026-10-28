# T102 汇报

- 执行者：子 agent（原型实测工程师）
- 分支：`spike/T102-codex-validation`
- 完成日期：2026-10-28

## 做了什么

### K6 结论（先看这里）

| 项 | 结论 | 对后续任务的含义 |
|---|---|---|
| **Q8（Codex）** | **通过。** 适配器是锁定的 codex-acp 2.2.2，CLI 是本机 codex-cli 0.159.2。3 轮全部通过：18 个 prompt 都返回 `end_turn`，没有串话，审批都发给了正确的会话，文件都正确。一个会话的审批挂起 15 秒期间，另一个会话的 prompt、`session/new`、`session/list`、`session/close` 都照常完成 | **K6 通过**：T111 按原设计写，**不派 T119**。ADR-09"每种 agent 共用一个适配器进程"的前提对 Codex 成立 |
| **SP-4b 遗留第 7 条（Codex）** | **Codex 拉起工具命令时也会 setsid。** app-server 直接拉起 `zsh -c`，zsh 是新会话的首进程，也是新进程组的组长，31 个实例没有例外。app-server 本身留在适配器的会话和进程组里。用户配置的 MCP 服务器各自成组，但仍在适配器的会话里 | 5.8.4：和 Claude 一样，对 Codex 的工具命令"同一会话""同一进程组"两个信号永远命中不了，只剩祖先链（zsh 的父进程就是 app-server）。K21：按适配器的进程组清理，杀不到正在运行的命令，也杀不到 MCP 服务器 |
| **Q31** | **只对声明了 JetBrains AIR 扩展的客户端透出。** 字段位置：`session/update` → `params.update`（`agent_message_chunk`）→ `_meta.jetbrains.air.phase`。取值有 `commentary` 和 `final_answer`，每个 chunk 都带，`session/load` 的回放里也有。触发条件是 `clientCapabilities._meta.jetbrains.air` 为一个对象。cupper 现在的声明拿不到这个字段：Q8 两次运行共 420 个 `agent_message_chunk`，都没有 `_meta` | T110 照现在的声明，按宿主规则（5.3.5）判定最终回答。要不要为了 `phase` 声明 AIR，留给 T110 评估，副作用见下文 |
| **Q25（初查）** | 模式取值 4 个：`read-only`、`workspace-write`、`agent`、`agent-full-access`，与 5.6.5 的档位表一致，没有新取值。AIR 客户端还会拿到 `kind`（standard、standard、auto_review、full_access），和档位一一对应。**`revise_plan` 会结束回合**：选它之后 1 ms prompt 就返回 `end_turn`，之后没有更新，协作模式保持 `plan` | 对照表和档位表不用改。按任务书，这里只记录，改表由 T110 做 |
| **配置文件** | `~/.codex/config.toml`、`~/.codex/hooks.json` 前后哈希一致，新增的 `[projects]` 信任项 0 条 | 编排者已核对与备份一致，不用恢复 |

### Q8 逐轮数据

剧本同 T003。一个适配器进程，每轮先并发新建两个会话，共用同一个临时仓库，都显式设为 `read-only`、`gpt-5.6-luna`、推理强度 `low`、协作模式 `default`。两个会话同时开始，各自依次发 3 个 prompt：
1. 回显令牌；
2. 用 shell 运行 `sleep 3; echo <令牌>`；
3. 用 apply_patch 新建文件，内容是令牌。read-only 下要审批，答 `allow_once`。

任何一帧里出现另一个会话的令牌，就算串话。

正式数据（重跑的 run2）：

| 轮 | 一轮用时 | 两边同时在途 | `end_turn` | 回显令牌 | 单个 prompt | 审批归属正确 | A/B 更新交替 | 串话 | 回合外的内容更新 | 网关重连 | 文件 | 判定 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 26.9 s | 25.7 s | 6/6 | 6/6 | 5.0–11.0 s | 2/2 | 12 次 | 0 | 0 | 0 | 正确 | 通过 |
| 2 | 37.9 s | 28.6 s | 6/6 | 6/6 | 4.0–23.4 s | 2/2 | 11 次 | 0 | 0 | 0 | 正确 | 通过 |
| 3 | 24.9 s | 24.5 s | 6/6 | 6/6 | 3.8–10.6 s | 2/2 | 16 次 | 0 | 0 | 0 | 正确 | 通过 |

- 每轮两个 `session/new` 并发，耗时 0.10–0.13 s。
- **hold**：会话 A 的文件修改审批挂起 15.0 s。期间会话 B 的 prompt 在 4.6 s 内返回 `end_turn`，`session/new` 用了 92 ms，`session/list` 用了 15 ms，`session/close` 正常。A 放行后返回 `end_turn`，文件正确。

**第一次运行（run1）已作废，记录如下：**
- 18 个 prompt 全部 `end_turn`，串话 0，回合外更新 0，但三轮都判成了 FAIL。
- 第 1、2 轮是我的判定写错了。我照搬了 T003 的"审批请求里带着本会话的令牌"，但 Codex 的文件修改审批（`kind: edit`）只带文件路径，不带文件内容。审批的 `sessionId` 和文件其实都是对的。
- 第 3 轮是模型不配合。两个会话都调用了用户配置的一个 MCP 工具去写文件，被 read-only 沙箱拒绝（EPERM），所以没有发审批。按 5.9.5，这属于"不确定"。
- 改法有两处：审批归属改成"带着本会话的令牌，或者写的是本会话的文件"（文件名每个会话每轮都不同）；第 3 个 prompt 明确要求只用 apply_patch。改完重跑一次，就是 run2。Q8 只重跑了这一次。
- run1 的 hold 也正常：B 4.8 s 返回，`session/new` 94 ms，`session/list` 20 ms。
- run1 的录制和结果也一并入库。

### SP-4b 遗留第 7 条：进程树（脱敏后，进程号换成了角色）

- 适配器按宿主的方式拉起：一行 Perl 先 setsid 再 exec node，pid = pgid = sid。
- 每 0.5 秒采样一次。pgid 取自 `ps`，会话 ID 取自 getsid(2)。
- 两次运行共 51 个采样，其中 43 个采到了正在运行的 `sleep`。

| 进程 | 父进程 | 进程组 | 会话 |
|---|---|---|---|
| adapter（node） | 客户端 | 自己的（组长） | 自己的（首进程） |
| codex app-server | adapter | adapter 的 | adapter 的 |
| `zsh -c 'sleep 3; echo …'` | app-server | **新组（组长）** | **新会话（首进程）** |
| sleep | zsh | 跟随 zsh | 跟随 zsh |
| 用户配置的 MCP 服务器 | app-server | **自己的（组长）** | adapter 的 |
| MCP 服务器的子进程 | MCP 服务器 | 跟随 MCP 服务器 | adapter 的 |
| 桌面应用插件的客户端 | app-server | adapter 的 | adapter 的 |
| app-server 拉起的 git | app-server | adapter 的，或自己的（两种都见过） | adapter 的 |

- 进程树里没有 sandbox-exec 之类的中间进程。
- run1 有一次采样把 zsh 的会话 ID 读成了 -1：进程在 `ps` 和 getsid 之间退出了，而 `ps` 显示的状态仍是 `Ss`，不影响结论。
- 杀进程组的补测没有做：任务书没有要求，也是为了守住规模。从上表可以直接推出：对适配器的进程组发信号，只会结束 adapter、app-server 和留在组里的少数子进程；正在运行的工具命令和 MCP 服务器都不在这个组里。

建议：
- **5.8.4**：把 v1.6 里关于 Claude 的那段结论扩展到 Codex。两边的工具命令都在新会话里，血缘检查只剩祖先链一个信号。Codex 的 MCP 服务器虽然不在适配器的进程组里，但在适配器的会话里，"同一会话"信号仍能识别它们。T038 按这个结论设定各信号的期望。
- **5.6.8 和 K21**：关闭 Codex 适配器时，按进程组清理同样不够。T003 的两条建议对 Codex 也适用：关闭前枚举适配器的全部后代，按各自的进程组逐个发信号；或者先让 CLI 自己正常退出。T113 的兼容剧本可以加一个 Codex 版的"关闭时有命令正在运行"。

### Q31：`agentMessage.phase`

源码（codex-acp 2.2.2）：
- 适配器在 app-server 的 `item/started`、`item/completed` 里记下 agentMessage 的 `phase`；
- 生成 `agent_message_chunk` 时经 `createMessagePhaseMeta` 放进 `_meta`，但这个函数只对 AIR 客户端返回值（`airOnlyMeta`）；
- 计划模式下的计划文本固定标为 `final_answer`；
- `session/load` 的回放走同一个函数。

实测：

| 客户端 | 回合内 | 回放 |
|---|---|---|
| 普通客户端（cupper 现在的声明） | 2 条消息，chunk 都没有 `_meta` | — |
| 声明 `_meta.jetbrains.air: {version: 1, capabilities: []}` | 第 1 条消息（工具调用之前）的每个 chunk 带 `phase: "commentary"`，第 2 条（最终回答）带 `"final_answer"`，同一层还有 `version: 1` | 两条消息，`phase` 与回合内一致 |

声明 AIR 的副作用（供 T110 评估）：
- 模式选项会带上 `kind`；
- 命令列表的每一项会带上 `skillPath`，内容是用户 skill 和插件的本机路径；
- 工具调用改用 AIR 的渲染方式（源码里不再补 `applyStandardFields`）。这次唯一的 `exec_command` 在两种客户端下逐字段相同，但没有覆盖文件修改、MCP、搜索等工具。

### Q25（初查）

| `value` | `name` | AIR `kind` | 审批策略 / 审批者 / 沙箱（源码） | 5.6.5 档位 |
|---|---|---|---|---|
| `read-only` | Read-only | standard | on-request / user / read-only，无网络 | normal |
| `workspace-write` | Workspace access | standard | on-request / user / workspace-write，无网络 | normal |
| `agent` | Auto review | auto_review | on-request / auto_review / workspace-write，无网络 | elevated |
| `agent-full-access` | Full access | full_access | never / user / danger-full-access | full_access |

- 新会话的默认模式是 `agent`，除非设了 `INITIAL_AGENT_MODE`（这个变量已在禁用清单里）。
- 切换模式只改会话状态，不写用户配置。
- 计划确认的流程：计划回合完成后，适配器在同一个 `session/prompt` 里发审批"Implement this plan?"（`toolCall.kind: switch_mode`），选项是 `implement_plan`（allow_once）和 `revise_plan`（reject_once）。
  - 只有选 `implement_plan`，才会切回 `default`，并在同一个 prompt 里接着跑实现回合；
  - 选 `revise_plan`、取消或出错，都直接结束回合。实测与源码一致。

### 版本（2026-10-28 查询，只记录，不升级）

| 组件 | 锁定或本机 | 当天最新 | 说明 |
|---|---|---|---|
| codex-acp | 2.2.2 | `latest` 2.3.0（10-26），`preview` 2.3.1-preview.8 | 2.3.0 依赖 `@openai/codex ^0.164.0` 和 ACP TS SDK `~2.0.2`（2.2.2 用的是 1.9.0）。升级前要重跑 Q8 和 Q31 |
| codex CLI | 本机 0.159.2；清单的已验证范围是 0.150.1–0.163.0 | `latest` 0.165.0，`alpha` 0.166.0-alpha.3 | 0.165.0 已经超出已验证上限，由 T113 扩展 |

### 产出

- `spikes/T102-codex-validation/`：
  - `codex.ts`：拉起方式、安全闸门、进程树采样；
  - `q8.ts`：Q8 和 hold；
  - `extra.ts`：Q31 和 Q25；
  - `config-hash.ts`：配置文件的前后哈希；
  - `sanitize-t102.ts`：第 0 遍脱敏；
  - ACP 客户端用的是 SP-1 的 `acp-client.ts`，配置跟踪和 `ps` 采样用的是 T003 的 `common.ts`，都原样复用。
- `docs/tasks/T102-codex-validation/notes.md`：环境、运行方法、逐项数据、试错记录。
- `docs/tasks/T102-codex-validation/data/`：脱敏后的 4 个录制和 3 个结果文件，约 1.8 MB。
- 遗留清单：主仓库的 `.private/cleanup/T102-leftovers.md`，不入库。

## 关键取舍

- **全部用 `read-only` 模式**：比 `workspace-write` 更严格，也正好让第 3 个 prompt 的文件修改需要审批，能测审批归属和 hold。`sleep`、`echo` 在 read-only 沙箱里不需要审批。
- **`CODEX_PATH` 指向本机 npm 安装里的原生二进制**：和宿主 ADR-08 的做法一致，进程树里也就没有 SP-1 看到的那层 node 包装进程。适配器自带的 0.160.1 没有用。
- **Q31 多开了一个 AIR 客户端**：先从源码确认 `phase` 只发给 AIR 客户端，再用两种声明各跑一个回合，并把 AIR 会话回放一次。
- **Q25 只跑了一个计划回合**：`revise_plan` 的 kind 是 `reject_once`，属于不持久的选项。
- **Q8 重跑了一次**：run1 的 FAIL 一半是我的判定写错，一半是模型不配合。修好之后重跑，run1 原样保留。
- **没有做杀进程组的补测**：理由同上。
- **规模**：
  - 第一版代码写完是 688 行，越过了硬停止线。去掉补测、把运行说明挪进 notes、合并重复代码之后降到 499 行。
  - 之后编排者要求在入库前把 MCP 服务器名换掉，加了 `sanitize-t102.ts`。我预计会越过停止线，先报给了编排者，编排者把停止线改为 540。
  - 最终 +530 行。录制放在 `docs/` 下，不计入规模。

## 验证方式

| 验收标准 | 情况 |
|---|---|
| Q8（Codex）有逐轮数据和明确判定 | 通过。逐轮数据见上表；逐 prompt 的数据在 `data/results/q8-codex-2.2.2-run2.results.json`。作废的 run1 也已入库，原因见上文 |
| SP-4b #7（Codex）有结论，并附脱敏的进程树 | 有，见上表。入库的采样只记角色；进程标签只留在本机的私有文件里 |
| Q31 有结论并附字段位置；Q25 列出观察到的取值 | 有，见上文 |
| 配置文件前后哈希对比，新增 `[projects]` 信任项单独列出 | `config.toml` 未变；`hooks.json` 未变；新增 `[projects]` 信任项 0 条。快照分别在第一次拉起适配器之前和全部运行结束之后各取一次。`config-hash.ts` 的逐表比较用合成的 TOML 自测过 |
| 遗留清单写进 `.private/cleanup/`；仓库里没有个人信息和网关域名 | 遗留清单已写入。用户的敏感词清单（`tools/sanitize words`）扫过 dossier 9 个文件、spike 5 个文件；通用扫描（`scan`）扫过 14 个文件，都 clean。另外全文搜过以下内容，都是 0 处：MCP 服务器名的各种写法、原始线程 ID、家目录、用户名、主机名、临时目录、邮箱、域名。网关域名没有出现在录制里；stderr 日志是空的，也不入库 |
| PR 上 GitHub CI 的 8 个检查都通过 | **通过。** PR #13：https://github.com/boris-w/cupper/pull/13 。提交 4ee2222 上的运行：https://github.com/boris-w/cupper/actions/runs/40631392813 ，8/8 pass（compat-framework、e2e、license、protocol、rust、testdata-scan、ts、ui）。上一轮运行（https://github.com/boris-w/cupper/actions/runs/40630903896 ）的 testdata-scan 失败，原因是 main 上的 cae6312 没有登记 `IT-runtime-first-prompt-waits`。编排者在 cc9df93 修复，并把 main 合进了分支 |

- **安全约束**：
  - 模型只用 `gpt-5.6-luna`，推理强度 `low`；
  - 每个会话都显式设定 `read-only` 和协作模式，发 prompt 前再检查一遍；
  - 审批只选过 `allow_once`，计划确认选过一次 `revise_plan`；
  - 剧本固定，不让模型联网；
  - 临时目录建在本用户的 `$TMPDIR` 下，权限 0700；
  - 没有设 `CODEX_HOME`，没有读写 `auth.json`；`~/.codex` 下只为算哈希读了两个配置文件；
  - 没有改任何用户配置；
  - 外发请求只有 npm（安装和查版本），没有带个人信息。
- **类型检查**：对 spike 的 5 个文件跑了 `tsc --noEmit --strict`，无错误。
- **额度**：43 个 luna 低推理强度回合（run1 20 个，run2 20 个，Q31 和 Q25 共 3 个）。另外 codex-acp 会为每个发过 prompt 的会话调用模型生成标题。网关这次没有过载，没有重连。

## 提交列表

```
4ee2222 Merge branch 'main' into spike/T102-codex-validation
42f91dd docs(task): add T102 working notes
1e011dc test(spike): add sanitized T102 recordings and results
5b59e12 chore(spike): add T102 pass-0 sanitizer for Codex recordings
ecf5d6b chore(spike): add T102 Codex harness and config hash check
```

（4ee2222 是编排者把 main 合进分支时生成的合并提交。）

## 遗留问题 / 范围外发现

1. **`tools/sanitize` 有同样的两处漏洞**（我没有改它）。它和 SP-1 的脚本规则相同：
   - MCP 服务器名只认 Claude 的写法 `mcp__<服务器>__<工具>`，不认 Codex 的工具标题 `mcp.<服务器>.<工具>`、`rawInput.server`，也不认 `_meta` 键名里的驼峰写法（例如 `codex/<服务器>ExecutionDurationMs`）；
   - `redactCommands` 只换命令的名字和描述，保留了 `_meta`。客户端声明 AIR 时，`skillPath` 会带出本机路径和插件名。

   T066 也补过一处脱敏漏洞。建议由 T110 或 T113 在 `tools/sanitize` 里集中补上，并加测试。SP-1、T003 已入库的 Codex 录制里如果有 MCP 工具调用，也建议复查。
2. **Codex 关闭会话后不释放 MCP 进程**：run2 每轮开始时，适配器的进程树里分别有 13、32、49 个进程，每轮多出约 18 个；run1 三轮之后关闭时有 78 个。这和 ADR-09 的 Codex 回收阈值、NFR-11 的 6 会话估算直接相关，建议 T111 或 T113 测一下长时间运行下的进程数和内存。
3. **在真实配置下，模型可以调用用户配置的 MCP 工具**：run1 第 3 轮，模型绕开了 apply_patch，改用 MCP 工具写文件，被沙箱拒绝。T113 的 prompt 要点名使用哪个工具。是否所有 MCP 调用都受沙箱约束，建议交安全工程师确认。
4. **`session/list` 没有结果时仍返回 `nextCursor`**，值是用户其他线程的 `updatedAt`。这个时间戳进了录制，影响很小，我没有处理。
5. **SP-1 客户端的最后一条注记会丢**：`stop()` 写的 `orphans-after-exit` 还没写进文件，脚本就调用了 `process.exit`。不影响结论：残留进程我另外核对过，没有残留。
6. **`config-hash.ts` 按行切分 TOML**：多行数组里单独一行的 `["a", "b"]` 会被误认成表头。这只影响逐表的差异列表，不影响"文件是否变化"的判断。T113 如果复用，最好换成真正的 TOML 解析。
7. **新版本已发布**：codex-acp 2.3.0（换用 ACP TS SDK 2.0）；codex CLI 0.165.0，超出了已验证上限。建议在 T113 的兼容运行里评估。
8. **本机遗留**（清单在 `.private/cleanup/T102-leftovers.md`，这些内容我都没有删除）：
   - 临时目录约 346 MB，里面有未脱敏的原始录制、进程标签、配置哈希快照；
   - Codex 写下的 21 个线程，其中 19 个发过 prompt，也会出现在 Codex 桌面应用的历史里；
   - 没有残留进程。
