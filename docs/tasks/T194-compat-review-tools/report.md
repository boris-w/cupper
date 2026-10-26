# T194 汇报

- 执行者：子 agent（工程师）
- 分支：`feat/T194-compat-review-tools`
- 完成日期：2026-10-26

T194 已完成。两件工具都写好了：`tools/rec-shape-diff`，以及 `trace-ids --compat`（连同 `ids.toml` 里 COMPAT 编号的登记）。UNIT-rec-shape-diff 和 UNIT-trace-ids-compat 都通过，并且由 `ts` 作业运行。

在干净的 worktree 里，`cargo xtask ci pr --record` 一次通过，摘要已提交。规模：`cargo xtask size` 为 +483 −15，停止线 560。这次每写完一个模块统计了一次，数字记在 `notes.md`。

T187 已在 main 上，还没有合进本分支；试合并没有冲突。报告新加的三种结果取值，本分支已经照你的要求处理。

## 做了什么

**`tools/rec-shape-diff`**（TS，结构同 `tools/sanitize`、`tools/schema-compat`）
- 用法：`node tools/rec-shape-diff/src/main.ts [--json] <旧录制> <新录制>`。
- 每类形态是一个字符串集合，共六类：
  - 方法，带方向：`send` 或 `recv`；
  - `session/update` 的 `sessionUpdate` 种类；
  - 每个 `_meta` 里的键路径，带所在的方法和 params、result、error，数组记成 `[]`，只记键、不记值；
  - 权限选项的 `optionId`，带 kind；
  - prompt 结果的 `stopReason`；
  - 错误码，带它所应答的方法。
- 应答按 `instance`、方向和 `id` 对回请求的方法；`note`、stderr 和非 JSON 行都跳过。文本、时间和 ID 不进入形态。
- 输出：默认是给人看的摘要，列出每类的增（`+`）和删（`-`）；加 `--json` 输出机器可读的报告。
- 退出码：没有差异 0，有差异 1，用法错误 2。

**`trace-ids --compat <报告>`**（可重复）
- 登记：`tests/ids.toml` 里的 6 个 COMPAT 编号改成内联表，写明剧本和 CLI，表头的格式说明补了这两个字段：

  | 编号 | 剧本 | CLI |
  |---|---|---|
  | COMPAT-claude-thinking | thinking | claude |
  | COMPAT-claude-six-sessions | six-sessions | claude |
  | COMPAT-claude-close-running | close-running | claude |
  | COMPAT-claude-resume-procs | resume-procs | claude |
  | COMPAT-untrusted-project-claude | untrusted-project | claude |
  | COMPAT-untrusted-project-codex | untrusted-project | codex |

- 三条登记规则，由 `trace-ids check` 检查：
  - `scenario`、`cli` 只能写在 COMPAT 编号上；
  - 套件为 compat 的 COMPAT 编号必须写这两个字段，`cli` 只能是 claude 或 codex；
  - 同一个 `(cli, scenario)` 只能对应一个编号。
- 读报告时只取需要的字段：`v`、`mode`、`cli.name`、`scenarios[].id`、`scenarios[].outcome`，其余忽略。
- 结果怎么算：
  - `pass` 计为通过，`fail` 计为失败；
  - `inconclusive`、T187 新加的 `not_applicable`、`observed`、`real_only`，以及任何不认识的取值，都记作跳过，不让编号完成；判定为 problem，说明里写"completes nothing"；
  - `inconclusive` 另外提示重跑。
- 假模式的报告直接报错；不是两种真实模式之一的报告也报错。
- 没有登记编号的剧本单独列一节"compatibility scenarios without a COMPAT ID"，只提示，不判失败。
- `--milestone` 可以同时给 `--compat`，前提是报告已提交且未改动；其余规则和 `--summary` 一样。

## 关键取舍

**照 T188 的做法做的部分**：六类形态、应答对回请求的方式、登记的三条规则、结果的映射、假模式报错、未登记剧本单独列一节、`--milestone` 放行已提交的报告。

**改动或补充的部分**：
- 摘要和机器可读报告用一个 `--json` 开关切换，而不是同时写两份文件。
- 模式只接受两种真实模式，不只是拦下假模式。这样以后加了新模式，也不会被当作真实 CLI 的结果。
- `--milestone` 不要求兼容报告的提交与摘要的提交相同，因为兼容报告是在另一次运行里写的。
- `not_applicable` 的处理：交接里说"不适用按通过算"，你的消息说三种都不让编号完成。我按你的消息做：都记作跳过，显示为 problem。

## 验证方式

- **UNIT-rec-shape-diff**（2 条）：
  - 两份录制只有文本、时间、ID、会话不同时，没有差异；
  - 六类各有一个差异用例：加一个方法、加一种 update、加一条带嵌套和数组的 `_meta` 键路径、换一个选项、换 `stopReason`、加一个应答为错误的请求。
- **UNIT-trace-ids-compat**（4 条）：
  - 登记规则：非 COMPAT 编号写了字段；compat 套件的编号漏写；`cli` 写了不认识的值；同一对 `(cli, scenario)` 重复。
  - 通过的剧本让编号完成，失败的不算，`inconclusive` 显示为 problem 并提示重跑。同名剧本只证明对应 CLI 的那个编号。
  - `not_applicable`、`observed`、`real_only` 和不认识的取值，都不让编号完成。
  - 假模式和格式不对的报告报错；未登记的剧本单独列出，不产生 problem。
- **手动试过**：
  - `rec-shape-diff` 对同一份 SP-1 录制输出"the same shape"，退出码 0；对两份不同的录制列出差异，退出码 1。
  - `trace-ids status --compat` 用一份真实模式的样例报告和一份假模式的报告试过，结果同上面的用例一致。
- **接入**：两个包都带 `test` 脚本，`ts` 作业自动运行，摘要里 UNIT-rec-shape-diff 和 UNIT-trace-ids-compat 都是 pass。ESLint、prettier、`tsc --build` 都通过；`trace-ids check` 报"418 IDs, consistent with the docs"。
- **CI**：机器负载降到 10 以下后，在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T194-compat-review-tools/ci/`，一次通过，摘要 `pr-65feaf1592e5.json` 已提交。跑的期间没有往工作区写文件。

  | 作业 | 结果 |
  |---|---|
  | rust | 2,172 通过 |
  | protocol | 188 通过 |
  | ts | 489 通过 |
  | license | 5 通过 |
  | testdata-scan | 4 通过（本机没有敏感词表，那一步跳过） |
  | compat-framework | 按路径筛选跳过 |
  | ui | 35 通过 |
  | e2e | 30 通过 |

- **试合并**：用 `git merge-tree` 试合并最新的 main（含 T187），没有冲突。main 上没有新增 COMPAT 编号，所以合并后 `trace-ids check` 不会出现漏写字段的编号。
- **规模**（逐模块，累计新增）：

  | 模块 | 新增 | 估计 | 累计 |
  |---|---|---|---|
  | 包与形态提取 | 129 | 120 | 129 |
  | 命令行 | 36 | 35 | 165 |
  | UNIT-rec-shape-diff | 110 | 90 | 275 |
  | 登记字段与 `ids.toml` | 33 | 36 | 308 |
  | 读报告与 `--compat` | 76 | 70 | 384 |
  | UNIT-trace-ids-compat | 99 | 90 | 483 |

## 提交列表

```
b89caad docs(T194): record the ci pr summary
65feaf1 docs(T194): record the size per module and how the work follows T188's plan
eb84ebf feat(trace-ids): --compat derives the COMPAT IDs from compatibility reports
c26609b feat(tools): rec-shape-diff compares the shape of two recordings
b98639f docs(T194): record the checks, the approach and the estimate
```

## 遗留问题 / 范围外发现

- **剧本表还没有对应条目**：COMPAT 编号登记的剧本 six-sessions、close-running、resume-procs、untrusted-project，在 main 的剧本表里还没有，要由 T192 补上。补上之前，报告里不会出现这几个剧本，对应编号保持待完成。
- **`rec-shape-diff` 的边界**：`_meta` 里如果有以 ID 为键的映射，键路径会随 ID 变，被当成形态差异。现有录制里没有见到这种情况；以后遇到，可以把这类键规范成占位符。
- **`not_applicable` 的口径**：交接说"按通过算"，你的消息说不让编号完成，本任务按你的消息做了。T188 写的"计入已验证范围"那一侧（Rust 的 `compat-reports`）是否也按同一口径，需要你确认。
