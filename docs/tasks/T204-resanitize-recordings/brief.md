# T204 已入库录制的补充脱敏

- 派发日期：2026-10-28
- 执行者：子 agent（工程师）
- 分支：`test/T204-resanitize-recordings`
- 依赖：T066、T102（都已合并）。T099 依赖本任务

## 背景

两个真实运行的任务各发现了脱敏工具 `tools/sanitize` 的漏洞：
- **T066**：用户自己的 skill，名字和描述会被换成 `user-skill-N`，但它的 `input.hint` 原样留着。T066 已经在工具里补上，但以前入库的录制没有重新脱敏。编排者核对过：SP-1 和 T003 的 Claude 录制、以及从它们拷进 `testdata/` 的录制和 A 层金样例里，都还留着用户一个 skill 的中文参数提示。
- **T102**：Codex 的 MCP 服务器名，工具只认 Claude 的写法 `mcp__<服务器>__<工具>`，不认：
  - Codex 的工具标题 `mcp.<服务器>.<工具>`；
  - `rawInput.server`；
  - `_meta` 键名里的驼峰写法，例如 `codex/<服务器>ExecutionDurationMs`。

  客户端声明 JetBrains AIR 时，命令列表的 `skillPath` 会带出本机路径和插件名，工具也不换。T102 用 spike 自己的预处理脚本绕过了这几处。
- **T066 还发现**：`words`、`scan` 收到绝对路径时，会把它拼到工作目录后面，然后找不到文件。

仓库是公开的。git 历史按用户 2026-10-26 的决定不重写，本任务只清理当前文件。

## 范围

**做**（计划 T204 一行；设计 5.9.3）：
1. **补 `tools/sanitize`**：
   - Codex 的 MCP 服务器名，包括上面三种写法，按录制里出现的服务器名取出来再全局替换，参照 T102 的 `spikes/T102-codex-validation/sanitize-t102.ts`；
   - `skillPath`；
   - `words`、`scan` 接受绝对路径；
   - 新增 UNIT-sanitize-codex-mcp，用合成的录制覆盖每种写法，做变异检查。
2. **重新脱敏已入库的录制**：
   - 范围：`spikes/` 下 SP-1、T003、T102 的录制；`testdata/recordings/` 下全部真实录制；从它们生成的 `testdata/golden/`；
   - 用新规则重跑脱敏。不能重跑的（原始录制已经不在），就对入库文件直接做替换，替换规则和工具一致；
   - 重新生成受影响的金样例，用 `rec-shape-diff` 逐份说明差异：只应该是被替换的字段；
   - `cargo xtask ci rust` 里的 A 层、B 层金样例照常通过。
3. **全仓库复扫**：用新规则和主仓库的本机敏感词清单（`.private/sensitive-words.txt`），扫一遍全部录制和金样例；命中的逐条处理。

**不做**：重写 git 历史；改录制格式。

## 验收标准

- [ ] UNIT-sanitize-codex-mcp 通过，登记在 `tests/ids.toml`。
- [ ] 已入库的录制和金样例里，不再有用户 skill 的 `input.hint`、Codex 的 MCP 服务器名、`skillPath`。编排者会用敏感词清单复核。
- [ ] 重新生成的金样例差异逐份说明，A 层、B 层金样例测试通过。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：250；硬停止线：320

<!-- 只算 `tools/sanitize` 的代码和测试；重新脱敏的录制、金样例不计入。执行者开工前在 `notes.md` 里按模块写行数估计。每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.9.3（录制、脱敏与 A/B 层）。
- 卷宗：
  - T066 的汇报和 `notes.md`（`input.hint` 的修法）；
  - T102 的汇报和 `sanitize-t102.ts`；
  - T052（录制与脱敏的规则）；
  - SP-1（脱敏方法）。
- `docs/03-plan.md`：T204 一行。

## 注意

- 不运行真实的 Claude Code、Codex CLI；不读、不改 `~/.claude`、`~/.codex`。
- 需要知道用户的 skill 名或 MCP 服务器名时，从录制本身取，不去读用户的配置。汇报和 notes 里不写出这些名字。
- 不在本机造高负载。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
