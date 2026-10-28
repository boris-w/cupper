# T066 工作笔记

- 执行者：子 agent（测试工程师）

## 开工前读过的

- 任务书；设计 5.9.3、5.9.5、5.10.3、5.8.7，5.13 的 Q8、Q10、Q28、Q34、Q42、Q43；计划 T066 一行和注、K12。
- `handoff-notes.md` 第一列含 T066 的 12 行（T004、T054、T053、T063、T064、T187、T188、T192、T194、T196、T003）。
- 卷宗：T064、T065、T187、T188、T192、T194 的汇报和笔记，T003 的汇报、笔记和遗留清单的写法。
- 代码：`crates/xtask/src/compat/`（编排、配置对比、报告、入库、已验证范围），`crates/cupper-host/tests/compat.rs` 和 `tests/compat/scenarios.rs`（执行者、21 个剧本），`tools/sanitize`。

## 开工时的状态

- `cargo xtask compat --real --dry-run`：21 个剧本的计划都列出来了，约束检查都是 nothing，steer 记为不适用；要对比的配置是 `~/.claude/settings.json`、`~/.claude/settings.local.json`、`~/.codex/config.toml` 和 `~/.claude.json` 里和权限有关的字段。
- 本机的 Claude CLI 是 2.1.296，正好是已验证范围的上限（ADR-08 的 2.1.200–2.1.296）。用户配置不改，所以不升级 CLI；这次的报告计入范围后，范围不会变宽。
- 主仓库的 `.private/` 里还没有 `sensitive-words.txt`。

## 一轮全套的回合数（估计）

| 剧本 | 回合 |
|---|---|
| 基本组 9 个（new-session … cancel-approval） | 各 1，共 9 |
| thinking（sonnet） | 1 |
| crash-resume | 3 |
| host-restart、image | 各 2 |
| concurrent-two（2 个会话 × 3 个 prompt × 3 轮，加两个会话的首条） | 20 |
| six-sessions（6 个会话的首条，加每个会话 1 个） | 12 |
| close-running、untrusted-project、hooks-reload、config-scope | 各 1 |
| resume-procs（首条加 5 次 resume） | 6 |
| steer | 0（不适用） |

合计约 58 个 haiku 回合、1 个 sonnet 回合。一个回合里可能有几次模型调用（工具调用）。

## 规模估计（只算手写的代码和测试）

现在还不知道真跑会碰到哪些框架问题，先按已经看到的估：

| 模块 | 内容 | 估计 |
|---|---|---|
| 入库的报告不带 CLI 会话 ID | `cleanup` 里的会话 ID 只留在本机的报告里，入库的报告只写个数（任务书：入库内容不出现会话 ID） | 15 |
| 真跑暴露的框架小问题 | 判定规则、等待上限、剧本步骤，每处在汇报里说明 | 60 |
| 预留 | | 50 |
| 合计 | | 约 125，停止线 250 |

## 运行记录

每次运行都只跑一份套件。"回合"按 CLI 实际计费的回合算（录制里带 `cost` 的 `usage_update`，加上没有结束就收尾的 close-running；并发剧本另一些会话的回合不在导出的录制里，按 prompt 数补上）。

| 运行 | 剧本 | 结果 | 回合 | 约合 |
|---|---|---|---|---|
| 冒烟 1 | new-session | 安装运行时期间发的 prompt 37 ms 就以 `runtime_failed` 结束（宿主缺陷，见下），等能力等了一阵，手动结束。CLI 没有被调用 | 0 | 0 |
| 冒烟 2 | new-session | 等运行时装好之后再建会话：结构检查都过，lsof 检查误报 | 1 | 0.003 |
| 冒烟 3 | new-session | 加临时诊断（不入库）找 lsof 误报的原因 | 1 | 0.003 |
| 冒烟 4 | new-session | 修好 lsof 之后通过 | 1 | 0.003 |
| 冒烟 5 | normal-turn、approval、cancel | normal-turn 通过；approval 等不到审批（`echo` 不需要审批），两次尝试后手动结束 | 3 | 0.014 |
| 冒烟 6 | normal-turn、approval、cancel | 改成会写文件的命令后，三个都通过 | 3 | 0.012 |
| 冒烟 7 | 其余 14 个有风险的剧本 | 10 个通过、2 个观察；plan、thinking 失败 | 23 | 0.22 |
| 全套 1（`--write`） | 21 个 | 19 个通过、2 个观察、thinking 失败、steer 不适用；`--write` 在报告的敏感词扫描处因路径 bug 中止 | 约 59 | 约 0.47 |
| 全套 2（`--write`，整套重跑第 1 次） | 21 个 | 与全套 1 相同；录制、金样例、报告都写入 | 约 59 | 约 0.47 |

合计约 150 个回合（haiku 约 145；sonnet 约 5：thinking 每轮 1 个，plan 在 plan 模式下被 CLI 换成 sonnet），约 1.2 美元。没有一个剧本重试过；整套重跑 1 次（上限 2 次）。

## 真跑暴露的框架问题和处理

| 问题 | 现象 | 处理 |
|---|---|---|
| 运行时还在安装就建会话 | 宿主启动后约 9 s 才装好运行时，执行者 0.2 s 就发 `session/create`，回合直接 `runtime_failed` | 执行者在建会话前等索引流的 `adapter.runtime` 变成 `ready`（上限 5 分钟，`failed` 记失败）。宿主不等安装器是产品缺陷，另开任务 |
| 只读命令不发审批 | Claude Code 直接运行 `echo`、`ls`、`sleep`，要回答请求的剧本白等 5 分钟 | 这些剧本改成运行会写文件的命令（`touch …`，cancel 和 close-running 是 `touch started.txt && sleep …`），检查不变 |
| 用户默认的 effort | 用户配置里默认 effort 是 xhigh | 真实运行对 haiku 的剧本传 `reasoning_effort: low`；thinking 用自己的模型，不传 effort |
| lsof 检查误报 | 宿主绑在路径上的 `host.sock` 和 claude 自己监听的 `/tmp/cc-socks/<pid>.sock` 都没有对端，两边的空字符串一比就"匹配" | 空地址不参与比对。用例覆盖：双方都只有绑在路径上的套接字时不算持有；持有宿主套接字的对端、宿主监听套接字本身（同一地址、没有对端）、连到宿主的管道仍算持有。变异检查：去掉"非空"、或者只比对端，各有一种情况会失败 |
| 报告里有 CLI 会话 ID | `cleanup` 逐条写了会话 ID，会随报告入库 | 报告只写个数，会话 ID 写到运行输出里的 `cli-sessions.txt` |
| 用户 skill 的提示语 | 用户 skill 的名字和描述被换掉了，`input.hint` 没有 | `tools/sanitize` 一并换成 `<redacted>`，加了用例，变异检查过 |
| GOLD-B 按文件名找录制 | 真实录制和假 agent 录制都叫 normal-turn 等 | 按录制路径区分；回放时 `session/create` 的 model、effort 从录制里宿主发出的 `set_config_option` 推出 |
| plan 剧本的期望 | 真实 CLI 的计划正文在 ExitPlanMode 请求里，没有 `plan` 更新；拒绝计划后回合以 `cancelled` 结束 | 检查换成"plan_confirm 请求带着非空的计划正文"、回合 `cancelled`；假 agent 的 Claude 人设拒绝计划后也结束回合（编排者裁定） |
| `--write` 的门槛 | 任何剧本失败都不写，thinking 的产品问题会挡住 GOLD-B | 真实运行只要配置没变就写报告，失败照实写进去；录制只取通过和观察到的剧本；敏感词扫描不变（编排者裁定） |
| `--write` 写 A 层金样例 | 设计 5.9.3 的 A 层用全部录制 | 真实的 GOLD-B 录制同时标 `gold-a`；`--write` 写两层金样例，写完再不带 `UPDATE_GOLDEN` 检查一遍（"录制和金样例一一对应"的用例在写的时候会抢跑，放在检查那一步） |
| 报告的敏感词扫描 | `tools/sanitize words` 把绝对路径拼在工作目录后面，找不到报告 | 传相对于工作区的路径 |
| Q28、Q34 没有阳性对照 | "false"分不清是没加载还是 hook 根本不运行 | hooks-reload 事先放一个 UserPromptSubmit hook；config-scope 在项目目录自己放一个 hook。只加观察项，判定不变 |
| Q43 的数字 | 检查只说"没有增加" | 执行者把每轮的进程数打印到日志 |

## 产品发现（停下汇报过，由编排者另开任务）

1. **安装运行时期间发的 prompt 直接失败。** `driver/runtime/launch.rs` 的 `start` 遇到 `Program::Runtime` 就核对 node 和打包文件的哈希，文件还没装好就返回 `RuntimeFailed`；`supervisor/mod.rs` 在后台起安装器（`Installer::run`），启动适配器之前不等它。设计 5.2.12 写的是"等待安装完成"。
2. **Claude 的思考流是空的。** 适配器 0.91.0 的源码注释："Recent models default `thinking.display` to "omitted", which streams signature-only thinking blocks whose text is empty"，文本为空时不发 `agent_thought_chunk`。客户端可以经 `session/new` 的 `_meta.claudeCode.options.thinking` 传配置，宿主没有传。thinking 剧本（sonnet）没有一条 thought；SP-1 的 s02d-thinking（haiku）也没有。
3. **拒绝计划会结束回合。** 选 `reject` 后适配器立刻以 `cancelled` 回答 prompt（工具调用标 `nonExecutionKind: user-rejected`）。协议 5.3.6 的选项表写的是 plan_revise、不结束回合、"继续规划"，宿主的选项映射也是这样。
4. **plan 模式换模型。** 宿主设了 model=haiku，plan 模式的回合实际用的是 sonnet，一回合约 0.10 美元。

## 问题结论

- **Q8 复核**：concurrent-two（2 个会话在同一个适配器上，每轮各同时排 3 个 prompt，3 轮）两轮全套都通过，每个会话只出现自己的标记，全剧本 49–52 s；six-sessions（6 个会话各 1 个 prompt 同时发）也通过，没有串话。T003 的结论在真实 CLI、宿主驱动下成立。
- **Q10**：入库的 19 份录制里有 37 个有应答的回合（含 3 个 `cancelled`），带 `cost` 的 `usage_update` 都是 prompt 应答之前的最后一条更新，相差 0–1 ms，都带 `_meta._claude/origin: {kind: "human"}`；回合内的 `usage_update` 不带 `cost`。prompt 不在途时除了 `available_commands_update`、`session_info_update` 和对 `set_config_option` 的 `current_mode_update`，没有别的更新。所有剧本都没有触发自发回合，"静默 10 秒"的兜底没有在真实 CLI 上用到，仍只靠假 agent（IT-K9-autonomous）验证。
- **Q28**：hooks-reload 事先放的 UserPromptSubmit hook 运行了（hooks 本身生效）；回合开始 3 秒后把 `.claude/settings.json` 改写成 PreToolUse hook，同一回合里的 Bash 命令触发了新 hook。结论：Claude Code 2.1.296 会在当前回合内加载改写后的 hooks。第一次冒烟没有阳性对照、改写后约 1 秒的命令没有触发，这一次以有对照、留出 3 秒的结果为准。
- **Q34（Claude）**：项目放在仓库的 `sub` 下（cwd 是 `sub`）。`sub/.claude/settings.json` 的 hook 运行了；git 根目录（`sub` 的上一级）和根目录之外的 `.claude/settings.json` 都没有运行。结论：2.1.296 只读会话 cwd 下的项目设置。现在的信任扫描范围（工作区到 git 根目录的每一级）比 CLI 读的宽，处在安全的一侧；编排者裁定不收窄（CLI 以后可能读得更宽）。
- **Q43**：resume-procs 5 轮"杀 `claude` 再发"，宿主后代进程数两轮全套分别是 6、6、6、5、5、5 和 6、5、5、5、6、5，没有累积。每轮适配器对下一个 prompt 回 "Session not found"（-32603），宿主照 K6 发 `session/resume`、重新设置 model 和 effort、重发 prompt。上游 #1011 在 0.91.0 上没有复现。
- **Q42**：任何剧本都没有发送 `/remote-control`；0.91.0 的源码里没有这条命令。

## 交接事项逐条核对

| 来源 | 事项 | 结论 |
|---|---|---|
| T054 | 常见位置 `~/.npm-global/bin`、nvm 只看 `~/.nvm`、原生安装器的布局 | 本机只有 Homebrew cask（`claude-code@latest`，`/opt/homebrew/bin/claude` 链到 `Caskroom/claude-code@latest/2.1.296/claude`），宿主从登录 shell 的 PATH 找到它，判为 native、verified；cask 名带 `@latest` 也认得。本机没有 `~/.npm-global`、`~/.nvm`、`~/.local/bin/claude`，`NVM_DIR` 也没设，这三处保守取舍在本机核实不了，维持原样 |
| T053 | 宿主不声明 `session.compaction` 时还推不推 `compaction_update` | 读 0.91.0 的源码：`clientSupportsCompactionUpdates` 只认 `clientCapabilities.session.compaction` 是对象；宿主的 `initialize` 没有声明，于是适配器用 `tool_call` 的呈现方式（一个标题为 "Compact conversation"、kind 为 `think` 的合成工具调用，事实放在 `rawOutput`），实时和回放都不发 `compaction_update`。套件里没有剧本触发压缩，没有实际录到 |
| T063、T003 | Q43 的进程数 | 见上面的 Q43：没有累积 |
| T064、T187、T192 | 真实模式第一次运行要留意的 | e2e 宿主包在登录 shell 的环境下找到了 `claude`；`--model` 被接受（`set_config_option model=haiku`，用量里是 haiku；只有 plan 模式被 CLI 换成 sonnet）；cancel 的"允许后等 1 秒再中断"可靠（命令改成 `touch … && sleep 60` 之后）；`claude` 的 0、1、2 是连到适配器的 Unix 套接字，没有继承宿主的管道，lsof 判失败是空地址的误报（已修）；`~/.claude.json` 的字段摘要没有误报，每次都是"没有变化"；config-scope、untrusted-project 事先放的 `.claude/settings.json` 没有让队列暂停，两个剧本都正常结束；crash-resume、resume-procs 杀掉 `claude` 后靠 K6 的 resume 恢复；close-running 3 秒收尾可行；5 分钟的等待上限只在 approval 等不到审批时用到过 |
| T188 | 已验证范围；适配器升级后基线怎么取 | 这份报告有失败（thinking），不计入；`compat-reports --write` 后清单不变（2.1.200–2.1.296），本机 CLI 本来就在上限。基线的取法：ADR-08 的起始值是在 0.91.0 上测出来的，只对 0.91.0 有效。锁定版本升级以后，起始值不再沿用，范围只由新适配器版本的报告算出；没有计入的报告时，`compat-reports` 应当报错，逼着升级前先跑一遍套件（5.9.5"升级适配器之前必须跑"）。实现放在升级适配器的任务里：把 `BASELINE` 和适配器版本绑在一起 |
| T194 | 评审工具 | `trace-ids status --compat <报告>`：COMPAT-claude-close-running、-resume-procs、-six-sessions、COMPAT-untrusted-project-claude 记为通过，COMPAT-claude-thinking 记为失败，和 `tests/ids.toml` 的登记一致；其余 16 个剧本列在"没有 COMPAT 编号"一节。`rec-shape-diff` 能读宿主导出的录制，拿 SP-1 的 s02 和新的 normal-turn 比，列出了方法和 `_meta` 键路径的差异；`toolResponse` 下的键随工具不同，会占去大半输出 |
| T196 | `VERSION_PROBE` 的 5 s | 实测：装好的 `claude --version` 约 10 ms；把 240 MB 的二进制复制一份再第一次执行，2.1 s（负载约 3），之后 10 ms。宿主启动时的探测在 20 ms 内完成。空闲时 5 s 够用，余量约 2.4 倍；T196 在负载 35–50 下量到 4–29 s，CLI 更新后的第一次探测在机器忙时仍可能超时。建议：超时后用更长的上限（例如 30 s）重试一次，而不是直接判为找不到 CLI（产品改动，由编排者定） |
| T004 | 升级适配器时重新评估静态扫描 | 本任务没有升级适配器，只评估：放行是白名单制，SDK 动态读取的约 1,160 个名字只有在用户把它们写进放行名单时才会传给适配器。两种补法里，按产物特征提取登记表依赖 SDK 打包后的内部形状，每次升级都可能失效；对 `CLAUDE_CODE_` 等前缀整体禁用、只放行逐个列出的名字，不依赖产物，建议升级时采用后者 |

## 0.93 候选版本：没有跑

跑候选版本要改运行时清单，按任务书不跑。缺的东西：
- 0.93.0 的锁文件（`runtime/locks/`）和清单里的条目：版本、打包产物的 sha-256（`runtime-pack` 算）、它读取的环境变量的归类（新出现的名字要人工归类，否则 `runtime-pack` 失败）。
- 宿主只从 app 包的 `Resources/runtime/` 读锁文件，所以还要一个带候选锁文件的 e2e 宿主包。test-harness 构建可以用 `test_config.runtime.manifest` 换清单，但 `xtask compat` 没有选适配器的参数，`host-bundle` 也只按锁定的清单打包。
- Q42：宿主还不拦截 `/remote-control`，也不会把它从命令列表里去掉。剧本不发这条命令，但升级前必须先有拦截（5.13 Q42）。

## 规模（实际）

每个提交之后都跑了 `cargo xtask size`。最终 +231 −91（不含 testdata、docs）：cupper-host 的执行者、剧本表、GOLD-B 运行器和 GOLD-A 条目 +151 −46，xtask +57 −43，假 agent +13，`tests/ids.toml` +4，`tools/sanitize` +6 −2。停止线 250 以内。
