# T245 工作笔记

- 执行者：子 agent（前端工程师，ui-core；T132 的执行者接手）

## 成因的确认

`ScrollKeeper` 分不出 scroll 事件是谁造成的：

- `StreamList` 的 onScroll 对每一次 scroll 事件都调 `scrolled()`，按当时离底部的距离（24 px 以内）重判是否贴底；
- 渲染后的 `follows()` 把"列表不在上次放的位置"当作用户滚过，同样按当时的距离重判。

于是一次不是用户造成的位置变化，只要那一刻离底部超过 24 px，就把贴底取消了，之后内容再长也不跟随。排版重测时浏览器会自己动 scrollTop：滚动锚定按锚点调整，内容暂时变矮时夹回范围以内。

**构造**：UNIT-stream-follow 的第一条用例（jsdom，挂上真实的 `StreamList`）。列表贴底（scrollTop 800，scrollHeight 1000，可视 200）；各行在变窄的列表里重排变高（scrollHeight 1400），浏览器按锚点把 scrollTop 移到 850 并发出 scroll 事件，期间没有 wheel、按键、触摸。现在的代码：列表停在 850，不再回到底部（`expected 850 to be 1400`）。

## 做法

- `ScrollKeeper` 记下用户的滚动意图（wheel、触摸、翻页键与方向键、在滚动条上按下并拖动），意图之后的一小段时间（以及按住滚动条期间）里的位置变化才算用户的；只有这样的位置变化能取消贴底。不是用户造成的位置变化只能把列表重新判为贴底。
- 在列表行上按下（点按钮、点文件）不算滚动意图：T132 的失败正是点了卡片里的文件、侧边面板随之打开。只有按在列表容器本身（滚动条）上才算。
- 消息流容器加 `overflow-anchor: none`。
- 容器尺寸变化（ResizeObserver）时，贴底就放回底部。
- T208 的两条用例（UNIT-stream-anchor）写的是"用户滚开了，但事件还没到"：那是用户的滚动，用例补上用户的 wheel；行为不变。

## 意图的时间窗口

- `INTENT_MS = 500`：从最后一次 wheel、touchmove、滚动键（PageUp、PageDown、Home、End、上下方向键、空格）算起，500 ms 内的位置变化算用户的。键盘的平滑滚动约 150–400 ms，落在窗口里。
- 计时用 `performance.now()`（单调时钟），事件处理时取一次，`ScrollKeeper` 的方法收 `now` 参数，纯逻辑可以在 jsdom 里用固定数值测。
- 触控板的惯性滚动在整个惯性期间持续发 wheel 事件，每一次都刷新窗口，所以一直算作意图；停下后 500 ms 窗口过期。
- 按在列表容器本身（滚动条）上：按住期间一直算意图（`Infinity`），在 window 上收到 pointerup 或 pointercancel 后，从松开时刻再算 500 ms。
- 应用自己的跳转（`jumpTo`、计划药丸）用 `leave()` 明确取消贴底；↓ 按钮恢复贴底。

## 实际规模

`cargo xtask size`：+223 −31（净 +192），编排者把硬停止线放到 210 后又接受了 223。实现：`scroll.ts` +39，`StreamList.tsx` +53（约 15 行是 prettier 拆行）；测试：UNIT-stream-follow 与 UNIT-stream-anchor 的改动约 +100，ResizeObserver 替身 `test-resize.ts` 约 +20（plan-error.test 也挂载消息流，同样要用），截图用例 +23 −13。

## 两处说明

- **stream-changes-undo 的基线要变**：去掉固定位置后，用户停在底部点"撤销"，卡片变高，按新规则只有用户才能取消贴底，列表继续贴底，确认控件仍在画面里。旧基线是旧代码在 runner 上碰巧拍下的位置（Playwright 的程序化滚动把贴底取消了，正是本任务要消除的情况）。由 ui-baseline 重新生成这一张。
- **plan-error.test 要改**：它也挂载 `StreamList`，消息流现在用 ResizeObserver，jsdom 里没有，5 个用例因此失败；和 stream.test 共用 `stream/test-resize.ts` 里的替身。
- **E2E-NFR1-4 的"往前补历史"用例要补用户的滚动**：它用 `list.scrollTop = 0` 模拟用户滚到顶，这一步没有用户意图，新规则不认它，列表又回到底部，`say t4` 等不到（PR #86 的 e2e）。和 T208 的两条单测一样，补的是用例的输入：在同一次 `evaluate` 里先在列表上派发 `WheelEvent`（冒泡，`deltaY` 为负），再设 `scrollTop = 0`。选这种而不是 `sendInputEvent` 的 mouseWheel，是因为意图监听是 React 的 `onWheel`，不看 `isTrusted`；不可信的 wheel 没有默认动作，不会自己滚动，位置仍由 `scrollTop` 决定，断言不变。两步在同一个任务里，scroll 事件在下一帧到，离 500 ms 的窗口很远。
