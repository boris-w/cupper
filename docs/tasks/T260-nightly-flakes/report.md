# T260 汇报

- 执行者：子 agent（前端与 e2e）
- 分支：`fix/T260-nightly-flakes`
- 完成日期：2026-11-13

三项都查清了原因并改好，分支上的连跑全部通过。只有 helpers 那一项在 runner 上复现不了，是按栈修的。PR #113（https://github.com/boris-w/cupper/pull/113）在 `40e0da4a` 上 8 个检查全部通过。规模 +55，停止线 150。

## 做了什么

**1. E2E-review-comment：审查文档在悬停之后又重画了一遍**
- 两晚都卡在 `review.spec.ts:181`：悬停第 2 行之后，"+"（`[data-utility-button]`）一直没出现。在 main 上连跑，一次一份和一次两份都是 20 次里失败 1 次，所以和窗口互相遮挡无关。
- 原因：
  - pierre 用 worker 高亮时，先把行按纯文本画一遍，worker 回来后再画一遍，第二遍会替换行元素。
  - "+" 挂在悬停时那一行的行号元素上。第二遍画完后它还挂在已经不在文档里的旧元素上，鼠标不动就不会再有 `pointermove`，点击只能一直等。
- 在截图工具的页面上用临时用例验证过（没有提交）：
  - 纯文本那一遍的 token 不带 style，高亮那一遍带 `--diffs-token-…` 颜色。
  - 替换发生在首次渲染后约 190–270 ms，12 次里有 3 次晚于"第一行可见"，而测试正是在那一刻悬停。
  - 改成等带颜色的 token 出现后再悬停，8 次都没有再替换。
- E2E-S3 一直没出问题，是因为它的 notes.txt 是纯文本，只画一遍。
- 改法（`ae2b6a4f`）：共用的 `reviewing()` 改为等 `[data-line] span[style]` 出现，也就是高亮那一遍画完。没有加 sleep。用到 `reviewing()` 的另外几个审查用例也一起受益。

**2. helpers"主进程未捕获的错误"：Electron 44 在窗口销毁后抛错**
- 11-12 那次的 main.log：`quit code 0` 之后 23 ms，出现 `TypeError: Object has been destroyed at BrowserWindow.visibilityChanged`。和 T254 在 E2E-SEC-fake-host 见到的是同一个栈。
- 原因：
  - Electron 44 自己给窗口的 show/hide/minimize/maximize/restore 挂了监听，里面会问窗口是否可见。
  - macOS 上窗口的遮挡状态变化也会发 show/hide。窗口销毁后再来一次遮挡变化，这个监听就抛错。
  - Electron main 分支已在 electron/electron#53971 删掉了这个监听（标着 needs-manual-bp/45-x-y），44 里还有。
- 改法（`3f340de9`，按你给的三条条件）：
  - 窗口 `closed` 时去掉这五个事件的监听。
  - 注释写明原因并附上游链接，不写死 Electron 内部的函数名。
  - 加了一条单元测试：监听会抛错的 emitter，在 `closed` 之后再发 hide/show 不抛错。
- **复现不了，按栈修**：main 上的基线连跑 ×40（43379707031）一次都没出现，以后以 nightly e2e 为准。

**3. UNIT-views-differential：那台 runner 慢，不是用例变多**
- Rust 那半每次固定写 900 个用例，数量没有变。
- TS 那半在 runner 上平时 3.4 s，已经用掉 5 s 时限的 68%；11-12 那晚用了 20.2 s。同一份用例在本机只要 1.5 s，所以是那台 runner 慢，没有哪个用例异常。
- 改法（`568b7a43`）：每个用例单独一个测试（`test.each`），另加一条"用例文件不为空"。时限按单个用例算，失败时直接报出是哪一条。

**4. security-peer（macos-15）11-13 被取消：GitHub 容量问题，只记录**
- 注解是 "The job was not acquired by Runner of type hosted even after multiple attempts"，同时有 macOS arm64 runner 容量受限的通知。
- 排队 19 分钟后被取消，一步都没跑。没有改 CI 设置。

## 关键取舍

- **review**：选了"等界面到最终状态"，没有用"悬停后点不到就重试"的循环（那样会掩盖竞态），也没有把文件换成纯文本（那样只是绕开了 markdown 高亮这条真实路径）。带颜色的 token 正是高亮那一遍画完的确切标志。产品本身没改：真实用户只要再动一下鼠标，"+" 就会回来。
- **visibilityChanged**：修在产品里，没有在测试夹具里过滤这条错误。过滤会让它在用户的真实日志里继续出现，夹具也仍会把其他任何未捕获的错误报出来。
- **differential**：按你的意见，没有连跑 nightly 10 次。这个改法去掉的是"总时长受单个测试时限约束"这一点，不是靠跑得更快过关，在分支上跑一次核对就够。

## 验证方式

| 运行 | 结果 |
|---|---|
| e2e-repeat，E2E-review-comment，一次一份（43384898710） | rest 组 20/20；@focus 组的 "E2E-review-comment: a copy …" 20/20。main 基线 43379698230：rest 19/20 |
| e2e-repeat，同上，一次两份（43384903110） | 20/20 + 20/20。基线 43379703136：19/20 |
| e2e-repeat，helpers ×40，一次两份（43384907162） | 40/40。基线同样 40/40，没复现 |
| nightly `job=differential`（43384911286） | 见下 |
| PR #113，`40e0da4a` | 8/8 通过 |

- differential 那一次：Rust 写了 900 个用例，TS 收集到 901 个测试（900 个用例 + 1 条不为空的检查），全部通过，0 个跳过。最慢的一条是 "random 0: the log"，36.8 ms（第一条，含预热）；其次两条约 25.6 ms；合计 3.8 s。
- 本机还跑过：
  - 桌面主进程的单元测试；
  - 用 11-12 的用例文件跑差分测试：901 个通过；把一条的期望值改坏后，只有那一条失败，并报出它的名字；
  - prettier、eslint、tsc。
- 对照验收标准：
  - 三项都给出了原因。
  - review 在 runner 上一次一份、一次两份各 ×20 全过；差分测试按你的意见跑了一次核对；helpers 按"复现不了、按栈修"处理，以 nightly 为准。
  - 没有加 sleep。
  - PR 8 个检查全部通过。

## 提交列表

```
40e0da4a docs(task): T260 notes on the three causes
ae2b6a4f test(e2e): the review tests start once the lines are highlighted
3f340de9 fix(desktop): a closed window drops its visibility listeners
568b7a43 test(views): a differential test per random stream
```

## 遗留问题 / 范围外发现

- test-repeat 的 vitest 模式不会生成 `CUPPER_VIEW_CASES`，所以差分测试在里面只会跳过，而这个 workflow 把跳过算作失败。以后要连跑差分测试，只能派 nightly `job=differential`。
- 证据和数据都在 `docs/tasks/T260-nightly-flakes/notes.md`（已提交）。
