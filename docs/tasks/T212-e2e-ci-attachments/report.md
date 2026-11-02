# T212 汇报

- 执行者：子 agent（工程师）
- 分支：`ci/T212-e2e-ci-attachments`
- 完成日期：2026-11-02

**还剩一项没核对**：revert 提交 `1fb499a` 已经推到 PR #27，但交汇报时那一轮的 8 个检查还没跑完。请合并前看一下 `gh pr checks 27`。这一步改的只是删掉临时 spec；不带临时用例时，上一轮除 e2e 外的 7 个检查都已通过。

## 做了什么

1. **xtask 给两轮各开一个输出目录**（`crates/xtask/src/ci/jobs/e2e.rs`）：`test-e2e` 加 `--output=dist/test-results/rest`，`test-e2e-focus` 加 `--output=dist/test-results/focus`。路径相对 `tools/e2e`，这正是 `pnpm --dir tools/e2e exec` 的工作目录，Playwright 也按这个目录解析 `--output`。
2. **失败时上传两轮的附件**（`.github/actions/xtask/action.yml`）：`-playwright` 构件改为上传 `tools/e2e/dist/test-results/rest/`、`tools/e2e/dist/test-results/focus/`，以及原来的 `tools/ui-screenshots/dist/test-results/`。`ci`、`nightly`、`cupper-sys` 三个 workflow 都用这个 action，一起生效。
3. **e2e-repeat**：核对过，不用改。它只跑一轮（`test:e2e:one`），用配置默认的 `dist/test-results`，上传的就是这个目录。
4. **`pnpm test:e2e`**（`tools/e2e/package.json`）：本机脚本的两轮也按同样的目录分开；`playwright.config.ts` 的注释里写了为什么要分开。这一条你已经同意保留。
5. **`--record` 的摘要**：不用改代码。摘要的字段白名单里本来就没有附件路径，计划 1.8 也规定"摘要的内容不变"。真正写下附件位置的是 `.private/ci-logs/<run>/` 里每轮的 JSON 报告和步骤日志，它们由 Playwright 写，现在自动指向 `rest/`、`focus/`。CI 上已经实际看到，见下文。

## 关键取舍

- 构件路径写两个子目录，没写父目录 `tools/e2e/dist/test-results/`：写父目录也能把两轮都传上去，写子目录是为了一看就知道有两轮。你已同意。
- 子目录名用任务书举例的 `rest`、`focus`，没用步骤名 `test-e2e`、`test-e2e-focus`。步骤名能和 ci-logs 里的文件名一一对应、代码也少几行，但 `rest`、`focus` 更好读。
- `pnpm test:e2e` 一起改了：两轮的结构一样，丢附件的问题也一样，只改一行。你已同意。

## 验证方式

- 本机：`cargo fmt --all --check`、`cargo clippy -p xtask --all-targets --locked -- -D warnings`、`cargo test -p xtask --locked`（62 个测试）都通过；改动的 TS、JSON、YAML 文件过了 `prettier --check`；临时 spec 过了 eslint 和 tsc。另外在 `tools/e2e` 下用临时配置做了一次核对：两个必然失败、不启动 App、不带 `@focus` 的用例，分两轮各用自己的 `--output` 跑，第二轮跑完后第一轮的附件还在，JSON 报告里的附件路径分别指向 `rest/` 和 `focus/`。这些临时文件都已删除。
- GitHub，带临时失败用例的那次运行：PR #27（https://github.com/boris-w/cupper/pull/27），提交 `1570787`，运行 https://github.com/boris-w/cupper/actions/runs/41379082863 ，e2e 作业 https://github.com/boris-w/cupper/actions/runs/41379082863/job/125065696806 。
  - 构件 `e2e-macos-15-playwright`（https://github.com/boris-w/cupper/actions/runs/41379082863/artifacts/13112736655）里，`e2e/dist/test-results/rest/` 和 `e2e/dist/test-results/focus/` 下各有对应临时用例的 `logs/main.log`、`data/logs/host.log`、`data/logs/adapter-claude.log` 和 `error-context.md`。adapter-claude.log 是空的，因为用例没有开会话；error-context.md 里就是那条故意失败的断言。
  - 只有这两个临时用例失败：rest 轮 18 条里只有临时用例 `unexpected`，focus 轮 11 条里也只有临时用例失败，两轮的 JSON 报告都没有报告级错误。摘要记的是 e2e 通过 31、失败 2。其余 7 个检查都通过。
  - 步骤日志里能看到 xtask 传出的 `--output=dist/test-results/rest` 和 `--output=dist/test-results/focus`；list reporter 打出的附件路径也都在这两个目录下。

验收标准对照：
- [x] 两轮的附件都留在构件里，临时用例那次运行的链接见上。
- [x] 最终 diff 里没有临时用例：`1fb499a` 把 `1570787` revert 掉了。分支相对 merge-base 的改动只剩 4 个文件：action.yml、e2e.rs、package.json、playwright.config.ts，+25 −13。
- [ ] PR 上 8 个检查全部通过：`1fb499a` 已推送，交汇报时还在跑，请合并前确认。PR 是 https://github.com/boris-w/cupper/pull/27 。

## 提交列表

```
1fb499a Revert "test(e2e): temporary, a failing test in each of the two runs"
1570787 test(e2e): temporary, a failing test in each of the two runs
b0f013d test(e2e): `pnpm test:e2e`'s two runs keep a folder each too
b711a09 ci(e2e): the two runs of `xtask ci e2e` keep their attachments in a folder each
```

规模：+25 −13，共 38 行。停止线是 60。

## 遗留问题 / 范围外发现

- e2e 的 Electron 用例没开 Playwright 的 screenshot 和 trace，所以失败用例的附件只有 fixture 挂上的日志和 `error-context.md`。你已经定了：不在本任务范围内。
- 下载的构件放在本机一个临时目录里，没有进 worktree，也没有提交。
