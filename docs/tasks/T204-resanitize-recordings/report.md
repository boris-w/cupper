# T204 汇报

- 执行者：子 agent（工程师）
- 分支：`test/T204-resanitize-recordings`（PR #20：https://github.com/boris-w/cupper/pull/20 ）
- 完成日期：2026-10-28

**CI 还没跑完。** 交汇报时，PR #20 上的运行 https://github.com/boris-w/cupper/actions/runs/40643620891 还在排队，8 个检查都是 pending。你原本要我等 8 个检查跑完再交汇报，但系统要求我现在就交。请你在 PR 上看 8 个检查的结果；失败的话把我叫回来修。

## 做了什么

**1. 补 `tools/sanitize`**
- **Codex 的 MCP 服务器名。** 和会话 ID 一样，写出之前先把全部输入过一遍，从下面这些地方取名字：
  - MCP 工具调用的 `rawInput.server`；
  - MCP 工具调用的标题 `mcp.<服务器>.<工具>`；
  - 文本里的 `mcp_servers.<服务器>`；
  - 文本里的 `OAuth tokens for server <服务器>`。这是本任务新发现的一种写法，见"遗留问题"第 1 条。
- **替换方式。**
  - 取到的名字在所有字符串里都换成 `<redacted>`，键名也换。所以 `codex/<服务器>ExecutionDurationMs` 这种键能换到。
  - 拼写方式照 T102 的 `sanitize-t102.ts`：snake、kebab、驼峰、空格、连写都算，不分大小写。
  - 比 T102 多加了一道边界：名字前面不能紧挨字母或数字，后面不能紧跟小写字母或数字。这样驼峰键照样能换，但名字是别的单词的前缀时不会误换。
  - 形如 `<redacted>` 的占位不算名字，所以对工具自己的输出再跑一遍，结果不变。
  - 标题只在确认是 MCP 调用时才取（`_meta.is_mcp_tool_call` 为真，或者 `rawInput` 带 `tool`），以免把文件名当成服务器名。
  - 写完后的复扫也查这些名字。
- **`skillPath`。** 出现在任何位置都整体换成 `<redacted skill path>`。这和 T102 用的占位一致。
- **`words`、`scan` 接受绝对路径。** `crates/xtask/src/compat/record.rs` 里 T066 为这个 bug 改传相对路径的做法已经去掉，现在直接传报告的路径（−4 +1）。
- **UNIT-sanitize-codex-mcp 通过**，编号原本就登记在 `tests/ids.toml` 的 `[T204]` 段。
  - 用例用合成的 Codex 录制，每种写法都有一个只靠它取到的名字，再在别处用另一种拼写出现。
  - 变异检查做了 13 处，每一处都让用例失败：4 个取名来源、2 个 MCP 调用的判定、键名脱敏、`skillPath`、分隔符、大小写、驼峰拆分、后边界、占位过滤。
  - 绝对路径的用例在修之前失败（`ENOENT`），修之后通过。

**2. 重新脱敏已入库的录制**
- 原始录制都已经不在，所以对入库文件直接重跑工具。工具对自己的输出不变，所以只有新规则命中的地方会变。
- 四组放进同一个暂存目录一次跑完，一组里取到的名字在其他组里也会替换：
  - SP-1 的 48 份；
  - T003 的 5 份；
  - T102 的 4 份（在 `docs/tasks/T102-codex-validation/data/`）；
  - `testdata/recordings/` 里从 SP-1 转来的 48 份。
- 105 份都跑过，复扫 clean。逐叶比较新旧版本，163 行有变化：
  - 157 处是用户 skill 的 `input.hint`，共 37 份（SP-1 16 份、T003 5 份、testdata 16 份）；
  - 6 处是错误信息里的 Codex MCP 服务器名，共 4 份（都是 `s07-cancel`）；
  - 没有别的变化。T102 的 4 份完全没变。
- 41 份有变化的录制，`rec-shape-diff` 的结果都是"形状相同"。
- 重新生成金样例后，A 层有 16 份变化，共 53 行，都只是 `input_hint`；B 层没有变化。逐份的差异表在 `notes.md`。
- 不是录制的数据文件有 10 份：SP-1 卷宗的 `summary.json`、T003 和 T102 的结果文件等。做法是把每份包成一行交给工具，再按原来的缩进写回（往返一致）。只有 `summary.json` 有 1 处变化（同一个服务器名）。
- **T066 的 19 份录制、它们的 A、B 层金样例和兼容报告没有动**，这一组以 T202 的结果为准。我用新规则跑过一遍，没有任何会变的地方。

**3. 全仓库复扫**
- 从录制里取出两个私有字符串：那条 skill 提示和那个服务器名。在全部入库文件里都已经是 0 处。
- 本机敏感词清单扫 `testdata`、`spikes`、`docs/tasks/*/data`、`docs/tasks/*/ci/*.json`，共 597 个文件，clean。
- main 上新加的 `testdata/codex-drift/` 两个文件，敏感词扫描和通用扫描都 clean。

## 关键取舍

- **服务器名加了边界。** T102 的写法是纯子串替换。这个名字恰好是一个常见英文单词的前缀，纯子串替换会把那个单词也换掉。加边界之后驼峰键照样能换。
- **标题和 `rawInput.server` 都要先确认是 MCP 调用才取。** 一旦误取到一个普通单词，它会在所有录制里被替换。
- **键名也走全套脱敏规则**，不只换服务器名：键里出现的路径、会话 ID 也应该换掉。
- **四组一次跑完**，名字和会话 ID 在各组之间一致。
- **顺手改了 `record.rs`。** 绝对路径修好后，原来那段注释和绕路就过时了。改动只有 3 行，T202 不碰这个文件。

## 验证方式

- [x] **UNIT-sanitize-codex-mcp** 通过，已登记；变异检查 13 处全部被测出。
- [x] **已入库的录制和金样例**里，不再有用户 skill 的 `input.hint`、Codex 的 MCP 服务器名、`skillPath` 的本机值。你已经用敏感词清单复核过 64 个文件。
- [x] **金样例差异逐份说明**（`notes.md` 的表）；A、B 层金样例测试通过。
- [ ] **PR 上的 8 个检查**：交汇报时还在排队。运行链接：https://github.com/boris-w/cupper/actions/runs/40643620891
- 本机跑过的检查：

| 检查 | 结果 |
|---|---|
| `tools/sanitize` 的 vitest | 11 个通过 |
| tsc、eslint、prettier | 通过 |
| `cargo clippy -p xtask` | 通过 |
| `gold_` 测试 | A 层 28 个、B 层 12 个通过 |
| `cupper-protocol` 的 recordings、`cupper-fake-agent` 的 persona、`unit_replay_a_host_runs_an_sp1_claude_turn` | 通过 |
| `trace-ids check` | 通过 |
| `testdata` 和 CI 摘要的通用扫描、敏感词扫描 | clean |
| `git merge-tree` 对 main | 无冲突 |

- **规模**：`tools/sanitize` +143 −23，`crates/xtask` +1 −4，合计 +144 −27，在 250 的停止线以内。`cargo xtask size` 显示 +251，是因为它把 `spikes/` 下重新脱敏的录制（±107）也算了进去；按任务书这部分不计，你已经接受。

## 提交列表

```
c312241 docs(T204): notes on the estimate, the rules and the re-sanitizing
3b6bf0d test(recordings): re-sanitize the converted SP-1 recordings and their goldens
5771abd test(spikes): re-sanitize the SP-1 and T003 recordings
2921fcb fix(sanitize): words and scan take absolute paths
686e952 fix(sanitize): redact Codex's MCP server names and AIR skill paths
```

## 遗留问题 / 范围外发现

1. **服务器名的第四种写法（新发现）。**
   - 漏网的服务器名不在任务书列的三种写法里，而在 Codex 自带 MCP 客户端的报错 `failed to refresh OAuth tokens for server <名字>` 里。
   - app-server 退出时，适配器把它的 stderr 放进错误应答的 `error.message`；SP-1 的客户端也把这段错误记进了 note 和 `summary.json`。
   - 工具现在从这里也取名字。Codex 可能还有其他报错文本也带服务器名，现在只认实际见过的这一种。T113 的 Codex 兼容运行如果见到新的写法，要补进工具。
2. **Claude 的 MCP 服务器名只靠旧规则。** 它们靠 `mcp__` 规则、`*mcp_servers` 数组的规则和 `--terms` 处理，没有按"取名再全局替换"的方式做。像 "Some MCP servers are still connecting: <名字>" 这种文本，现在只靠敏感词清单兜底。现有的录制里没有漏网，所以本任务没改。要不要把 Claude 的来源也纳入取名，由你定。
3. **敏感词清单在录制以外的命中。** 扫全部入库文件时还有 65 处，都不在录制和金样例里：
   - 62 处：任务汇报和评审里 GitHub 仓库链接中的仓库所有者名；
   - 3 处：`Cargo.lock` 里第三方 crate 的名字。
   - 都不是本任务的范围，只报告，没有改。
4. **`cargo xtask size` 不排除 `spikes/`。** 以后再改 spike 录制的任务都会被多算。要不要把 `spikes/*/recordings/` 加进排除列表，由你定。
5. **T202 的录制要等 T202 合并后再查一次。** T202 重新生成的 19 份如果是用本分支合并前的工具脱敏的，它们是 Claude 的录制，不涉及 Codex 的写法。但为保险，T202 合并后可以用本分支的工具再扫一遍。
