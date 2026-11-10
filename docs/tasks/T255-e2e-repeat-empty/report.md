# T255 汇报

- 执行者：子 agent（CI）
- 分支：`ci/T255-e2e-repeat-empty`
- 完成日期：2026-11-10
- PR：https://github.com/boris-w/cupper/pull/104
- CI：https://github.com/boris-w/cupper/actions/runs/42767132966 。`6b379d70` 上 8 个检查全部通过。
- 确认运行：
  - 匹配不到用例：https://github.com/boris-w/cupper/actions/runs/42767132247 （判失败，符合预期）
  - 正常参数：https://github.com/boris-w/cupper/actions/runs/42767142972 （判成功，符合预期）

## 做了什么

只改了 `.github/workflows/e2e-repeat.yml` 的 Run 一步，+20 行：
- **计数**：两组跑完、后台的负载进程结束之后，小函数 `ran` 用 `node` 读 `repeat-<组>.json` 的 `stats`，计算 `expected + unexpected + flaky`，也就是实际跑过的用例数，每次重复都算一次，跳过的不算。报告不存在或读不出来时，记为 0。
- **摘要**：两组各自的条数写进作业摘要。
- **判定**：
  - 合计为 0 时，打一条 `::error::No test ran: the spec and the test name match none`，作业失败；
  - 不为 0 时，照旧按两组的退出码判定。
- 两组的 `--pass-with-no-tests` 保留，因为本来就可能有一组是空的。

## 关键取舍

- **判定写在 workflow 里**：照 T247 给 test-repeat 加判定的做法，没有另写脚本。逻辑只有几行，放在运行它的地方最直观。
- **不回显输入**：新加的 `::error::` 和摘要里只有固定文字和数字，不回显 `spec`、`test`，免得输入里带换行时被当作 workflow 命令。输入照旧只经环境变量进 shell，没有新增注入面。
- **"跑过"的口径**：重复的每一次都算；跳过的不算，因为跳过的用例并没有执行。

## 验证方式

- **确认运行 1**，匹配不到用例（42767132247，`spec=src/composer.spec.ts`，`test` 用默认值 `E2E-SEC-real-input`，即 T149 那次的组合）：
  - 结论 failure，日志里有 `##[error]No test ran: the spec and the test name match none`；
  - 两组的报告都是 `expected 0、unexpected 0、flaky 0`，摘要里两组都应为 0。
- **确认运行 2**，正常参数（42767142972，`test=E2E-slash-commands`，`times=2`）：
  - 结论 success，Playwright 输出 `2 passed`；
  - 报告：不带 `@focus` 的一组 `expected 2`，带 `@focus` 的一组 0，摘要里应为"not tagged @focus: 2"和"tagged @focus: 0"。
- **关于摘要的核对方式**：作业摘要用只读的 `gh` 取不到，checks API 里 `output.summary` 是空的。所以我下载了两次运行的 `e2e-repeat` 产物，用和 workflow 里相同的计算重新算了一遍，得到上面的数字。你在 GitHub 页面上看到的摘要应当与此一致。
- **本机**：
  - 用 ruby 解析 workflow 的 YAML，取出 Run 一步的脚本，`bash -n` 语法检查通过；prettier 通过。
  - 计数函数用造的报告试过：`expected 3、unexpected 1、flaky 2、skipped 4` 算出 6，报告不存在时算出 0。
- **对照验收标准**：
  - 两次确认运行的结果如上，链接见上。✓
  - 输入只经环境变量进 shell，没有新增注入面。✓
  - PR 上 8 个检查全部通过。✓

## 规模

+20 −0，停止线 40。

## 提交列表

```
6b379d70 ci: fail e2e-repeat when no test ran
```

## 遗留问题 / 范围外发现

- 按任务书没有改 `test` 的默认值。今后 `spec` 换了、`test` 忘了改时，作业会直接失败并说明原因，不会再静默判为通过。
- 本机做了几处临时文件和下载，都已删掉；worktree 保留着。T252 等你的消息再开始。
