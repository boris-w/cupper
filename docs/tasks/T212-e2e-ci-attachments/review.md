# T212 评审

- 评审人：编排者
- 日期：2026-11-02
- 结论：**通过，合并**（PR #27）

## 核对

- **规模**：4 个文件，+25 −13，共 38 行，停止线 60 以内（不算已 revert 的临时提交）。
- **CI**：
  - 带临时失败用例的那次：https://github.com/boris-w/cupper/actions/runs/41379082863 （1570787）。e2e 按计划失败，构件 `e2e-macos-15-playwright` 里 `rest/`、`focus/` 两轮的附件都在；两轮的 JSON 报告里失败的只有两个临时用例。其余 7 个检查通过。
  - revert 之后：PR #27 https://github.com/boris-w/cupper/pull/27 ，1fb499a 上 8 个检查全绿（run 41382410627）。
- **个人信息**：改动的 4 个文件过了敏感词清单和通用扫描，都是干净的。

## 逐项意见

1. **改法最小**：两轮各传一个 `--output`，Playwright 各清各的目录，后一轮不再删前一轮的附件。路径相对 `tools/e2e`，执行者核对过 pnpm 的工作目录和 Playwright 的解析方式。
2. **验证到位**：用临时失败用例在 runner 上实测，确认两轮的附件都进了构件，再 revert。最终 diff 里没有临时用例。
3. **范围**：`pnpm test:e2e` 是同样的两轮结构，一起改了，只多一行，编排者同意。e2e-repeat 只跑一轮，核对后不改。`--record` 的摘要本来就不记附件路径，不改代码，理由成立。

## 记下的点

- e2e 的 Electron 用例没有开 Playwright 的 screenshot 和 trace，失败时的附件只有 fixture 挂上的日志和 `error-context.md`。这次不加；以后查 e2e 偶发失败时如果缺截图，再单独开任务。
