# T225 工作笔记

- 执行者：子 agent（桌面主进程工程师；T160 的执行者接手）

## 规模估计

开工时按任务书的四件事估：主进程的修复和日志约 40 行，launcher、control 的防护约 20 行，单测约 40 行，E2E-quit-during-reconnect 及 fixture 约 150 行，合计约 250 行（停止线 300）。实际 +199：apps/desktop +52，tools/e2e +147。

## 复现与原因

全在本机单份跑，用 e2e 构建（临时数据目录，不是 Cupper Local），一次一个 App。先用一个临时脚本（不入库）：把 e2e App 复制到临时目录，设好 `CUPPER_E2E_DIR` 启动，等 main.log 出现 `window connected`，然后分情形动手，最后发 SIGTERM，看 15 s 内是否退出。

| 情形 | 修前 |
|---|---|
| 不动宿主，直接 SIGTERM | 1.5 s 退出 |
| 换包（同一个构建，cdhash 不变），SIGTERM | 0.5 s 退出 |
| 杀宿主，50 ms 后 SIGTERM（这时已经重连上了） | 0.5 s 退出 |
| 杀宿主，3 s 后 SIGTERM | 0.5 s 退出 |
| 换包、杀宿主，50 ms 后 SIGTERM（新二进制首次执行慢，正在重连） | **卡住**：主程序、gpu-process、network service、重连拉起的宿主都在 |
| 杀宿主，2 ms 后 SIGTERM（正在重连，包没换） | **卡住**，两次都是 |

给 `before-quit`、`will-quit`、`quit`、窗口的 `close`/`closed`、`window-all-closed` 和退出判定加临时日志后：

- 正常：`before-quit`（未定）→ 判定 quit → `before-quit`（已定）→ 窗口 close → closed → `will-quit` → `quit`。
- 卡住：`before-quit`（未定）→ 判定 quit → `before-quit`（已定）→ 窗口 close → closed → **`window-all-closed`**，没有 `will-quit`。

`window-all-closed` 只在 Electron 认为"不在退出"时才发给 JS，说明这时 Electron 的 `is_quitting_` 是 false。原因：

- 原生的退出（SIGTERM、菜单的 ⌘Q）调用 `Browser::Quit()`，它执行 `is_quitting_ = HandleBeforeQuit()`，也就是先发 `before-quit`，等事件返回后才写 `is_quitting_`。
- 我们的 `before-quit` 先 `preventDefault()`，再 `quit.decide()`。控制连接不在 ready（正在重连，或已失败）时，`host/status` 立即以"连接丢失"结束，判定在同一次事件派发的微任务里就完成，`app.quit()` 就在外层 `Browser::Quit()` 还没返回时被调用。
- 内层那次把 `is_quitting_` 设成 true 并开始关窗；外层随后用"被 preventDefault"的结果把它写回 false。窗口关完，Electron 以为不在退出，只发 `window-all-closed`。我们的处理是什么都不做（macOS 上关掉最后一个窗口 App 照常留着），App 就留下了。
- 控制连接 ready 时，判定要等宿主应答，`app.quit()` 落在后来的回合里，不嵌套，所以正常的退出、以及"杀宿主后已经重连上"都没事。
- 从 JS 里调用 `app.quit()`（e2e 用例的 `app.evaluate`）时，栈上已有 JS，微任务不会在事件派发中途运行，所以不会嵌套。这也是现有的 E2E-quit-confirm 碰不到它的原因；回归用例必须用 SIGTERM。

这也解释了 T160 的三次运行：SIGTERM 时控制连接都不在 ready（第 1 次正在重连随后失败，第 2 次正在握手，第 3 次已经 `failed: spawn_failed`）。换包只是让重连变慢或失败，本身不是原因。

**修复**：判定出来后，用 `setImmediate` 在下一个回合再 `app.quit()`。修后上面卡住的两种情形（三次）都在 0.5 s 内退出，日志里都是 `will-quit` → `quit`。

**修复中顺带看到的**：修后"杀宿主后 2 ms 退出"那次，main.log 在判定之后还出现了一次 `host connection ready`：判定前已经在进行的重连，在 `end()` 之后才完成，照样 `attach` 上了。改为丢弃这个连接；launcher 在拨号期间被关闭时，也不再接着拉起宿主。`HostClient.attach()` 对已经 `close()` 的客户端已经是关掉端口、不改状态（main 上已有），没有再改。

## 回归用例

`tools/e2e/src/quit.spec.ts`，e2e 构建，Playwright 启动，但退出用 SIGTERM。涉及换包的情形用测试自己根目录下的 App 副本，fixture 的 `launch` 为此加了 `bundle` 参数。

- (a) 正在重连：把副本里的宿主换成一个先 `sleep 5` 再 exec 真宿主的脚本，杀掉宿主，等 main.log 出现 `reconnecting`，发 SIGTERM。
- (a) 重连已失败：删掉副本里的宿主，杀掉宿主，等 `failed`，发 SIGTERM。
- (b) 杀宿主、包没换：`killHost`（等它重连上），发 SIGTERM。
- (c) 换包、宿主还在：换成宿主被重签过（cdhash 不同）的副本，发 SIGTERM。
- (d) 换包、宿主已因待升级退出：换包后用 CLI 连一次，等旧宿主退出，立刻发 SIGTERM。

每例断言：10 s 内退出；main.log 里依次是 `before-quit`、`before-quit`、`will-quit`、`quit`；判定之后没有 `ready`；10 s 内没有本用例数据目录的宿主进程。

本机单份跑（`--workers 1`）：
- 修前的构建：(a) 两例和 (d) 都卡住（"the app quit within 10 s" 失败）；(b)、(c) 能退出，只因修前没有那几行日志而失败。
- 修后：5 例全过；`app.spec.ts` 的 E2E-single-instance 和 4 例 E2E-quit-confirm 也都通过。
