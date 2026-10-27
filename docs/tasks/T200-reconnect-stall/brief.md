# T200 窗口重连时主进程卡住

- 派发日期：2026-10-27
- 执行者：子 agent（工程师）。T016、T190、T194、T195、T193 的执行者接着做（T195 改过主进程的 `index.ts` 和 e2e 的对话框替身）
- 分支：`fix/T200-reconnect-stall`
- 依赖：T199。T099 依赖本任务：退出验收要求 nfr1 组同一提交上连续 20 次全绿

## 背景

T199 在 GitHub 的 macOS runner 上查明：E2E-NFR1-4（"a window whose connection dropped resumes from its cursor"）大约每 100 次失败 1 次，是产品问题。证据、日志与采样栈都在 `docs/tasks/T199-timing-tests/notes.md` 的"交接给 T200"一节：
- 窗口的数据连接断开后，主进程不再连宿主，也不再写日志；`app.evaluate` 挂住；
- 两次采样（run 40453156576 的 repeat72、repeat104）都显示 Electron 主线程停在原生模态对话框里（`-[NSAlert runModal]`），对话框是从 libuv 的 async 回调里经 JS 打开的；
- 主进程没有 `uncaughtException`、`unhandledRejection` 的处理，Electron 的默认做法是弹出同步的错误框；e2e 的对话框替身没有替换它，所以它真的弹出来，挡住了主线程；
- 推测：断开之后、重连之前，有一处异常或 rejection 没被处理，抛出点还没找到。

用户那里同样会发生：窗口断线时弹出"主进程出了 JavaScript 错误"，App 卡住。

## 范围

**做**：
1. **找到抛出点**：先在主进程装上 `uncaughtException`、`unhandledRejection` 的处理，把错误和栈记进 main.log、不弹框；然后请编排者用 `e2e-repeat` 在 runner 上跑 E2E-NFR1-4 × 200，从日志里拿到抛出点。
2. **修根因**：让抛出的那处正确处理断开与重连，单处不超过 100 行、不改接口（计划 1.2），超出就停下汇报。
3. **全局处理的长期行为**：对照设计 07-desktop（5.7.x）和 11-diagnostics 里关于主进程错误、崩溃、诊断的规定，定下主进程遇到未处理的异常时做什么：记日志、上报诊断、继续运行还是退出重启。设计里没写的，先提方案、写进汇报，由编排者定，不要自己定。不论哪种，都不要再让同步的错误框挡住主线程。
4. **e2e 的对话框替身**：替换 `dialog.showErrorBox`，被调用时让用例失败并报出内容。这样以后任何错误框都不会再挡住测试，而是直接暴露。
5. **验证**：修后请编排者在 runner 上用 `e2e-repeat` 跑 E2E-NFR1-4 × 300，没有失败；PR 上的 e2e 冒烟照常通过。

**不做**：改宿主或协议。在本机制造高负载（`CLAUDE.md` 守则第 10 条），所有重复运行都在 runner 上做。

## 验收标准

- [ ] 抛出点、根因、修法写进本目录的 `notes.md`，附运行链接。
- [ ] runner 上 E2E-NFR1-4 × 300 没有失败。
- [ ] 主进程遇到未处理异常时不再弹同步错误框；长期行为的方案经编排者确认后实现，有用例。
- [ ] e2e 替身接管 `showErrorBox`，有用例。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 交接事项

- T199：证据、采样栈、日志的位置；诊断代码（失败时先附日志、`disconnectWindow` 的注解与探测、`sample` 采样、主进程的建连日志）。
- T195：主进程的 e2e 后台开关；对话框替身（`e2e-dialogs.ts`）。

## 注意

- 主进程在调用时才取 `dialog.showMessageBox` 这类函数，不在模块加载时解构保存（代码约束）。
- 界面文案一律走 `@cupper/i18n`。

## 规模

- 停止线：200；硬停止线：240

<!-- 执行者开工前在 `notes.md` 里写估计；每写完一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b fix/T200-reconnect-stall main`。要在 GitHub 上跑时照守则第 9 条请编排者推送和触发。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
