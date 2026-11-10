# T245 汇报

- 执行者：子 agent（前端工程师，ui-core）
- 分支：`fix/T245-stream-follow`
- 完成日期：2026-11-09

## 做了什么

1. **先确认成因**（写在 notes.md）：在 jsdom 里模拟一次不是用户造成的滚动，类似 Chromium 的滚动锚定：内容高 1400，scrollTop 被挪到 850。旧代码因此取消了贴底，列表停在 850，没有回到 1400。这一条现在是 UNIT-stream-follow 的第一个用例，在旧代码上失败（`expected 850 to be 1400`），在新代码上通过。
2. **修正**（`packages/ui-core/src/stream/scroll.ts`、`StreamList.tsx`）：
   - `ScrollKeeper` 现在记录用户的滚动意图。只有用户操作后 500 ms 内发生的位置变化，或者按住滚动条期间的变化，才能取消贴底。用户操作指滚轮、触摸、滚动键（PageUp、PageDown、Home、End、上下方向键、空格）。其他来源的滚动只能把列表重新判为贴底。
   - 按住列表本身（也就是滚动条）时一直算作意图；在 window 上收到 pointerup 或 pointercancel 后，从松开时刻再算 500 ms。按在行上不算。
   - 应用自己的跳转（`jumpTo`、计划药丸）用 `leave()` 明确取消贴底；↓ 按钮恢复贴底。
   - 容器加了 `overflow-anchor: none`。
   - 加了 ResizeObserver：贴底时容器尺寸一变，就放回底部。
3. **用例**：
   - 新增 UNIT-stream-follow 4 条：
     - 非用户的滚动不取消贴底；
     - 纯逻辑：意图窗口、按住滚动条、松开、`leave`；
     - 按键和滚动条会让列表离开底部，按在行上不会；
     - 尺寸变化后回到底部。
   - jsdom 里没有 ResizeObserver，替身放在 `stream/test-resize.ts`，stream.test 和 plan-error.test 共用。
4. **截图用例**：
   - 去掉了 UI-turn-changes 里标着 T245 的位置固定（`toEnd` 辅助函数和撤销那张的 `scrollIntoView`）。
   - 在 UI-late-output 里加了一条：流式输出时跳到原条目，之后继续流式输出，列表停在原条目不动。

## 关键取舍

- **用意图窗口判断，而不是看 `isTrusted` 或比较位置**：浏览器自己造成的滚动和用户的滚动，scroll 事件长得一样，只能看前面有没有用户的输入。
  - 窗口取 500 ms：键盘平滑滚动约 150–400 ms，落在窗口里。
  - 触控板的惯性滚动全程都会发滚轮事件，每次都刷新窗口，所以一直算作意图。
  - 计时用 `performance.now()`（单调时钟）。这个值在事件处理时取，`ScrollKeeper` 本身仍然是不做 I/O 的纯逻辑。
- **跳转用例放在 Playwright，没有放在 jsdom**：在 jsdom 里写 `jumpTo` 的用例时，TanStack 的 `scrollToIndex` 会一直对齐下去，单个 CPU 跑满 100–400%。我只结束了自己的进程，确认没有残留，之后改成 UI-late-output 里的截图用例。
- **三处测试输入的修正**（产品对真实用户的行为没有变）：原来这些用例用程序化滚动冒充用户的滚动，新规则不再认它，所以都补上了用户的输入：
  - UNIT-stream-anchor 里 T208 的两条：滚动前先发一次 `fireEvent.wheel`；纯逻辑用例先调用 `intend(0)`。
  - E2E-NFR1-4 的"往前补历史"用例（`tools/e2e/src/nfr1.spec.ts`）：在同一个 `evaluate` 里先在列表上派发一个冒泡的 `WheelEvent`，再设 `scrollTop = 0`。
    - 意图监听是 React 的 `onWheel`，不看 `isTrusted`，所以不需要经 `sendInputEvent` 发真实的滚轮事件。
    - 不可信的滚轮事件没有默认动作，自己不会滚，位置仍由 `scrollTop` 决定。
    - 断言（视图不动）没有改。理由写在 notes.md。

### 为什么 stream-changes-undo 的基线变了
- 去掉位置固定以后，用户停在底部点"撤销"，确认框让卡片变高。按新规则，只有用户能取消贴底，所以列表继续贴底。
- 新图里最后一张卡片"这一轮的改动没能记录"完整可见，没有 ↓ 按钮；确认框完整，第一张卡片的标题行被推到视口上方。
- 旧图是 `scrollIntoView` 把列表从底部拉开后拍的，所以显示着 ↓ 按钮，这正是本任务要消除的情况。
- 新基线来自 ui-baseline 42589255119。下载到新的空目录后逐字节核对：66 个文件里只有这一张不同，`meta.json` 和其余 64 张图完全一样。

### 为什么 plan-error.test 要改
- 它也挂载 `StreamList`。消息流现在用 ResizeObserver，jsdom 里没有，5 个用例因此报 "ResizeObserver is not defined"。
- 改法是在 `beforeAll` 里装上和 stream.test 共用的替身，用例本身没动。

## 验证方式

- **UNIT-stream-follow、UNIT-stream-anchor 通过**：
  - 本机：stream.test 11 条通过，ui-core 的 vitest 全部 122 条通过；tsc、eslint、prettier 都通过。
  - runner：test-repeat 42589259180（https://github.com/boris-w/cupper/actions/runs/42589259180 ），10 次 × 4 份并行，macos-15。40 份日志我逐个看过，去掉颜色码后都是 `Test Files 1 passed (1)`、`Tests 11 passed (11)`，没有一条失败，每次约 3.4 s。
  - 这个作业本身显示失败，是误报，原因见下一节"遗留问题"。按你的决定，记为 40 次全部通过。
- **UI-turn-changes 去掉位置固定后在 runner 上通过**：
  - 本机无头单份跑过 UI-turn-changes 和 UI-late-output，除了撤销那张（预期会变）都通过。
  - PR 上的 `ui` 作业在换上新基线后通过。基线只变了撤销那一张，这是你批准的。
- **E2E-NFR1-4**：e2e-repeat 42593417168（https://github.com/boris-w/cupper/actions/runs/42593417168 ）`40 passed`，4 条用例 × 10 次，其中"往前补历史"这条 10/10。
- **PR 的 8 个检查**：PR #86，运行 https://github.com/boris-w/cupper/actions/runs/42593423947 。e2e、rust、ui、compat-framework、ts、license、testdata-scan、protocol 都通过。
- **规模**：`cargo xtask size` 为 +228 −32（净 +196），你已接受。ui-core +201 −18，tools/ui-screenshots +22 −13，tools/e2e +5 −1。

## 提交列表

```
0c9969a1 docs(task): T245 notes: the e2e case's wheel
711764f5 test(e2e): E2E-NFR1-4's history case scrolls up with the user's wheel
e1de0b7f test(ui-screenshots): the undo shot of UI-turn-changes at the stream's end
564104c8 docs(task): T245 notes: the cause shown, the intent window, size
3cf0a46a test(ui-screenshots): UI-turn-changes without its pins; UI-late-output's jump while streaming
2861f77f fix(ui-core): only the user takes the stream away from its end
```

## 遗留问题 / 范围外发现

- **test-repeat 跑 vitest 时误报（已立为 T247）**：
  - workflow 用 `RAN='…|Tests +[1-9][0-9]* passed'` 判断用例确实跑了而且通过。
  - runner 上设了 `CI`，Vitest 因此输出带颜色，"Tests" 和数字之间夹着 ANSI 转义码，正则匹配不上，每一次都记成 FAILED。
  - 之前的 test-repeat 都是 cargo，这是第一次跑 vitest。
  - 修法可以是在 Run 步骤加 `NO_COLOR: "1"`，或者判定前先去掉转义码。
- **jsdom 里的 `scrollToIndex`**：在 jsdom 里，TanStack Virtual 的 `scrollToIndex` 会一直对齐下去，跑满 CPU。以后要在 jsdom 里测跳转，应该先替换掉它，或者直接放到 Playwright 里测。
- **撤销那张截图**：新图里第一张卡片的标题行（含"撤销"按钮）在视口上方。截图要表达的确认框完整可见；如果以后希望同一张图里也能看到"撤销"按钮，需要调整截图用例的窗口或内容，不在本任务范围内。
