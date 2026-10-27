# T200 汇报

- 执行者：子 agent（工程师）
- 分支：`fix/T200-reconnect-stall`（T052 worktree），PR #12
- 完成日期：2026-10-27

**结论：卡住的根因在 e2e 钩子，不在产品代码；修好后 runner 上 E2E-NFR1-4 跑 300 次全部通过，PR #12 的 8 个检查全绿。** 任务书里"用户那里同样会发生"这条推测不成立，但全局异常处理照样有必要，已按你定的方案 A 实现。

## 做了什么

1. **全局处理（8f01e95，新文件 `apps/desktop/src/main/errors.ts`）**
   - 主进程在 `main.log` 打开后，立即监听 `uncaughtException` 和 `unhandledRejection`。
   - 收到时用 `util.inspect` 把错误连同栈、`cause`、`code` 记进 main.log，不弹框。
   - Electron 自带的处理见到还有别的监听，就不再弹 `showErrorBox`。
   - 长期行为按你定的 **A：记日志，继续运行**。
2. **e2e 替身与夹具（b16c57f）**
   - `e2e-dialogs.ts` 接管 `dialog.showErrorBox`：调用后立即返回并记录下来。
   - 夹具收尾时，用例没认领的错误框，以及 main.log 里用例没认领的未处理异常，都会让用例失败，失败信息带内容和栈。
   - 收尾查出问题时也附上日志。用例可以用 `cupper.dialogs.errorBoxes()` 和 `cupper.uncaught()` 认领。
   - 新增两个 helpers 用例：
     - 错误框不阻塞、被记录，用例可以认领；
     - 主进程抛出未捕获异常、出现未处理的 rejection 时，都记进日志，不弹框，App 照常应答。
3. **找到抛出点**
   - 诊断运行：https://github.com/boris-w/cupper/actions/runs/40459463296 ，跑 200 次失败 2 次。两次的用例体都通过了，窗口也重连上了，只是夹具报了未处理异常。
   - 错误是 `RangeError [ERR_OUT_OF_RANGE]: The value of "err" is out of range. It must be a negative integer. Received 599`（另一次是 919），栈为 `getSystemErrorName` ← `new ErrnoException` ← `Pipe.onStreamRead`，时间正好落在断开的那一刻。
4. **根因**
   - Electron 44.9.0 自带 Node 24.22.0。Node 的 `onStreamRead` 先读出这次读到的字节数，再判断 `nread > 0 && !stream.destroyed`。套接字如果在这两步之间被销毁，就会走到错误分支，把字节数当成错误码，于是抛出 `RangeError`。
   - Playwright 的 `electronApp.evaluate` 经 inspector 进入主进程，inspector 会用 V8 中断插进正在运行的 JavaScript。e2e 钩子 `disconnectWindow` 就这样在 flood 期间插进了某次 `onStreamRead` 的半途，同步 `destroy()` 了它正在读的套接字。
   - 本机复现（材料在 `docs/tasks/T200-reconnect-stall/repro/`，单进程，不造高负载）：经 inspector 断开 400 次，抛出 105 到 130 次；改成在 `setImmediate` 里断开，0 次。
   - 产品代码只在自己的事件回调里关连接，正式构建里也没有 inspector，所以走不到这条路。
5. **修法（9fa8f07）**：钩子改用 `setImmediate`，推到下一轮再断开，只改了 3 行，不改接口。
6. **你的合并（81137e3）已核对，没有问题**：失败的用例由 T199 在退出 App 之前附日志；末尾那一处只在"用例本身通过、收尾时才查出问题"时附，不会附两遍。我的 `setImmediate` 修改和 T199 的建连日志、`disconnected` 注解也都保留了。

## 关键取舍

- **修在钩子里，不改 Node 也不改窗口桥**：问题只出在"evaluate 重入时销毁套接字"这一条路，推迟到下一轮执行就能避开 Node 的这个缺陷，所有调用钩子的地方都受益。
- **夹具对任何未处理异常都判失败**：这次能拿到抛出点靠的就是它；以后再出现，会直接暴露出来，不会悄悄漏过。

## 验证方式

- **本机（单次）**：
  - `pnpm --filter @cupper/desktop test` 96 个通过；
  - 修复后重新 `build:e2e`，`test:e2e:one` 跑 E2E-NFR1-4、helpers 的断开用例和两个新用例，4 个都通过；
  - 反向检查：去掉认领后，夹具分别报 `an error box: title: content` 和 `main process: unhandled rejection …; main process: uncaught exception …`。
- **runner 上 × 300**：https://github.com/boris-w/cupper/actions/runs/40464509695 ，提交 81137e3，300 个全部通过，用时 50.6 分钟。
- **PR 的 CI**：https://github.com/boris-w/cupper/pull/12 ，运行 https://github.com/boris-w/cupper/actions/runs/40464511746 ，8 个检查全部通过，包括 e2e 冒烟。
- **规模**：+122 −7，停止线 200。

## 提交列表

```
633ff05 docs(T200): record the 300 runs on the runner and the PR's checks
81137e3 Merge branch 'main' into fix/T200-reconnect-stall
1c61529 docs(T200): record the throw site, the root cause, the fix and the decision
3f16b06 docs(T200): keep the inspector repro of the RangeError
9fa8f07 fix(desktop): the e2e hook drops a window's connection on a turn of its own
41576b5 docs(T200): record the sizes, the local checks and the options for the long run
b16c57f test(e2e): an error box or an error nothing caught fails the test
8f01e95 fix(desktop): an error nothing caught in the main process is logged, not boxed
7ed45d8 docs(T200): record the checks, the plan and the estimate
```

## 遗留问题 / 范围外发现

- **诊断包还收不到 main.log**：`diag/export` 属于 M2，还没实现，宿主对它返回 `unsupported`。按设计 5.11.3，诊断包收的是 5.11.1 表里的全部日志，其中有 main.log，它和 host.log 在同一个日志目录，所以照设计实现时就会收进去。另外，main.log 目前只把家目录改写成 `~`，未处理异常的栈里可能带别的内容，完整脱敏要到诊断包导出时才做（5.11.2）。
- **其他 `app.evaluate` 也会重入**：任何 `app.evaluate` 都可能在主进程正在运行的 JavaScript 半途插入执行。以后加 e2e 钩子时，凡是会改动 I/O 状态的（关闭套接字、销毁窗口之类），都应该推到下一轮再做。现有的对话框应答、`powerMonitor.emit("shutdown")` 我没有改。
