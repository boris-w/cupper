# T200 工作笔记

## 开工前的核对

- 任务书；T199 `notes.md` 的"交接给 T200"（PR #11 未合并时从 `fix/T199-timing-tests` 读）。
- 采样（run 40453156576 repeat72）：主线程 `uv__io_poll` → `uv__async_io` → JS → 微任务 → `-[NSAlert runModal]`。Electron 自带的 `uncaughtException` 处理（只在没有别的监听时起作用）是 `import('electron').then(({ dialog }) => dialog.showErrorBox(...))`，在微任务里弹同步的错误框，与采样相符。未处理的 rejection 在没有 `unhandledRejection` 监听时按 Node 默认转成未捕获异常，走的是同一条路。
- 设计 07（5.7.2 主进程模块）和 11（5.11.4 崩溃信息）都没写主进程遇到未处理异常时怎么办：11 只写了宿主的 panic hook 和桌面端的 `crashReporter`（原生崩溃）。长期行为要先提方案，由编排者定。
- 读码没找到抛出点（与 T199 的结论一样）。值得留意的一处：主进程用 `new net.Socket({ fd })` 包装的数据连接和 control 连接都没有 `error` 监听，`net.Socket` 发出 `error` 时没有监听就会变成未捕获异常；但 e2e 的断开是主进程自己 `destroy()`，不会发出 `error`。等 runner 上的日志。

## 做法

1. 主进程一启动（`main.log` 打开后）就装 `uncaughtException`、`unhandledRejection` 的处理：用 `util.inspect` 把错误（含栈、`cause`、`code`）记进 `main.log`，不弹框。装上之后 Electron 自带的处理看到还有别的监听，就不再弹框。在长期行为定下之前，先照 Electron 的默认做法继续运行。
2. e2e：替身接管 `dialog.showErrorBox`，记下标题和内容；夹具收尾时，用例没认领的错误框和 `main.log` 里的未处理异常都让用例失败，并写出内容。这样 runner 上跑 × 200 时，抛出点直接写在失败信息里。
3. 拿到抛出点后修根因（单处不超过 100 行）。
4. 长期行为按编排者的决定实现，并写用例。

## 规模估计（开工前）

| 模块 | 估计 |
|---|---|
| 主进程的全局处理与单元测试 | 50 |
| 替身接管 `showErrorBox`；夹具对错误框和未处理异常报失败；e2e 用例 | 45 |
| 根因修复与测试 | 40（未知，上限 100） |
| 长期行为（待定） | 40 |
| 合计 | 约 **175** |

停止线 200，硬停止线 240。根因修复或长期行为一旦让推算超过 200，就停下汇报。

## 按模块统计

| 模块 | 提交 | `cargo xtask size` 累计 |
|---|---|---|
| 主进程的全局处理与单元测试 | 8f01e95 | +43 −0 |
| 替身接管 `showErrorBox`；夹具对没认领的错误框和未处理异常报失败；两个 helpers 用例 | b16c57f | +113 −6 |
| 根因：e2e 钩子推到下一轮再断开 | 9fa8f07 | +118 −8（净 +110） |

## 本机验证（单次）

- `pnpm --filter @cupper/desktop test`：96 个通过。
- `test:e2e:one src/helpers.spec.ts`（两个新用例）：通过。
- 反向检查（不提交）：
  - 去掉用例里对错误框的认领，夹具报 `an error box: title: content`；
  - 不认领日志里的两条未处理错误，夹具报 `main process: unhandled rejection: Error: rejected on purpose …; main process: uncaught exception: Error: thrown on purpose`，带栈。

## 抛出点与根因

**诊断运行**：https://github.com/boris-w/cupper/actions/runs/40459463296 ，提交 41576b5，E2E-NFR1-4 × 200。198 次通过，2 次失败（repeat97、repeat103）。这两次的用例体其实都过了，窗口也重连上了：装上全局处理后主线程不再被挡住，失败只来自夹具收尾时查到的未处理异常。失败信息里的错误和栈是：

```
RangeError [ERR_OUT_OF_RANGE]: The value of "err" is out of range. It must be a negative integer. Received 599
    at Object.getSystemErrorName (node:util:455:11)
    at new ErrnoException (node:internal/errors:734:23)
    at Pipe.onStreamRead (node:internal/stream_base_commons:221:20)
```

另一次是 `Received 919`。时间上，这个异常正好落在断开的那一刻：主进程记下异常是 06:31:47.617，宿主 13 ms 后才看到旧的窗口连接关闭，随后新连接 c_9 建立。

**根因**：
- Node 的 `onStreamRead`（Electron 44.9.0 自带 Node 24.22.0）先读出这次读到的字节数，再判断 `nread > 0 && !stream.destroyed`。如果这时套接字已经被销毁，就会落到错误分支，把字节数当成错误码去构造 `ErrnoException`，结果 `getSystemErrorName(599)` 抛出 `RangeError`。
- 套接字怎么会在这两步之间被销毁？Playwright 的 `electronApp.evaluate` 经 Node 的 inspector 到达主进程；inspector 收到消息时会请求 V8 中断，在当前正在运行的任何 JavaScript 里插进去执行。e2e 钩子 `disconnectWindow` 就这样插进了 flood 期间某次 `onStreamRead` 里，同步 `destroy()` 了它正在读的套接字。
- 产品代码不会走到这条路。主进程只在自己的事件回调里关连接：导航、渲染进程退出、窗口关闭、窗口端口的 `close` 消息，以及握手失败；握手失败的后续处理在 `onStreamRead` 返回之后的微任务里执行。这些都不在一次读的中途，正式构建里也没有 inspector。所以任务书里"用户那里同样会发生"这条推测不成立：这个错误框只是 e2e 钩子带出来的。不过任何未捕获的错误都不应该挡住主线程，第 1 步的全局处理照样有必要。
- 采样里主线程停在 `uv__async_io` → JS → 微任务 → `NSAlert runModal`，也对得上：Electron 自带的处理用 `import('electron').then(...)` 在微任务里弹 `showErrorBox`，而 inspector 的消息和 Node 的中断任务都经 `uv_async` 投递到主线程。

**本机复现**（`repro/`，单进程、不造高负载）：
- 一个被自己的服务端 flood 的 Unix 套接字，经 inspector 的 `Runtime.evaluate` 调 400 次 `destroy()`：抛出 `RangeError` 105 次（再跑一次是 130 次）。
- 改成 `setImmediate` 里再 `destroy()`：0 次。

**修法**（9fa8f07）：主进程的 e2e 钩子 `disconnectWindow` 改成用 `setImmediate` 推到下一轮再断开，这样套接字只会在两次读之间关闭。只改了 3 行，不改接口。

**本机单次验证**：重新 `build:e2e` 后，`test:e2e:one` 跑 E2E-NFR1-4、helpers 的断开用例和两个新用例，4 个都通过。

## 诊断包是否收进 main.log

`diag/export`（M2）还没实现，宿主对它返回 `unsupported`（T033 的 UNIT-unsupported-set）。设计 5.11.3 的诊断包"日志"一项写的是 5.11.1 表里的全部日志文件，其中就有 `main.log`，而且它和 `host.log` 在同一个日志目录。所以按设计实现时就会收进去；现在没有东西可核对，只记进汇报，不扩范围。

## 长期行为：定为方案 A

设计 07、11 都没写，先提了下面三个方案。编排者定为 **A：记日志，继续运行**，并会写进设计 07。用例是 helpers 的 "an error nothing caught in the main process is logged, with no box"：抛出一个未捕获异常和一个未处理的 rejection，断言两条都记进了 main.log、没有弹错误框，并且 App 照常应答 `evaluate`。

- **方案 A（推荐，现在的实现）：记日志，继续运行。** 这正是 Electron 默认的做法，只是去掉了弹框。
  - 主进程手里的状态很少：会话在宿主里，窗口的数据按游标从宿主重新取。一处未捕获的错误通常只影响那一次操作。
  - 风险是某个子系统坏了却悄无声息，比如一条窗口桥不再重连。靠两点兜住：日志里有栈，可以按栈修根因；e2e 遇到任何这类错误都会失败。
  - 以后可以照宿主的做法（5.11.4）补一条：下次启动时提示一次"上次主进程出错"，并引导导出诊断包（M2）。这不在本任务内。
- **方案 B：记日志后重启 App**（`app.relaunch()` 再 `app.exit(1)`）。
  - 这是 Node 文档的建议：未捕获异常之后，进程状态不再可信。
  - 代价有三：窗口重新加载会打断用户（宿主里的回合照常运行）；要防重启循环，比如 60 s 内第二次就退出不再重启；要多写约 30 行和对应的用例。
- **方案 C：** 未处理的 rejection 按 A 处理；同步抛出的未捕获异常按 B 处理。
