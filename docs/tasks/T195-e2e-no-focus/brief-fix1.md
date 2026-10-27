# T195 修复 1：E2E-single-instance 要标 `@focus`

- 派发日期：2026-10-27
- 执行者：子 agent（工程师）。T093、T189、T197 的执行者
- 分支：`fix/T195-single-instance-focus`
- 依赖：T195（已合并）

## 背景

T195 让不带 `@focus` 的 e2e 用例在后台启动，并用前台采样检查它们不抢前台。10-26 夜里的 nightly（https://github.com/boris-w/cupper/actions/runs/40412822285 ）在 e2e 全集里抓到一条：

`src/app.spec.ts:14 › E2E-single-instance: a second launch quits and brings the first window forward` 报 `the app took the foreground in 4 of 15 samples; it needs @focus`。

这条用例测的就是"第二次启动把第一个窗口带到前面"，本身就要激活 App，却没有标 `@focus`。PR 上只跑冒烟子集，所以 T195 的 PR 没碰到。检查是对的，要改的是用例的标签。

## 范围

**做**：
1. 给 E2E-single-instance 加 `@focus`。
2. 同类排查：查一遍 e2e 全集里不带 `@focus`、却会走到 `showMainWindow()` 的 `show()`（`second-instance`、`activate`）或别的激活路径的用例，一并处理。要逐条说明是加 `@focus`，还是用例本不该激活。
3. 用 `e2e-repeat` 在 runner 上跑一次 `src/app.spec.ts` 全部用例作为核对：请编排者推送并触发。

**不做**：改主进程的激活逻辑；改前台检查。

## 验收标准

- [ ] E2E-single-instance 在 `@focus` 段里通过，后台段不再有前台命中。
- [ ] 同类排查的结论写进 `docs/tasks/T195-e2e-no-focus/notes.md`。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：60；硬停止线：80

## 工作方式

在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b fix/T195-single-instance-focus main`。要在 GitHub 上跑时照守则第 9 条请编排者推送。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report-fix1.md`。汇报里不要写本机的绝对路径。
