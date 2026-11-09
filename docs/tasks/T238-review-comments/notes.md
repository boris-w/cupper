# T238 notes

## 开工前的估算（2026-11-04）

叠在 T131 的分支上做。T131 已经有了端口协议、`ReviewFrame` 对 `hunk_action`、`draft_comment` 的校验，以及还原经 `askRevert` 进确认控件（来源为 `view`）。本任务要补的是审查文档里的交互和用例。

| 模块 | 实现 | 测试 |
|---|---|---|
| A. 协议与主文档：`files` 加回 `actions`、`drafts`，`ReviewFrame` 下发 | 10 | — |
| B. 文件头的按钮（按范围） | 20 | — |
| C. hunk 按钮：每个 hunk 末行挂一条 annotation，从 pierre 的 hunk 序号对回宿主的 `hunk_id` | 45 | — |
| D. 行内评论：pierre 自带的行号"+"（`enableGutterUtility` 和 `onGutterUtilityClick`，可拖选多行）、评论框（Esc 关闭、⌘Enter 保存）、引用的行文、已有草稿显示在行上 | 90 | — |
| E. 评论框里的按键不转发；文案 | 15 | — |
| F. e2e 钩子：复制一段文本（核实剪贴板用） | 5 | — |
| G. 用例：E2E-review-comment；E2E-SEC-revert-confirm；剪贴板核实（`@focus`，只在 runner 上跑）；截图工具里一个评论交互用例 | — | 200 |
| 合计 | ~185 | ~200 |

合计约 385，按以往偏差上浮后约 460，在停止线 500 之内。

## 实现记录

- **按钮**：
  - 范围的操作随 `files.actions` 下发，`ReviewFrame` 按 `ACTIONS[scope]` 给出，不在审查文档里判断范围。
  - 文件头的按钮发不带 `hunk_ids` 的 `hunk_action`，主文档按整个文件执行。
  - hunk 的按钮挂在每个宿主单元最后一个 pierre hunk 的末行下面（annotation）。pierre 的 hunk 序号按 `patchOf` 的顺序对回宿主的 `hunk_id`：一个 `whole_file` 单元可能有几个 `@@`，按钮只挂在最后一个下面。
  - 文件模式的单元没有单独的按钮，文件头的按钮已经覆盖它。
  - 按钮是只有图标的圆形 ghost 按钮，宽 28；tooltip 用 `title`，审查文档里没有 Radix。
- **还原**：审查文档只发消息。`ReviewFrame` 校验后调 `act`，进 T130 的 `askRevert`（来源 `view`）：不抢焦点，同一时间只有一个。
- **行内评论**：
  - 用 pierre 自带的行号"+"（`enableGutterUtility` 加 `onGutterUtilityClick`）。按住拖动可以选多行，松开时回调给出行范围。
  - 评论框是 range 末行下的一条 annotation：Esc 关闭，⌘Enter 或"添加评论"保存；关闭时清掉 pierre 的选中行。
  - 引用的行文取自 pierre 的 `additionLines`/`deletionLines`：按 hunk 的 `*LineIndex` 换算。
  - 草稿由主文档随 `files.drafts` 下发，显示为对应 `line_end` 行下的卡片。
  - 评论框是 textarea，`keydown` 的目标是 textarea 时不转发给主文档。
  - 打开着、还没保存的评论框只是审查文档里的临时状态，iframe 重载就没了。保存了的草稿在主文档，不会丢。
- **剪贴板**：
  - e2e 版本的审查文档钩子多一个 `copy(text)`，即 `navigator.clipboard.writeText`。
  - 用例带 `@focus`，并且只在 `GITHUB_ACTIONS=true` 时运行，本机直接跳过，不碰剪贴板。
  - 用例先确认焦点在主文档时写入被拒（`NotAllowedError`）；再点进审查文档写入，在主进程带等待地轮询 `clipboard.readText()`，把读到的值打印到日志里。
- **本机跑过**：
  - E2E-review-comment（评论部分）、E2E-SEC-revert-confirm、E2E-SEC-review-iso，各单份一次，都通过。都是后台模式：窗口不激活，没有真实按键，不碰剪贴板。
  - 截图工具用临时配置在无头 Chromium 里跑了 review 的 19 个用例，功能都通过。
- **基线**：
  - 每个文件头多了按钮，hunk 下多了一行按钮，review、review-tree、review-commit 的 15 张基线都会变。
  - 只用 `ui-baseline` 生成。

## runner 上的验证（2026-11-04）

- e2e-repeat 41794397923（E2E-review-comment × 10）：
  - 评论用例 10 次全过；
  - 剪贴板用例 10 次全过，带 `@focus`，单独一轮。
  - 每次都先确认焦点在主文档时写入得到 `NotAllowedError`，再点进审查文档写入。主进程带等待地轮询 `clipboard.readText()`，10 次读回的都是 `"t238 copied"`，和写入的一致。
  - T104 记下的"读回对不上"，原因落在第一种怀疑：读得太早，没等写入落到系统剪贴板，不是 runner 的剪贴板服务有问题。带等待地轮询之后就对上了。
- e2e-repeat 41794402045（E2E-SEC-revert-confirm × 10）：10 次全过。
- ui-baseline 41794406105：14 张基线变了，逐张核对后提交（2c4a651d）。

## merge T131 分支（2026-11-09，含 main ad61061d 与 T132）

- 没有冲突：T132 改的是 `ReviewPanel.tsx`、`state.ts`、`view.ts`，T131 分支已经解决过；T238 只改审查文档和 `ReviewFrame` 下发的 `files`。
- **`turn` 范围（T132：某一轮的改动，由改动卡片打开）**：
  - 按钮：仍取 `ACTIONS[scope]`，`turn` 只有还原。文件头和 hunk 末尾只出现还原按钮，不出现暂存、取消暂存。还原照样经主文档的确认控件，令牌是这一轮 diff 的令牌，宿主按它校验。
  - 行内评论：`turn` 范围也允许写。行号和引用的行文都来自这一轮的 diff；如果文件后来又改了，行号可能对不上，这时引用的行文仍能让 agent 认出说的是哪一行。不为 `turn` 单独禁用评论。
  - 本机无头跑了截图工具里 T132 的"改动卡片打开审查面板"：面板是"第 1 轮"，文件头和 hunk 末尾只有还原按钮。
- 合并后本机：tsc（ui-core、desktop、ui-screenshots、e2e）、ui-core 的 vitest（121 个）、全仓 eslint 和 prettier 都通过；截图工具用临时配置在无头 Chromium 里跑了 review 和 stream 两个 spec，39 个全过。
- 基线：`stream-changes-review.png` 在 T131 上是没有按钮的版本，到 T238 上会再变；T238 自己的 14 张也要在 merge main 之后重新核对。等 T131 合并、merge main 之后，由编排者跑 ui-baseline。
