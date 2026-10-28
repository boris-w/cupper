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
