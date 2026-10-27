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
