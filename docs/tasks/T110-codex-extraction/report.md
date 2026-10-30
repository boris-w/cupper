# T110 汇报

- 执行者：子 agent（Rust 工程师）
- 分支：`feat/T110-codex-extraction`
- 完成日期：2026-10-30

## 做了什么

T110 的范围都做完了。PR #24 上 GitHub CI 的 8 个检查全部通过，Claude 的 21 份金样例逐字不变。规模按三次补充裁定计，分两栏：

| 栏 | 提交 | 行数 | 线 |
|---|---|---|---|
| 搬动 | 5c27a66 | +571 −447，按净增 **+124** 计 | 补充裁定二 |
| Codex 部分 | 07a890d、3cf5f16、e329f74 | **+1,025 −63** | 停止线 950、硬停止线 1050（补充裁定三，编排者同意按现状提交） |
| 数据（不是代码） | 96b8428 | `spikes/` 下两份录制 +72 −72；`testdata/` 不计 | — |

`cargo xtask size` 按总数和分支上的任务书报告，会超出任务书写的线。这是预期的。

1. **搬动提交 5c27a66**：Claude 模块里与 agent 无关的代码搬到 `driver/extract.rs` 和 `driver/options.rs`，A 层运行器用 `git mv` 移到 `driver/gold_a/`。每一块原来在哪里，列在 `notes.md` 的"搬动提交搬了什么"一节。
2. **选项与能力（07a890d）**：
   - **选项对照表**（`driver/codex/options.rs`）照 5.3.6 的 Codex 各行：
     - `allow_once` 为 once；
     - `allow_for_session` 只用于文件修改，为 session、privileged；
     - `accept_execpolicy_amendment` 只用于命令，持久、privileged，`rule.prefix` 取原文反引号里的部分；
     - `cancel` 为 reject，结束回合；
     - `implement_plan` 为 plan_accept；
     - `revise_plan` 为 plan_revise，结束回合（T102 实测）；
     - 表里没有的选项按 kind 兜底，reject 一律记为结束回合。
   - **场景**：`plan-review:` 开头的为计划确认，`edit` 为文件修改，`execute` 为命令，其余为 Other。
   - **档位表**（Q25）：`read-only`、`workspace-write` 为 normal，`agent` 为 elevated，`agent-full-access` 为 full_access，表外的取值为 full_access。
   - **配置项**：category `collaboration_mode` 映射为 `plan_mode`。
   - **能力**：不停命令（K3）、拒绝结束回合（K2）、插话无副作用、输出流式、`per_hunk`、`entries`、`agent_reported`。
3. **提取层（3cf5f16，`driver/codex/mod.rs`、`ask.rs`）**，照 5.6.3 的对照表：
   - `threadStatus` 变成 `RunState`：`active` 带两个标记；`idle`、`notLoaded` 为不活跃；`systemError` 为不活跃加 `system_error`。
   - `_meta.codex.error` 带 `willRetry` 时变成 `UpstreamRetry`，次数取自 "Reconnecting… n/m"。
   - 文本形式的错误（K17），见"关键取舍"。
   - 命令：`execute` 用标题作命令文本，cwd 取 `rawInput.cwd`；`_meta.terminal_output` 逐段追加；读取类命令的输出取 `content` 文本；退出码取 `terminal_exit`，没有时取 `rawOutput.exit_code`。
   - diff 每个 hunk 一条，按 `_meta.kind` 分 add、update、delete；用 shell 运行 `apply_patch` 时没有 diff。
   - 计划确认从 `rawInput.plan` 取计划。
   - JSON-RPC 错误分为 `not_logged_in`、`restartable`、`process_exited`、`session_not_found`、`quota_exhausted`、`adapter_exited`，消息只留第一行。
4. **A 层运行器与金样例（e329f74）**：
   - 运行器改用一张"适配器层"描述表，两种适配器共用。
   - SP-1 的 30 份 Codex 录制（0.159.2 的 14 份、0.160.1 的 16 份）都有金样例，按场景名分成 16 个用例。
   - GOLD-A-tool-fields-codex：命令、编辑和 shell 里的 `apply_patch`，并检查 30 份录制里没有一个工具在结束之后又来更新（T058）。
   - GOLD-A-option-map-codex："取值都在表里"的检查覆盖了 Codex（录制一个用例、内联录制一个用例）。
   - K17：s10x 的金样例，另加一份内联录制，覆盖两种不应误判的情形。
   - UNIT-option-map-codex、UNIT-capabilities 的 Codex 用例。
   - `tests/ids.toml` 的 T110 段追加了 GOLD-A-codex-s01、s02b、s03、s07b、s08、s09、s10、s11、s12。
5. **录制里的临时路径（96b8428，经编排者同意）**：Codex s05 最后一句回答里有每用户临时目录的路径，被切进约 35 个 chunk。改了 4 个副本：`testdata/` 下两份，`spikes/SP-1-adapter-behavior/recordings/` 下的 codex-local、codex-bundled 两份原件。帧数和 seq 不变，折叠后写成 `<tmp>`。文件和帧号列在 notes 里。
6. **notes（7c908a6）**：规模的估计、三次裁定、进度和收紧；搬动清单；各项取舍；K17；Q31、Q25；交接事项；录制的修正。

## 关键取舍

- **K17 的判定**：线程报 `systemError` 之后、下一次报 `active` 之前，不带 `messageId` 的消息块记为这个 prompt 的错误文本，prompt 返回 `end_turn` 时改为 `PromptFailed`（`form: text`）。分类按原文：含 "model" 和 "not supported" 为 `model_unsupported`，其余为 `other`。
  - **会触发的帧**：源码里不带 `messageId` 的块都是适配器自己写的（错误、配置警告、警告、压缩提示）；模型的文本总带条目 ID。
  - **不会误判的情形**：`systemError` 之后模型正常输出，带 ID，仍是消息；下一回合 `active` 之后不带 ID 的警告也仍是消息。内联录制覆盖了这两种。
  - **残余风险**：`systemError` 之后、下一次 `active` 之前来的警告，会并进错误文本，但不改变回合的结局。如果顺序反过来（先来文本、后报 `systemError`），会漏判；录制里是先报 `systemError`，留给 T113 核对。
- **Q31：不声明 AIR**。代价有三：
  1. 命令列表会带出 `skillPath`，即本机路径，会进原始帧，而 sanitize 不处理它；
  2. 工具调用改用 AIR 的渲染，审批请求、读取类命令的输出位置、提问表单都会变，所有录制和金样例要用真实 CLI 重录；
  3. AIR 是私有扩展，升级适配器时多一份核对。

  好处有限：Codex 的过程说明多在工具调用之前，宿主规则已经把它们排除在最终回答之外。
- **diff 每个 hunk 一条**：和能力 `per_hunk` 一致，也和 Claude 一样不合并。路径取 diff 的文件。
- **适配器给 `plan-review:` 的 `tool_call_update` 不出事件**：它之前没有对应的 `tool_call`，合并器会为它新建一个 `other` 条目，而这次确认已经是一个交互请求了。
- **JSON-RPC 错误消息只留第一行**：`1001` 的消息后面接着 app-server 的 stderr，里面有用户的配置路径和 MCP 服务器名。
- **搬动提交里不是原样搬动的约 170 行新增**（连同对应的删除，约 340 行）都是胶水，分几类：
  1. **import 和模块声明**（约 45 行）：搬走之后 use 列表重新分组；`driver/mod.rs` 加 `extract`、`options`、`gold_a`；运行时和 gold_a 改从 `extract` 取 `prompt_done`、`elicitation`。
  2. **可见性**（约 12 行）：搬进 `extract.rs` 的小函数从私有 `fn` 改为 `pub fn`。
  3. **档位表改成参数**（约 15 行）：新增 `Tiers` 类型和 `Session.tiers` 字段，`config_options(options, tiers)`、`tier(tiers, mode)`；Claude 留下传入自己 `TIERS` 的一行包装。
  4. **会话状态变成共用的 `Session`**（约 25 行）：`ClaudeSession` 改为新类型 `ClaudeSession(Session)`，字段访问改为 `self.0.paths`、`self.0.cwd()`；非工具分支委托给 `self.0.update`；`set_config`、`config`、`last_plan` 改为委托。
  5. **选项构造拆开**（约 25 行）：
     - `options::option(option, row, reject_ends_turn)` 接收查表闭包；
     - `by_kind` 增加 `reject_ends_turn` 参数，Claude 传 false，与原先相同；
     - 持久规则拆成 `rule(prefix)`；
     - 新增 `Row` 类型别名；
     - Claude 的 `option()` 改为调用它们。
  6. **文档注释**（约 45 行）：两个新模块的模块文档，各函数和类型的说明，Claude 模块文档里指向共用模块的两三句。
  7. **rustfmt 重排**（少量）：例如 `CommandsChanged(commands(…))` 折成三行。

  逻辑上只有第 3、5 类是参数化，传入的值让 Claude 走原来的路径。
- **为什么金样例逐字不变能证明行为没变**：A 层运行器把 21 份 Claude 录制的每一帧，依次送进和运行时相同的生产函数：`ClaudeSession::update`、`permission`、`set_config`，`prompt_done`，`agent_error`，`capabilities`，`config_options`，`elicitation`，然后把序列化的驱动事件逐字节和金样例比对。
  - 搬动和胶水涉及的分支都在这条路径上：消息、思考、计划 Markdown、命令列表、模式和配置变化、标题、用量与费用、压缩（s09）、提问（s05 和一个兼容场景），以及每个 session 行里按档位表算出的 `risk`。
  - 选项方面，录制里出现的全部选项都走一遍查表，含文件编辑和命令上的 `allow-with-updates`、全部 `exit-plan-*`；GOLD-A-unknown-enums 走未知 kind 的兜底。
  - 录制覆盖不到的兜底分支，由没有改动、照样通过的 UNIT-option-map-claude 覆盖：kind 不一致、`reject_always`、未知 kind、前缀与风险。`set_mode` 和档位由 UNIT-capabilities 覆盖。
  - 唯一没被录制执行到的是 `plan`（条目）分支，`--color-moved` 显示它是原样搬动，没有改动。

## 验证方式

- PR #24：https://github.com/boris-w/cupper/pull/24 ，提交 7c908a6 上的运行：https://github.com/boris-w/cupper/actions/runs/41005362501 。8/8 通过：rust、ts、protocol、ui、e2e、compat-framework、license、testdata-scan。
- 本机：
  - `cargo fmt --all --check` 通过。
  - `cargo clippy -p cupper-host --all-targets` 以默认特性和 `--all-features` 各跑一次，带 `-D warnings`，都无警告。
  - cupper-host 的 lib 测试 435 个通过，另有 1 个本来就是 ignored。
  - `trace-ids check` 一致。
  - `tools/sanitize` 的 `scan` 和主仓库的敏感词清单扫了 `testdata/` 和 SP-1 的原件，共 181 个文件，都干净。
  - 每个提交单独检查过：只保留它自己的改动，跑 clippy 和相关测试。
- 对照验收标准：
  - 计划 T110 一行的编号都已登记并通过：GOLD-A-codex-s02、s04、s04b、s05、s06、s07、s10x 以及其余录制的编号，GOLD-A-tool-fields-codex，GOLD-A-option-map-codex，UNIT-option-map-codex。✅
  - SP-1 的每份 Codex 录制都有 A 层金样例，"每份录制都有金样例"的匹配检查覆盖 Codex；"取值都在表里"的检查覆盖 Codex。✅
  - Q31 和交接事项逐条有结论，见下。✅
  - PR 上 8 个检查都通过。✅

**交接事项的结论**：
- **T042 `isOther` 和 `<id>_note`**：不合成一题。源码里，不论选了哪个选项，note 都会以 `user_note: ` 附在该问题的答案后面。合成为带 `allow_custom` 的一题，会失去"选项加备注"的写法。现在的两个字段已经能表达"其他"：选 "None of the above"，再在 note 里写。
- **T058 裁判与 idle**：`idle` 和 `notLoaded` 都发 `active: false`。30 份录制里没有任何工具在结束之后又来更新：s07 取消后补发的输出都在最终的 `completed` 之前。
- **T058 修复，空闲时的 thread active 要不要挡住出队**：现在不需要。录制里，prompt 之外从没出现过 `active`。取消之后线程立即 `idle`，命令还在跑，所以 `active` 本来也覆盖不了 K3 的晚到输出。Codex 自己开的回合一有输出就会开自发回合挡住出队。如果 T113 见到 prompt 之外的 `active`，再交给 T112 处理。
- **T055 第二种 agent 的 `ActorKind`**：本任务不拉起 Codex 的运行时，转给 T111。
- **T002 其余录制的编号**：已追加 9 个，见上。
- **T102 Q31、Q25**：见上。
- **给 T112**：K17 的 `error` 条目宿主还不写。回合会以 `failed` 结束，并带上 `error_code`（`model_unsupported`），但消息流里没有 `error` 条目。
- **给 T111**：
  - 运行时接 Codex 时，用 `CodexSession::new`、`update`、`permission`、`set_config`、`prompt_done`，`codex::agent_error`，`codex::capabilities::{capabilities, config_options}`。`CodexSession::config()` 是给 SetConfig 用的。
  - `prompt_done` 返回 `Result`：`Err` 时发 `PromptFailed`。

## 提交列表

```
7c908a6 docs(task): add the T110 notes
e329f74 test(host): add the A-layer goldens for Codex's recordings
3cf5f16 feat(host): extract Codex's frames into driver events
07a890d feat(host): add Codex's option table and tier table
96b8428 fix(testdata): drop a temp path split across Codex s05 chunks
5c27a66 refactor(host): move the agent-neutral extraction code out of Claude's
```

## 遗留问题 / 范围外发现

1. **`tools/sanitize` 缺一条"跨 chunk 的路径"规则**：逐帧扫描看不到被切进多个 `agent_message_chunk` 的路径，s05 的临时路径就是这样漏掉的。建议在脱敏和扫描时，先把同一条消息的文本折叠起来再匹配。编排者说另开任务。
2. **K24 的"条目 `extraction: partial`"做不到**：`extraction` 由合并器按必填字段判定，用 shell 运行 `apply_patch` 的条目是 `execute`，字段齐全，判为 `full`。提取层只能做到"没有 diff"（GOLD-A-codex-s02、s05）。要么在合并器里识别 `apply_patch`，要么改 5.6.9 K24 的写法，请编排者定。
3. **`initialize` 没有声明 `session.notices`、`session.compaction`**（SP-1 的客户端声明了）：真实运行时，Codex 的压缩提示和配置警告会以不带 ID 的消息文本出现，而不是 `compaction_update` 或 `notice`。留给 T111、T113 核对。
4. **codex-acp 有几种录制里没出现过的选项**：`decline`（不结束回合）、命令上的 `allow_for_session`、网络策略、权限档案、MCP 审批。现在都按 kind 兜底，`decline` 会被保守地标为结束回合。T113 录到这些选项之后，再把它们加进 5.3.6 和对照表。
5. **规模**：这个任务放宽了三次，原因是开工前的估计偏低：漏算了搬动范围，也低估了 rustfmt 展开后的行数和测试的行数。以后类似的"第二种 agent"任务，估计时建议把共用代码的搬动单独列一项。
6. **worktree 已保留。**
