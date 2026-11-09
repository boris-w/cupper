# T247 test-repeat 认不出带颜色的 vitest 输出

- 派发日期：2026-11-09
- 执行者：子 agent（工程师，CI）
- 分支：`ci/T247-test-repeat-vitest-color`
- 依赖：无

## 背景

test-repeat 用正则 `RAN='test result: ok\. [1-9][0-9]* passed|Tests +[1-9][0-9]* passed'` 判断"用例确实跑过并通过"。runner 上设了 `CI`，Vitest 因此输出带颜色，"Tests" 和数字之间夹着 ANSI 转义码，正则匹配不上，每次都记成 FAILED。T245 的 run 42589259180 里，40 份日志都是 `Tests 11 passed (11)`，汇总却是 0 passed、40 failed。之前的 test-repeat 都跑 cargo，这是第一次跑 vitest。

## 范围

**做**：
1. `.github/workflows/test-repeat.yml`：判定前先去掉 ANSI 转义码，或者在 vitest 的 Run 步骤关掉颜色（`NO_COLOR`）。二选一，取更稳的，理由写进 `notes.md`。cargo 那条路的行为不变。
2. 用 T245 那组参数重跑一次确认判为通过：vitest，`packages/ui-core`，`src/stream/stream.test.tsx`，filter `UNIT-stream`，10 次 × 4 份。再用一个不存在的 filter 跑 1 次，确认仍会判失败。两次都由 `main` 触发。

**不做**：test-repeat 的其他改动。

## 验收标准

- [ ] 上面两次运行一次判通过、一次判失败，汇报里写上链接。
- [ ] PR 上 GitHub CI 的 8 个检查都通过（守则第 9 条）。

## 规模

- 停止线：20；硬停止线：40。

## 参考资料

- 卷宗：T245（误报的发现过程）、T196（test-repeat 的来历）。

## 注意

- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
