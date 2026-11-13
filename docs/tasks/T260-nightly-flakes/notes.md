# T260 笔记

## E2E-review-comment

- 两晚（43126389561、43328452471）都停在 `review.spec.ts:181`：悬停第 2 行之后，`[data-utility-button]` 一直没出现。error-context 里审查文档已经画出 README.md 的两行和 hunk 的按钮。
- 在 main 上连跑（43379698230 一次一份、43379703136 一次两份）各 20 次，各失败 1 次，所以和窗口互相遮挡无关。
- 原因：pierre 用 worker 高亮时，先按纯文本画一遍，worker 回来后再画一遍，第二遍替换列里的行元素（`columns.content.innerHTML = …`）。"+" 挂在悬停时那一行的行号元素上（`showUtilityOnLine`）；第二遍画完后，`placeUtility` 还挂在旧的、已经不在文档里的元素上，鼠标不动就不会再有 `pointermove`。
- 用截图工具的页面探了一下（临时用例，未提交）：纯文本那一遍的 token 是不带 style 的 `<span>`，高亮那一遍带 `style="--diffs-token-dark:…;--diffs-token-light:…"`；替换发生在首次渲染后约 190–270 ms，12 次里有 3 次晚于"第一行可见"。等到带颜色的 token 出现再悬停，8 次都没有再替换，"+" 一直在。
- E2E-S3 不受影响：它的 notes.txt 是纯文本语言，不走高亮，只画一遍。
- 改法：`reviewing()` 等 `[data-line] span[style]` 出现，即高亮那一遍画完。

## helpers 未捕获错误

- 11-12 那次的 main.log：`quit code 0` 之后 23 ms，`TypeError: Object has been destroyed at BrowserWindow.visibilityChanged`。T254 在 E2E-SEC-fake-host 见过同一个栈。
- Electron 44 的 `lib/browser/api/browser-window.ts` 给窗口挂了 show/hide/minimize/maximize/restore 的监听，里面调 `this.isVisible()`；macOS 上窗口的遮挡状态变化也会发 show/hide（`electron_ns_window_delegate.mm` 的 `windowDidChangeOcclusionState`）。窗口销毁后再来一次遮挡变化，监听就抛错。
- Electron main 分支在 electron/electron#53971（2026-09）删掉了这个 JS 监听，标着 needs-manual-bp/45-x-y，44 里没有。
- 基线连跑 ×40（43379707031，一次两份）没有复现。按栈修：窗口 `closed` 时去掉这五个事件的监听。

## UNIT-views-differential

- Rust 那半每次写 900 个用例（300 × 3 条流，都是会话视图，约 3.4 万个事件），数量没变。
- TS 那半在 runner 上平时 3.4 s（11-11、11-13），11-12 那晚 20.2 s；同一份用例在本机 1.5 s。是那台 runner 慢，不是哪个用例异常。
- 改成每个用例一个测试：时限按条算，失败时报出是哪一条。

## security-peer（macos-15），11-13

- 注解："The job was not acquired by Runner of type hosted even after multiple attempts"，以及 macOS arm64 runner 容量受限的通知。排队 19 分钟后取消，一步都没跑。只记录。
