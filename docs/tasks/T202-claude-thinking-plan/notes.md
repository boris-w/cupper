# T202 工作笔记

- 执行者：子 agent（Rust 工程师）

## 规模估计（开工前）

| 模块 | 内容 | 估计 |
|---|---|---|
| `driver/runtime/mod.rs` | `session/new`、`session/resume` 的 `_meta` 抽成一个函数，加上思考显示 | 20 |
| `driver/claude/options.rs` | 计划确认的 `reject` 改为 `ends_turn: true` | 5 |
| 测试 | UNIT-claude-session-thinking（runtime 的 tests，假 agent 的 `echo_meta`）约 45；UNIT-claude-plan-reject-ends-turn 约 25；IT-trust-settingsources 跟着改约 10 | 80 |
| 合计 | | 约 105，停止线 250 |

金样例（`testdata/`）不计入。

## 思考配置：按锁定版本核实

来源：从 npm 下载 `@agentclientprotocol/claude-agent-acp@0.91.0` 和它锁定的 `@anthropic-ai/claude-agent-sdk@0.3.300`，与 `runtime/locks/claude-agent-acp-0.91.0.package-lock.json` 里的 integrity 核对过，只放在本机临时目录里读，不进仓库（ADR-11）。行号是包里的文件。

**适配器怎么用 `_meta.claudeCode.options`**（`dist/acp-agent.js`）：
- 8115 行：`userProvidedOptions = sessionMeta?.claudeCode?.options`；8223 行把它整个展开进 SDK 的 `Options`。
- 8118 行：适配器自己只从环境变量 `MAX_THINKING_TOKENS` 推 `thinking`（10317 行 `resolveThinkingConfig`）。宿主的环境白名单（`runtime/manifest.json` 的 `env.allow`）不传这个变量，所以现在 SDK 收不到 `thinking`，用的是 CLI 的默认值。
- 8261–8264 行：`extraArgs` 是 `{...userProvidedOptions?.extraArgs, "replay-user-messages": ""}`，客户端给的项照样保留。
- 9732–9745 行：`thinking`、`thinking_delta` 的文本为空时不发 `agent_thought_chunk`。注释："Recent models default `thinking.display` to "omitted", which streams signature-only thinking blocks whose text is empty."
- 354 行 `OPTION_REBUILDS_SESSION`（368 行 `extraArgs: true`，398 行 `thinking: true`）和 455 行 `computeSessionFingerprint`：`session/resume`、`session/load` 的 options 和会话已有的不一样时，适配器会重建 Query 进程。所以 `session/new` 和 `session/resume` 必须带同一个值。

**SDK 怎么把它变成 CLI 参数**（`sdk.d.ts`、`sdk.mjs`）：
- `sdk.d.ts` 9831 行 `ThinkingAdaptive = {type: 'adaptive'; display?: 'summarized' | 'omitted'}`，9851 行 `ThinkingEnabled = {type: 'enabled'; budgetTokens?; display?}`，还有 `ThinkingDisabled = {type: 'disabled'}`。`type` 必填。
- `sdk.mjs`（压缩过，在第 157 行）：adaptive 和不带 budget 的 enabled 都变成 `--thinking=adaptive`；带 budget 的变成 `--max-thinking-tokens=N`；disabled 变成 `--thinking=disabled`；只要不是 disabled、又给了 display，就另加 `--thinking-display=<display>`。带 budget 的时候只有 `--max-thinking-tokens` 和 `--thinking-display`，没有 `--thinking`，可见 CLI 是单独读 `--thinking-display` 的。
- `sdk.d.ts` 1684 行 `extraArgs?: Record<string, string | null>`。`sdk.mjs` 第 157 行逐项变成 `--<键> <值>`（`DY`、`H_`），和 `--<键>=<值>` 是同一个参数。

**两种写法**：
- A：`thinking: {type: "adaptive", display: "summarized"}`，也就是 `--thinking=adaptive --thinking-display=summarized`。`type` 必填，所以会把思考强制打开：用户在自己的 Claude 配置里关了思考（`alwaysThinkingEnabled: false`、按模型的开关），经 cupper 也照样思考、照样计费。宿主不读 `~/.claude`，判断不出用户关没关。
- B：`extraArgs: {"thinking-display": "summarized"}`，也就是 `--thinking-display summarized`。只请求显示方式，思考开不开、档位多少仍然按用户的 CLI 配置。

编排者选了 B（2026-10-28），理由是不覆盖用户的 CLI 配置（设计 5.6.5），宿主也不用猜用户的配置。设计 5.6.3 的"思考"一行已经按 B 改了。

**哪些 CLI 认 `--thinking-display`**：SDK 0.2.100、0.3.142、0.3.200 的 `sdk.d.ts` 都有 `ThinkingAdaptive.display`，`sdk.mjs` 也都会发 `--thinking-display`。SDK 0.3.N 和 Claude Code 2.1.N 对应（SDK 的 CHANGELOG："Updated to parity with Claude Code v2.1.N"），所以已验证范围 2.1.200–2.1.296 内的 CLI 都认这个参数。

**哪些模型或档位不该带**：
- B 不改思考的开关，关了思考的会话不受影响。Claude Code 2.1.183 修了"display 和 disabled 一起发给 API 时报 400"的问题（CHANGELOG："Fixed `thinking.disabled.display: Extra inputs are not permitted` 400 errors"），已验证范围的下限 2.1.200 在它之后。
- 档位（`reasoning_effort`，适配器的 `effort`）和显示方式无关，B 不碰。
- `highlights` 只有 Anthropic 托管的会话能用（`sdk.d.ts` 5127 行），所以只用 `summarized`。
- 第三方服务商（Bedrock、Vertex）上 `summarized` 是否生效没有办法从 SDK 看出来，由 CLI 决定，不在本任务的真实运行里。

## 计划拒绝

- `driver/claude/options.rs`：`(PlanConfirm, "reject", RejectOnce)` 的 `ends_turn` 改为 `true`，其他字段不变，与设计 5.3.6 一致。
- 其他场景的 `reject` 不变，仍然 `ends_turn: false`，`capabilities.rs` 的 `interactions.reject_ends_turn` 说的就是这种拒绝（"Rejecting a permission ends the turn (Codex #556)"），仍然是 false。
- `extraction.thinking` 仍然是 true（设计 5.6.3：适配器支持，宿主请求显示后就有）。
- 界面：`RequestPanel.tsx` 的 `Body` 把 `plan_confirm` 交给 `OptionsKind`，`kinds.tsx` 的 `notes()` 对 `effect.ends_turn` 为真的选项加"拒绝将结束本回合"，和 Codex 的 `cancel`、`revise_plan` 走的是同一条路，界面不用改。

## 修前修后对照

测试先写，在没改的代码上跑一次，再改代码跑一次（`cargo test -p cupper-host --lib -- unit_claude_session_thinking unit_claude_plan_reject_ends_turn`）。

| 用例 | 修前 | 修后 |
|---|---|---|
| UNIT-claude-session-thinking（`driver/runtime/tests.rs`，假 agent 的 `echo_meta`） | 失败：信任的项目新建会话时 `_meta` 是 `null`，`left: Null, right: {"extraArgs": {"thinking-display": "summarized"}}` | 通过：新建、resume（信任）带同一个 `{"extraArgs": {"thinking-display": "summarized"}}`；resume（不信任）另带 `settingSources: ["user"]` |
| UNIT-claude-plan-reject-ends-turn（`driver/claude/options.rs`） | 失败：`left: (Some(false), "plan_revise", Approve), right: (Some(true), "plan_revise", Approve)` | 通过：只有计划确认的 `reject` 是 `ends_turn: true`；四个 `exit-plan-*` 和文件编辑、命令、未知工具的 `reject` 都是 false |

## 受影响的测试和金样例

- **GOLD-A**：只有 `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s05-questions.jsonl` 第 41 行变了，用 `UPDATE_GOLDEN=1` 重写后逐字段比对，只有 `/event/ask/request/classified/options/4/effect/ends_turn` 从 false 变成 true，就是 ExitPlanMode 请求里的 `reject`（"No, keep planning"）。其他 GOLD-A 录制里没有计划确认的 `reject`。GOLD-A 不看宿主发出的请求，所以 `_meta` 的变化不影响它。
- **GOLD-B（假 agent）**：回放按 T067 的规则匹配宿主的消息，忽略 `_meta`，三份金样例不变，照常通过。
- **IT-trust-settingsources**（`tests/interaction_trust.rs`）：原来断言信任的项目 `meta=null`、不信任的只有 `settingSources`；现在两边都带 `extraArgs`。期望值按宿主写入的键顺序写：工作区的 serde_json 开了 `preserve_order`，`said` 比的是文本。
- **T066 的录制**：T066 分支里新的真实录制（包括 `plan.jsonl`）也标了 `gold-a`。T066 先合并的话，它的 GOLD-A 是按旧映射写的，本分支要跟着重写那份金样例（只会是计划 `reject` 的 `ends_turn`）；本分支先合并的话，T066 写金样例时自然就是新值。

## 本机跑过的检查

- `cargo test -p cupper-host --lib -- driver::`：77 个通过（含 GOLD-A 23 个）。
- `cargo test -p cupper-host --features test-harness --test gold_b --test interaction_trust`：9 个、7 个都通过。
- `cargo clippy -p cupper-host --all-targets --features test-harness -- -D warnings`、`cargo fmt --all -- --check`：通过。
- `cargo xtask size`：+95 −11（净 +84），停止线 250。

## 合并 main 之后（b23f06f，含 T066）

- T066 最后只把 approval、cancel、normal-turn 三份真实录制标成 `gold-a`、`gold-b`；`plan.jsonl` 只标了 `replay`，没有 GOLD-A 金样例，所以没有要按新映射重写的。
- 合并后重跑：GOLD-A 26 个、GOLD-B 12 个（含 3 个真实 Claude 的）、`interaction_trust` 7 个，都通过。GOLD-B 回放忽略 `_meta`，真实录制里旧的 `session/new`（不带 `_meta`）照样能对上。

## 真实运行（COMPAT-claude-thinking，用户 2026-10-28 许可）

命令：`cargo xtask compat --real --model sonnet --only thinking --only plan --out <worktree>/target/compat-T202`，跑一次，没有重试。先跑过 `--dry-run`：两个剧本都没有被约束拒绝，`files` 为空，临时仓库在用户自己的临时目录里。`--out` 要写绝对路径：执行者在 `crates/cupper-host` 下运行，相对路径找不到计划文件。

| 剧本 | 结果 | 模型、effort | 回合 | 约合 |
|---|---|---|---|---|
| plan | 通过：计划正文、请求已回答、没有 hello.txt、回合 `cancelled` | sonnet-5-5，low | 1 | 0.154 美元 |
| thinking | 失败：没有 `thought` 条目 | sonnet-5-5，用户默认的 xhigh | 1 | 0.056 美元 |

- 两个剧本的 `session/new` 都带 `{"claudeCode":{"options":{"extraArgs":{"thinking-display":"summarized"}}}}`。
- 配置的 19 项前后都没有变化。运行后没有进程留下，临时目录都已由执行者删除。遗留清单写在主仓库的 `.private/cleanup/T202-leftovers.md`（2 个 CLI 会话），不入库。
- **plan 回合有 14 条 `agent_thought_chunk`**，摘要正文共 130 个字符。T066 修复前录的 `plan.jsonl`（`session/new` 不带 `_meta`）一条都没有，T066 入库的全部真实录制里也一条都没有。思考显示在真实 CLI 上生效了。
- plan 选 `reject` 之后，prompt 以 `cancelled` 返回，与 `ends_turn: true` 一致。
- thinking 回合直接回答了 "17 × 23 = 391"，上下文只多了 51 个 token，与可见的回答长度相当：这个回合模型没有思考，不是思考了但显示被省掉。同一次运行里 plan 回合在 sonnet 上思考了，所以不是 sonnet 默认不思考；更可能是 adaptive 认为这道题不用想。按编排者的要求，停下汇报，没有改成 A，也没有重跑。
- `xtask compat` 拒绝 `--real --write --only`（只跑部分剧本的报告不进 `testdata/compat/reports/`），这次没有 `--write`。报告和录制先留在本机的运行输出里，怎么入库由编排者定。

## 补充：换题、整套重跑（编排者 2026-10-28，用户许可约 150 个回合、1.5 美元）

**换题**（`2893c4e`）：thinking 剧本的 prompt 换成一道排名次的小逻辑题：五个人，三个约束（Ben 紧跟在 Ana 后面；Cy 在 Dee 前、Eli 后；Dee 不是最后）。要分情况讨论（Eli、Cy、Dee 只能占 1、2、3 名，Ana、Ben 才能连着排），唯一答案是 Eli, Cy, Dee, Ana, Ben，用穷举核对过。检查和假 agent 的剧本不变；`--fake --only thinking` 通过，compat 的单元测试通过。

**整套重跑**：`cargo xtask compat --real --write`（其余剧本用 haiku，thinking 用自己的 sonnet），先跑过 `--dry-run`。一次就过，没有剧本重试，整套也没有重跑：19 个通过、2 个观察（hooks-reload、config-scope，与 T066 相同）、steer 不适用。配置的 19 项前后都没有变化。没有进程遗留（运行后还在的 `claude` 进程都是今天之前启动的交互式会话，没有一个是 SDK 拉起的），临时目录都已删除。遗留清单追加在 `.private/cleanup/T202-leftovers.md`（26 个 CLI 会话）。

| 剧本 | 结果 | 录制里的 thought 条数 |
|---|---|---|
| thinking（sonnet-5-5，用户默认的 xhigh） | 通过，答案 "Eli, Cy, Dee, Ana, Ben"，正确 | 38（摘要共 381 个字符） |
| plan（sonnet-5-5） | 通过，reject 后回合 `cancelled` | 14 |
| command-exit、question、reject、image、resume-procs、untrusted-project（haiku-5-5） | 通过 | 19、14、6、13、21、18 |
| 其余 | 通过或观察 | 0 |

T066 那批录制里一条 thought 都没有。

**回合与费用**：录制里带 `cost` 的回合 38 个、0.342 美元；并发剧本另几个会话的回合不在导出的录制里，按 prompt 数补上（concurrent-two 另 10 个，six-sessions 另 10 个，close-running 1 个没有结束），一共约 59 个回合、约 0.45 美元。加上前面只跑两个剧本的那次（2 个回合、0.21 美元），本任务累计约 61 个回合、约 0.66 美元，在许可的范围内。

**入库内容**：19 份录制重写，新增 `thinking.jsonl`（T066 那次 thinking 失败，没有录制）；报告 `testdata/compat/reports/claude-agent-acp-0.91.0-claude-2.1.296.json` 取代 T066 的那份（`commit` 是 `2893c4e`）；A、B 两层的三份真实 Claude 金样例（approval、cancel、normal-turn）跟着重写。

**和 T066 那批的差异**（`rec-shape-diff`，19 份逐份比对）：
- 全部 19 份：`session/new` 多了 `_meta.claudeCode.options.extraArgs.thinking-display`；crash-resume、host-restart 的 `session/resume` 也多了。这是本任务的改动。
- 7 份多了 `agent_thought_chunk`：command-exit、image、plan、question、reject、resume-procs、untrusted-project。这是本任务要的结果。
- 其余是模型这一次怎么做的不同，和宿主无关：
  - approval、command-exit、image、six-sessions 少了 `_claude/rateLimit` 那组 `_meta`。额度窗口的更新只在变化时才发。
  - cancel-approval 少了 `agent_message_chunk`：这次模型在请求之前没有先说话。
  - image 多了一次 Read 工具调用（`tool_call`、`tool_call_update`，`toolResponse.file`）：这次模型用工具读了图片。
  - plan 少了 `terminal_*` 那组 `_meta`：T066 那次模型在规划时运行过一条命令，这次没有。
- 金样例：三份真实 Claude 的 GOLD-A、GOLD-B 都没有 thought（这三个剧本这次没有思考）。差异只是条数：usage 更新、命令列表事件少一两条，cancel 的回复文本多一段。`s05-questions` 等 SP-1 的 GOLD-A 不受影响。
- 写入后 `--write` 自己跑的 GOLD-A 28 个、GOLD-B 12 个都通过，`sanitize scan testdata` 报 clean。

**扫描**：27 个改动的文件（报告、6 份金样例、20 份录制）`sanitize words --list`（主仓库的敏感词清单）和 `sanitize scan` 都报 clean。`notes.md` 也过了 `words`。报告里没有本机路径和会话 ID（会话只记个数）。

**已验证范围与编号**：`compat-reports --write` 之后 `runtime/manifest.json` 没有变化，仍然是 2.1.200–2.1.296（本机 CLI 本来就在上限）。这份报告全部通过，计入范围。`trace-ids status --compat <新报告>`：COMPAT-claude-thinking 变为"passed in compat"，另外 4 个 COMPAT 编号仍然通过。
