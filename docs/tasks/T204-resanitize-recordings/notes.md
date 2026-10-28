# T204 工作笔记

- 执行者：子 agent（工程师）

## 开工前读过的

- 任务书；设计 5.9.3；计划 T204 一行。
- 卷宗：T066 的汇报和笔记（`input.hint` 的修法）；T102 的汇报和 `sanitize-t102.ts`；T052 的笔记、汇报和 `convert-sp1.ts`；SP-1 的 `sanitize.ts`；T003 的 `sanitize-t003.ts`。
- 代码：`tools/sanitize`、`tools/rec-shape-diff`、`crates/xtask/src/compat/record.rs`、`crates/xtask/src/ci/jobs/testdata_scan.rs`、`crates/cupper-host/src/driver/claude/gold_a.rs`。

## 开工时的状态

用录制本身取出私有字符串（不读 `~/.claude`、`~/.codex`），在全部入库文件里找它们出现在哪里。这里只记个数，不写字符串本身。

| 漏网的内容 | 出现在哪里 |
|---|---|
| 用户 skill 的 `input.hint`（1 条，中文） | SP-1 的 Claude 录制 16 份；T003 的录制 5 份（`claude/` 3 份、`q9/` 2 份）；`testdata/recordings/claude-agent-acp/0.91.0/2.1.296/` 里从 SP-1 转来的 16 份；对应的 A 层金样例 16 份（字段名 `input_hint`） |
| Codex 的 MCP 服务器名（1 个） | 不在任务书列的三种写法里，而在 Codex 自带 MCP 客户端的报错里：`failed to refresh OAuth tokens for server <名字>`。app-server 退出时，适配器把它的 stderr 放进错误应答的 `error.message`，SP-1 的客户端也把这段错误记进了 note。SP-1 的 Codex 录制 2 份、`testdata/recordings/codex-acp/` 里转来的 2 份（都是 `s07-cancel`），以及 SP-1 卷宗的 `data/summary.json`（摘录了这段错误） |
| 任务书列的三种写法、`skillPath` | 只在 T102 的录制里有，T102 的预处理脚本已经换掉了（`mcp.<redacted>.<工具>`、`"server":"<redacted>"`、`codex/<redacted>ExecutionDurationMs`、`<redacted skill path>`） |

- 本机敏感词清单扫 `testdata`、`spikes`、`docs/tasks/*/data`：clean（清单里没有上面两项）。
- 通用模式扫同一范围：只命中 4 个脚本和说明文件，是扫描规则本身写着的家目录前缀和一个像邮箱的图标文件名，不是录制。
- T066 的 19 份录制、它们的 A、B 层金样例和兼容报告，用户 skill 的提示已经是 `<redacted>`；这一组由 T202 重新生成，本任务不动。

## 行数估计（只算 `tools/sanitize` 和顺带的 xtask）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `sanitize.ts` | 收集服务器名（工具调用的标题和 `rawInput.server`、配置键、MCP 客户端报错），按任意拼写替换；键名也脱敏；`skillPath`；扫描时查服务器名 | 55 | — |
| `sanitize.test.ts` | UNIT-sanitize-codex-mcp：合成的 Codex 录制覆盖每种写法，再跑一遍不变 | — | 75 |
| `scan.ts` | `expand`、`scan` 接受绝对路径 | 5 | 15 |
| `xtask/src/compat/record.rs` | 去掉 T066 为这个 bug 绕的路（改传绝对路径） | 5 | — |
| 合计 | | 65 | 90 |

预计约 155 行，停止线 250。`cargo xtask size` 会把 `spikes/` 下重新脱敏的录制也算进去（它只排除 `docs/` 和 `testdata/`），按任务书这部分不计，每次提交后另记只含工具的数。

## 工具的改法（`tools/sanitize`）

- **服务器名从哪里取**：和会话 ID 一样，写出之前先把全部输入过一遍。取名字的地方：
  - Codex 的 MCP 工具调用（`_meta.is_mcp_tool_call` 为真，或者 `rawInput` 带 `tool`）的 `rawInput.server` 和标题 `mcp.<服务器>.<工具>`；
  - 文本里的 `mcp_servers.<服务器>`（Codex 警告里引用的配置键，原有规则只换这一处，名字在别处的写法换不到）；
  - 文本里的 `OAuth tokens for server <服务器>`（Codex 自带 MCP 客户端的报错，本任务新发现的一种）。
  - 形如 `<redacted>` 的占位不算名字，所以对自己的输出再跑一遍不变。
  - 标题只在确认是 MCP 调用时才取：否则一个以 `mcp.` 开头、带两个点的文件名标题会把普通单词当成服务器名，到处替换。
- **怎么替换**：照 `sanitize-t102.ts`，名字按非字母数字和驼峰边界拆开，各段之间允许 `_`、`-`、`.`、空格或什么都没有，不分大小写，所以 `foo_bar`、`fooBar`、`FOO-BAR`、`foo bar` 都换成 `<redacted>`。和 T102 不同的是加了边界：前面不能紧挨字母数字，后面不能紧跟小写字母或数字。这样驼峰键 `codex/fooBarExecutionDurationMs` 照样换，但名字是别的单词的前缀时（例如名字 `abc`、单词 `abcd`）不误换。
- **键名也脱敏**：原来只换值，`_meta` 键名里的服务器名漏掉；现在键名走同一套规则。
- **`skillPath`**：任何位置的 `skillPath` 字符串整体换成 `<redacted skill path>`，与 T102 的占位一致，所以 T102 已入库的录制重跑后不变。
- **扫描**：写完后的复扫也查这些名字（规则名 "MCP server name"）。
- **绝对路径**：`expand`、`scan` 改用 `resolve`，绝对路径和相对路径都行。xtask 的 compat 入库原来为这个 bug 改传相对路径，现在直接传报告的路径。

### 变异检查（UNIT-sanitize-codex-mcp）

每次改一处，跑这一个用例，13 处都让用例失败：去掉配置键、MCP 客户端报错、标题、`rawInput.server` 任一个来源；去掉 `is_mcp_tool_call` 或 `rawInput.tool` 任一个判定；键名不脱敏；不换 `skillPath`；各段之间只认 `_`；只认原来的大小写；不按驼峰拆；去掉"后面不能紧跟小写字母或数字"；占位也当名字（第二遍会把 `<redacted>` 换成 `<<redacted>>`）。

`words`、`scan` 的绝对路径用例在修之前失败（路径被拼到工作目录后面，`ENOENT`），修之后通过。

## 重新脱敏

原始录制都已经不在，所以对入库文件重跑工具：工具对自己的输出不变，旧规则已经换过的内容不会再动，只有新规则命中的地方会变。

1. 把要处理的录制拷进一个暂存目录，四组各放一个子目录，一次跑完，这样一组里取到的服务器名在其他组里也会替换：
   - `spikes/SP-1-adapter-behavior/recordings/`（48 份，SP-1 格式）；
   - `spikes/T003-tech-validation/recordings/` 的 `claude/`、`q9/`（5 份）；
   - `docs/tasks/T102-codex-validation/data/codex/`（4 份；T102 的录制在卷宗里，不在 `spikes/`）；
   - `testdata/recordings/` 里从 SP-1 转来的 48 份：Claude 的 `s01`–`s12` 共 18 份，Codex 的两个目录共 30 份。T066 的 19 份由 T202 重新生成，不放进来（注意 `six-sessions` 也以 `s` 开头，第一次误拷进去，去掉后重跑）。
2. `node tools/sanitize/src/main.ts sanitize --in <暂存> --out <输出> --terms <主仓库的敏感词清单>`：105 份，复扫 clean。
3. 逐叶比较新旧录制：163 行有变化，157 处是用户 skill 的 `input.hint`（37 份），6 处是错误信息里的服务器名（4 份），没有别的变化。T102 的 4 份没有变化。
4. 把输出拷回原处。
5. `UPDATE_GOLDEN=1 cargo test -p cupper-host --features test-harness --locked gold_ -- --skip recordings_and_goldens_match_up` 重新生成金样例：A 层 16 份有变化，53 行，都只是 `input_hint`；B 层没有变化。
6. 不是录制的数据文件（SP-1 卷宗的 `summary.json`、T003 的 `load-rates.json` 和 `results/*.json`、T102 的 `results/*.json`，共 10 份）工具不直接处理：把每份包成一行 `{"dir":"note","data":...}` 交给工具，再拆出来按原来的缩进（1 个空格，往返一致）写回。只有 `summary.json` 有 1 处变化（服务器名），其余 9 份不变。

### 逐份差异

录制用 `rec-shape-diff`（旧 → 新）比较，41 份都是"形状相同"：方法、更新类型、`_meta` 键、选项、停止原因、错误码都没变。金样例不是录制，`rec-shape-diff` 不适用，改用逐叶比较。"行数"是有变化的行数，"字段"是变了的叶子。

| 文件 | 行数 | 字段 | `rec-shape-diff` |
|---|---|---|---|
| `spikes/SP-1-adapter-behavior/recordings/claude/s01-init-session.jsonl` | 1 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s02-normal-turn.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s02b-plan.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s02c-plan-tasks-enabled.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s02d-thinking.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s03-exec.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s04-approval.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s04b-reject-command.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s05-questions.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s06-queue-steer.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s07-cancel.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s07b-cli-crash-recovery.jsonl` | 6 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s07x-sleep-backgrounded.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s09-load-resume.jsonl` | 7 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s11-image.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/claude/s12-resources.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/codex-bundled/s07-cancel.jsonl` | 2 | `msg.error.message`, `data.error.message` | 形状相同 |
| `spikes/SP-1-adapter-behavior/recordings/codex-local/s07-cancel.jsonl` | 1 | `msg.error.message` | 形状相同 |
| `spikes/T003-tech-validation/recordings/claude/q8-claude-0.91.0-hold.jsonl` | 10 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/T003-tech-validation/recordings/claude/q8-claude-0.91.0.jsonl` | 22 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/T003-tech-validation/recordings/claude/q8-claude-0.93.0.jsonl` | 13 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/T003-tech-validation/recordings/q9/q9-probe-claude-0.91.0.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `spikes/T003-tech-validation/recordings/q9/q9-probe-claude-0.93.0.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s01-init-session.jsonl` | 1 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s02-normal-turn.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s02b-plan.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s02c-plan-tasks-enabled.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s02d-thinking.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s03-exec.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s04-approval.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s04b-reject-command.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s05-questions.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s06-queue-steer.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s07-cancel.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s07b-cli-crash-recovery.jsonl` | 6 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s07x-sleep-backgrounded.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s09-load-resume.jsonl` | 7 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s11-image.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/golden/gold-a/claude-agent-acp/0.91.0/2.1.296/s12-resources.jsonl` | 3 | `event.commands_changed[].input_hint` | — |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s01-init-session.jsonl` | 1 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s02-normal-turn.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s02b-plan.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s02c-plan-tasks-enabled.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s02d-thinking.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s03-exec.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s04-approval.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s04b-reject-command.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s05-questions.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s06-queue-steer.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s07-cancel.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s07b-cli-crash-recovery.jsonl` | 6 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s07x-sleep-backgrounded.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s09-load-resume.jsonl` | 7 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s11-image.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s12-resources.jsonl` | 3 | `msg.params.update.availableCommands[].input.hint` | 形状相同 |
| `testdata/recordings/codex-acp/2.2.2/0.159.2/s07-cancel.jsonl` | 1 | `msg.error.message` | 形状相同 |
| `testdata/recordings/codex-acp/2.2.2/0.160.1/s07-cancel.jsonl` | 2 | `msg.error.message`, `note.error.message` | 形状相同 |

另外 `docs/tasks/SP-1-adapter-behavior/data/summary.json` 1 行（`codex-bundled.s07-cancel.notes[].error.message`）。

## 全仓库复扫

- 两个私有字符串（从录制里取出的 skill 提示和服务器名）在全部入库文件里：0 处。服务器名按"前后不连着别的字母数字"计；单按子串匹配会在代码里误命中别的单词，那些不是泄露。
- 新规则对没有重跑的文件（T066 的 19 份录制、假 agent 的 3 份、全部金样例 27 份、兼容报告）：经工具跑一遍再逐叶比较，0 处变化。
- 本机敏感词清单：`testdata`、`spikes`、`docs/tasks/*/data`、`docs/tasks/*/ci/*.json` 共 597 个文件 clean。扫全部入库文件时另有 65 处，不在录制和金样例里：62 处是任务汇报、评审里 GitHub 仓库链接中的仓库所有者名，3 处是 `Cargo.lock` 里第三方 crate 的名字。都不是本任务的范围，只报告。
- 通用模式：`testdata`、`docs/tasks/*/ci/*.json` clean；`spikes`、`docs/tasks/*/data` 只有 4 处，在脚本和说明文件里（扫描规则本身写着的家目录前缀，像邮箱的图标文件名），不是录制。

## 规模

`cargo xtask size` 在两次工具提交之后：+144 −27，其中 `tools/sanitize` +143 −23、`crates/xtask` +1 −4。它不排除 `spikes/`，重新脱敏的录制提交后会把 `spikes/` 下的 +/−163 行算进去，按任务书不计。
