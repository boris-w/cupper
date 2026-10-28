# T066 汇报

- 执行者：子 agent（测试工程师）
- 分支：`test/T066-claude-compat-real`（PR #15：https://github.com/boris-w/cupper/pull/15 ，已由编排者合并）
- 完成日期：2026-10-28

K12 的通过条件满足。全套用真实 CLI 跑了两轮，结果相同：19 个剧本通过、2 个观察、thinking 失败（产品问题）、steer 不适用。没有一个剧本需要重试；整套重跑了 1 次，上限是 2 次。GOLD-B-claude 覆盖了普通回合、审批、取消三个剧本。PR #15 的 8 个检查在合并提交 c75c070 上全部通过。

## 做了什么

**1. 运行情况**
- 环境：claude-agent-acp 0.91.0（锁定版本），Claude Code 2.1.296，`real_config` 模式。
- 模型：haiku，effort low；thinking 剧本用 sonnet，effort 取默认。
- 顺序照任务书：先 `--real --dry-run`，再冒烟 new-session，再跑三个 GOLD-B 剧本，然后逐个冒烟其余有风险的剧本，最后带 `--write` 跑全套。
- 全套第 1 轮的 `--write` 在报告的敏感词扫描处中止（框架的路径 bug）。修好后用已提交的代码整套重跑，入库的是第 2 轮。
- 额度：累计约 150 个回合（haiku 约 145，sonnet 约 5），约 1.2 美元，在用户同意的范围内。每次运行的明细在 `notes.md`。

**2. 入库的内容**（用户接受了编排者的抽查）
- 19 份录制，放在 `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/`。thinking 失败，它的录制没有入库。
- GOLD-B-claude：normal-turn、approval、cancel 三份。同三份录制也标为 `gold-a`，生成 A 层金样例，新登记 GOLD-A-claude-compat-normal-turn、-approval、-cancel 三个编号（编排者裁定）。
- 报告：`testdata/compat/reports/claude-agent-acp-0.91.0-claude-2.1.296.json`。`cleanup` 只写会话个数，不写会话 ID。
- MAN-compat-claude 的证据：运行记录 `man-compat-claude.md`，并在 `ids.toml` 里加了 `[[evidence]]`。

**3. 已验证 CLI 范围**
- 由 `compat-reports --write` 生成。这份报告有失败（thinking），不计入已验证范围，`runtime/manifest.json` 不变，仍是 2.1.200–2.1.296。本机的 CLI 本来就在上限，用户决定不升级。
- 适配器升级后基线怎么取（我的决定）：ADR-08 的起始值是在 0.91.0 上测出来的，只对 0.91.0 有效。升级以后起始值不再沿用，范围只由新适配器版本的报告算出；还没有计入的报告时，`compat-reports` 应当报错，逼着升级前先跑一遍套件（对应 5.9.5"升级前必须跑"）。实现留给升级适配器的任务：把 `BASELINE` 和适配器版本绑在一起。

**4. COMPAT 编号**
`trace-ids status --compat <报告>` 的结果和 `tests/ids.toml` 的登记一致：
- 通过：COMPAT-claude-six-sessions、COMPAT-claude-close-running、COMPAT-claude-resume-procs、COMPAT-untrusted-project-claude；
- 失败：COMPAT-claude-thinking。

## 关键取舍

框架本身的改动，每处的理由如下。放宽类的改动都先汇报、经裁定后才做。

- **等运行时装好再建会话。** 执行者先等索引流里 `adapter.runtime` 变成 `ready`，上限 5 分钟，变成 `failed` 就记失败。宿主的缺陷另开任务（编排者选的方案 A）。
- **要回答请求的剧本，改用会写文件的命令。** Claude Code 把 `echo`、`ls`、`sleep` 当作只读命令，直接运行，不发审批，approval 剧本因此白等了两次 5 分钟。approval、cancel、cancel-approval、command-exit、close-running、hooks-reload 都改了提示词，例如 `touch … && sleep 60`。检查本身没动。
- **effort 调低。** 用户默认的 effort 是 xhigh。haiku 的剧本改传 `reasoning_effort: low`；thinking 剧本保持默认。
- **lsof 检查的误报。**
  - 现象：宿主绑在路径上的 `host.sock`，和 claude 自己监听的 `/tmp/cc-socks/<pid>.sock`，在 lsof 里都没有对端，两边的空字符串一比就"匹配"了。claude 的 0、1、2 是连到适配器的 Unix 套接字，并没有继承宿主的管道。
  - 修法：空地址不参与比对。
  - 用例（UNIT-compat-guards 的 `unit_compat_guards_lsof_names_each_pipe_and_socket_end`）：双方都只有绑在路径上的套接字时，不算持有；后代持有宿主套接字的对端、宿主监听套接字本身（同一地址、没有对端）、连到宿主的管道，都仍算持有。
  - 变异检查：去掉"非空"这个条件，或者只比对端，各有一种情况会让用例失败。
- **报告不写 CLI 会话 ID。** 报告只写个数，完整 ID 写到运行输出的 `cli-sessions.txt` 和遗留清单里。
- **`tools/sanitize` 的脱敏漏洞。** 用户 skill 的名字和描述会被换掉，但它的 `input.hint` 原样留着。改成一并换成 `<redacted>`，补了用例，做过变异检查。
- **GOLD-B 运行器按录制路径区分剧本。** 原来只按文件名，真实录制和假 agent 录制同名会撞。回放时，`session/create` 的 model 和 effort 从录制里宿主发出的 `set_config_option` 推出。
- **plan 剧本的期望改成真实行为**（编排者裁定）。检查换成"plan_confirm 请求带着非空的计划正文"和"回合以 `cancelled` 结束"。假 agent 的 Claude 人设在拒绝计划后也结束回合，新增用例 `unit_fake_scripts_plan_rejected_by_claude_ends_the_turn`，做过变异检查。
- **`--write` 的门槛折中**（编排者裁定）。真实运行只要配置前后没有变化，就把报告照实写入，失败也写进去；录制只取通过和观察到的剧本；敏感词扫描照旧是硬门槛。`--write` 同时写 A、B 两层金样例，写完再不带 `UPDATE_GOLDEN` 检查一遍。"录制和金样例一一对应"的用例会和写入抢跑，所以放在检查那一步跑。
- **两个 bug。**
  - 报告的敏感词扫描传的是绝对路径，`words` 会把它拼到工作目录后面，找不到文件。改成传相对路径。
  - 兼容报告不该被 prettier 重排，把 `testdata/compat/reports/` 加进了 `.prettierignore`。f77d9fa 上 ts 作业挂的就是这一步。
- **Q28、Q34 加阳性对照。**
  - hooks-reload：项目里事先放一个 UserPromptSubmit hook，回合开始 3 秒后再改写成 PreToolUse hook。
  - config-scope：在项目目录自己的 `.claude/settings.json` 里也放一个 hook。
  - 都只是观察项，判定不变。own-ran 出现，也顺带证明 untrusted-project 的"hook 不运行"不是空检查：同样写法的 hook 在已信任的项目里确实会运行。
  - resume-procs 把每轮的进程数打印到日志。

## 验证方式

- K12：GOLD-B-claude 的三个剧本都通过；每个剧本都只跑了 1 次；整套重跑 1 次。
- `cargo xtask compat --fake`：18 个通过、steer 不适用、2 个 `real_only`；`--real --dry-run` 正常；`ci compat-framework` 15 项通过。
- `pnpm test:golden`：A 层 28 个、B 层 12 个用例通过。GOLD-B 连跑 3 次结果稳定。
- `trace-ids check` 通过；`cargo xtask ci testdata-scan` 通过，合并 main 之后又跑了一遍。
- 配置前后的对比：`~/.claude/settings.json`、`~/.claude/settings.local.json`、`~/.codex/config.toml`，以及 `~/.claude.json` 里和权限有关的字段，每次运行都没有变化，也没有新增项目条目。
- 敏感词与个人信息：
  - 录制经脱敏后复扫，clean；报告用清单扫过，clean；testdata 的通用扫描，clean；
  - 我另外按上下文里能看到的个人名词（用户名、邮箱、主机名、用户 skill 和 MCP 名字、`/Users/` 路径等）扫过 testdata，没有命中。
- 遗留清单已写进主仓库的 `.private/cleanup/T066-leftovers.md`：
  - 75 个 CLI 会话（73 个知道 ID），以及对应的项目目录（按命名规则推出来，没有读 `~/.claude`）；
  - `/private/tmp/claude-501/` 下 24 个 CLI 目录。清单里注明：编排者会话 `2738de2b-…` 的目录不能动；
  - `~/.claude/plans/` 下 3 个计划文件；
  - 可能有的 shell 快照。
- 规模：+233 −91，停止线 250 以内。每次提交后都跑了 `cargo xtask size`。
- CI：
  - c75c070 上 8 个检查全绿：https://github.com/boris-w/cupper/actions/runs/40636570998
  - 之前 f77d9fa 那一轮（https://github.com/boris-w/cupper/actions/runs/40634985484 ）只有 ts 挂在 prettier，其余 7 个都通过。
  - 1cacb39 和 8e8667f 没有触发 CI，原因是 PR 和 main 在 `ids.toml` 末尾有冲突。编排者把 main 合进分支解决后，CI 才跑起来。

## 提交列表

```
c75c070 Merge branch 'main' into test/T066-claude-compat-real   （编排者合入 main）
8e8667f fix(ci): prettier leaves the compat reports alone
1cacb39 docs(T066): MAN-compat-claude's evidence is the run record
f77d9fa docs(T066): the run record for MAN-compat-claude and the notes' conclusions
80f9bf2 docs(T066): notes on the estimate, the runs and what they showed
d353824 test(compat): the first real Claude recordings, goldens and report
fcc8912 test(compat): a real GOLD-B sample is a GOLD-A one too
cd30968 fix(xtask): the word scan reads a real report by its relative path
55e5998 feat(xtask): a real compat report enters testdata/ with its failures
a43e342 test(compat): resume-procs logs the host's process counts
a161b86 test(compat): the plan scenario checks the plan's text and the ended turn
469aafc test(fake-agent): Claude's rejected plan ends the turn
89b57e6 test(compat): Q28 and Q34 observe a hook that shows hooks run
1e61807 test(gold-b): a scenario is named by its recording's path
5955446 fix(sanitize): a user skill's input hint is redacted with its name
20ea179 fix(xtask): a real compat report counts the CLI's sessions, not names them
d19fa89 fix(compat): a socket bound to a path shares no address with another
3ea08dc fix(compat): real runs wait for the runtime and ask with writing commands
```

## 遗留问题 / 范围外发现

**问题结论**
- **Q8 复核**：两轮里 concurrent-two（2 个会话，每轮各同时排 3 个 prompt，共 3 轮）和 six-sessions 都通过，没有串话，concurrent-two 全程 49–52 s。
- **Q10**：入库录制里有 37 个有应答的回合。
  - 每个回合里，带 `cost` 的 `usage_update` 都是 prompt 应答之前的最后一条更新，相差 0–1 ms，并带 `_claude/origin: {kind: human}`；回合内的 `usage_update` 不带 `cost`。
  - 没有剧本触发自发回合。"静默 10 秒"的兜底在真实 CLI 上没有验证到，仍只靠假 agent 的用例。
- **Q28**：事先放的 hook 运行了；回合开始 3 秒后改写的 PreToolUse hook，在同一回合内就生效了。也就是说，回合中改写的 hooks 会被当前回合加载，5.8.7 里的"可能"可以改成"会"。第一次冒烟时没有阳性对照，改写后约 1 秒的命令没有触发新 hook；结论以有对照的这一次为准。
- **Q34（Claude）**：cwd 是 `sub`。`sub/.claude/settings.json` 的 hook 运行了；git 根目录（`sub` 的上一级）和根目录之外的都没有运行，所以 2.1.296 只读 cwd 下的项目设置。现在的信任扫描范围比 CLI 实际读的宽，处在安全的一侧；编排者已定不收窄，因为 CLI 以后可能读得更宽。
- **Q43**：5 轮"杀掉 `claude` 再发"之后，宿主后代进程数两轮分别是 6、6、6、5、5、5 和 6、5、5、5、6、5，没有累积，上游 #1011 没有复现。每轮的过程是：适配器回 "Session not found"（-32603），宿主照 K6 发 `session/resume`、重新设置 model 和 effort，再重发 prompt。
- **Q42**：没有剧本发送 `/remote-control`。

**0.93 候选版本：没有跑**，因为要改运行时清单。缺的东西：
- 0.93.0 的锁文件；
- 清单里的条目：版本、打包产物哈希、新环境变量的归类；
- 带候选锁文件的 e2e 宿主包，`xtask compat` 和 `host-bundle` 目前都没有选适配器的参数；
- Q42 的拦截，宿主还没有实现。

**交接事项**
- **T054**：本机只有 Homebrew cask（`claude-code@latest`），宿主经登录 shell 的 PATH 找到了它，判为 native、verified。本机没有 `~/.npm-global`、`~/.nvm`、`~/.local/bin/claude`，`NVM_DIR` 也没设，那几处保守取舍在这台机器上核实不了，维持原样。
- **T053**：读了 0.91.0 的源码。只有 `clientCapabilities.session.compaction` 是对象时，适配器才发 `compaction_update`；宿主没有声明，适配器就改用一个合成的工具调用（"Compact conversation"，kind 为 `think`），实时和回放都一样。套件里没有剧本触发压缩，没有录到实例。
- **T064、T187、T192 列的留意点**：
  - 登录 shell 下能找到 `claude`；`--model` 被接受（plan 模式除外）；
  - cancel 的"允许后等 1 秒再中断"可靠；lsof 判失败是误报，已修；
  - `~/.claude.json` 的字段摘要没有出现误报；
  - 事先放好的 `.claude/settings.json` 没有让队列暂停；
  - K6 的 resume 正常；close-running 3 秒收尾可行。
- **T188**：见上面的"已验证 CLI 范围"。
- **T194**：`trace-ids --compat` 的结果见上；`rec-shape-diff` 能读宿主导出的录制，但 `toolResponse` 下的键随工具不同，会占去大半输出。
- **T196**：装好的 `claude --version` 约 10 ms；把二进制复制一份后第一次执行是 2.1 s（负载约 3）；宿主启动时的探测在 20 ms 内完成。空闲时 5 s 够用，但 T196 在高负载下量到过 29 s。建议超时后用更长的上限（例如 30 s）重试一次，是否采用由编排者定。
- **T004**：本任务没有升级适配器，只评估。建议升级时改成对 `CLAUDE_CODE_` 等前缀整体禁用、只放行逐个列出的名字；按产物特征提取登记表依赖 SDK 打包后的内部形状，每次升级都可能失效。

**产品问题**（都已停下汇报，编排者另开了任务）
1. **安装期间发的 prompt 直接失败。** `driver/runtime/launch.rs` 的 `start` 遇到 `Program::Runtime`，在文件还没装好时就核对哈希，返回 `RuntimeFailed`；`supervisor/mod.rs` 在后台起 `Installer::run`，启动适配器前不等它。这和设计 5.2.12 的"等待安装完成"不符。
2. **Claude 的思考流是空的**（COMPAT-claude-thinking 失败的原因）。适配器源码注释写着："Recent models default `thinking.display` to "omitted""，思考文本为空时适配器不发 `agent_thought_chunk`；宿主没有经 `_meta.claudeCode.options.thinking` 请求显示。SP-1 的 s02d-thinking 录制里也没有 thought。
3. **拒绝计划会结束回合**（`cancelled`，工具调用标着 `nonExecutionKind: user-rejected`）。协议 5.3.6 的选项表和宿主的选项映射都写的是"继续规划"。
4. **plan 模式换模型**：宿主设了 haiku，CLI 在 plan 模式下用 sonnet，一回合约 0.10 美元。

**其他**
- `tools/sanitize` 的 `words`、`scan` 会把绝对路径拼到工作目录后面。本任务只在调用的地方改成传相对路径，工具本身没改。
- 以前入库的录制里，用户 skill 的 `input.hint` 要不要回查，由编排者另定。
- 草稿目录里有未脱敏的原始输出，在编排者会话的目录里。会话结束以后可以删，清理时不要动会话目录本身。
