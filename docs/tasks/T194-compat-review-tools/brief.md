# T194 兼容套件 V：审阅工具

- 派发日期：2026-10-26
- 执行者：子 agent（工程师）。T016、T190 的执行者接着做（`tools/schema-compat` 是它写的）
- 分支：`feat/T194-compat-review-tools`
- 依赖：T064（已合并）。T066 依赖本任务

## 背景

兼容套件的审阅有两件工具（设计 5.9.5、5.9.10），计划 v1.3.62 由 T188 拆出：
- 升级适配器时，评审看录制的形态差异报告，不看事件全文；
- COMPAT 编号要从兼容报告推导完成状态，在那之前一直是待完成（T002 的交接）。

T188 的执行者开工前已经把两件的做法想好了，写在它分支上的 `notes.md` 里（`git show feat/T188-compat-tools:docs/tasks/T188-compat-tools/notes.md`）。本任务照那份做法实现，有更好的做法可以改，在汇报里说明。

## 范围

**做**：
1. **`tools/rec-shape-diff`**（TS，与 `tools/sanitize`、`tools/schema-compat` 同样的结构）：
   - 对比两份录制里的方法、update 种类、`_meta` 键路径、`option_id`、`stopReason`、错误码；
   - 输出形态差异报告（机器可读，再加一份给人看的摘要），没有差异时退出码 0；
   - 只比形态，不比文本内容、时间和 ID。
2. **`tools/trace-ids --compat <报告>`**（可重复）：
   - `tests/ids.toml` 里给 COMPAT 编号登记 `scenario`、`cli` 两个字段：只能写在 COMPAT 编号上；套件为 compat 的 COMPAT 编号必须写；同一个 `(cli, scenario)` 只能对应一个编号；
   - 剧本编号以 T187 定下的 21 个为准：
     - thinking → COMPAT-claude-thinking；
     - six-sessions、close-running、resume-procs → 对应的 `COMPAT-claude-*`；
     - untrusted-project → COMPAT-untrusted-project-claude、COMPAT-untrusted-project-codex；
   - 通过的剧本让对应编号完成；失败的不算；`inconclusive` 记作跳过，显示为 problem 并提示重跑；
   - 假模式的报告直接报错：假 agent 证明不了 COMPAT 编号；
   - 没有登记的剧本单独列一节提示，不判失败，因为基本组的大部分剧本本来就没有 COMPAT 编号；
   - `--milestone` 允许同时给 `--compat`，前提是报告已提交且未改动（与 `--summary` 同样的规则）。
3. 报告的读取在 TS 里按 `testdata/compat/report.schema.json` 的形状来，只读需要的字段，其余忽略。

**不做**：报告的严格校验与已验证范围（T188）；真实模式与剧本（T187、T192）。

## 验收标准

- [ ] UNIT-rec-shape-diff：相同形态时没有差异；六类差异各有用例；文本、时间、ID 不同不算差异。
- [ ] UNIT-trace-ids-compat：
  - 登记规则：字段只能写在 COMPAT 编号上、compat 套件的 COMPAT 编号必须写、`(cli, scenario)` 不重复；
  - 通过的剧本让编号完成，失败的不算，`inconclusive` 显示为 problem，假模式的报告报错，没有登记的剧本单独提示。
- [ ] 两件工具接进 `ts` 作业的测试（照 `tools/sanitize` 的接法）。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T194 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T194-compat-review-tools/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T188 或 T194 的行）

- T002：`trace-ids` 的 `--compat`。
- T188：做法写在它的 `notes.md` 里（见背景）。
- T187 同时在做，它会给报告加"不适用""观察"一类的结果取值。TS 读报告时，不认识的取值不要当作通过；"不适用"按通过算、"观察"不算，两者都不让编号完成。

## 注意

- 测试夹具里的报告与录制不含个人信息。
- 不运行真实的 Claude Code、Codex CLI。

## 规模

- 停止线：560；硬停止线：640

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项；每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b feat/T194-compat-review-tools main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
