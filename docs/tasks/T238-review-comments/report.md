# T238 汇报

- 执行者：子 agent（前端工程师，桌面与渲染进程安全）
- 分支：`feat/T238-review-comments`
- 完成日期：2026-11-09
- PR：https://github.com/boris-w/cupper/pull/88
- PR 上的 CI（8802c8ae，8 个检查全过）：https://github.com/boris-w/cupper/actions/runs/42604667804

## 做了什么

T131 拆分出来的审查文档 II：审查文档里的交互、还原经确认控件、剪贴板核实。主文档对 `hunk_action`、`draft_comment` 的接收与校验在 T131 已经做好，本任务做审查文档这一侧，以及两个 E2E。

1. **hunk 与文件按钮**（`packages/ui-core/src/review/document/ReviewDocument.tsx`）
   - 范围的操作由 `ReviewFrame` 按 `ACTIONS[scope]` 随 `files.actions` 下发，审查文档不自己判断范围：最近一轮和 `turn` 只有还原；未暂存有暂存、还原；已暂存只有取消暂存。
   - 文件头上是整个文件的按钮，发不带 `hunk_ids` 的 `hunk_action`。
   - 每个宿主单元最后一个 pierre hunk 的末行下面挂一行按钮（pierre 的 annotation），发带 `hunk_ids` 的 `hunk_action`。pierre 的 hunk 序号按 `patchOf` 的顺序对回宿主的 `hunk_id`；一个 `whole_file` 单元有几个 `@@` 时，按钮只挂在最后一个下面。
   - 按钮按设计 5.7.5：只有图标的 ghost 圆形按钮，宽 28，tooltip 写操作名（审查文档里没有 Radix，用 `title` 和 `aria-label`），文案来自主文档下发的 `strings`。
   - 文件模式的单元没有单独的按钮，文件头的按钮已经覆盖它。

2. **还原只经主文档的确认控件**
   - 审查文档只发消息。`ReviewFrame` 校验令牌、范围、`hunk_ids` 之后才调 `act`，进 T130 的 `askRevert`，来源为 `view`。
   - RP-11：确认控件不自动获得焦点，同一时间只显示一个，已有一个时忽略新的。

3. **行内评论**（Codex 的交互）
   - 用 pierre 自带的行号"+"（`enableGutterUtility` 加 `onGutterUtilityClick`）：悬停行号出现"+"，按住拖动可以选多行，松开时给出行范围。
   - 评论框是行范围末行下的一条 annotation，只写草稿：Esc 关闭，⌘Enter 或"添加评论"保存，关闭时清掉 pierre 的选中行。
   - 保存时发 `draft_comment`。`ReviewComment` 带 `path`、`line_start`、`line_end`、`side`（old/new）、`quote`、`comment`；引用的行文从 pierre 的 `additionLines`/`deletionLines` 按 hunk 的 `*LineIndex` 换算出来。
   - 主文档把草稿加进附件条，并随 `files.drafts` 下发，审查文档在对应 `line_end` 行下显示成卡片。
   - 评论框是 textarea：`keydown` 的目标是 textarea 时，按键不转发给主文档。
   - 打开着、还没保存的评论框只是审查文档里的临时状态，iframe 重载就没了。保存了的草稿在主文档，不会丢。

4. **剪贴板核实**（T104 留下的问题）
   - e2e 版本的审查文档钩子多一个 `copy(text)`，即 `navigator.clipboard.writeText`。
   - 用例带 `@focus`，并且只在 `GITHUB_ACTIONS=true` 时运行，本机直接跳过，不碰用户的剪贴板。
   - 用例先确认焦点在主文档时，审查文档写入得到 `NotAllowedError`；再点进审查文档写入，在主进程带等待地轮询 `clipboard.readText()`，并把读到的值打印到日志里。
   - **结果**：runner 上连跑 10 次，每次读回的都是 `"t238 copied"`，和写入的一致。T104 的"读回对不上"，原因是它怀疑的第一种：读得太早，写入还没落到系统剪贴板，不是 runner 的剪贴板服务有问题。带等待地轮询就对上了。

5. **用例**
   - **E2E-review-comment**（nightly）：在审查文档里悬停行号、点"+"、写评论、⌘Enter；草稿显示在行下，附件条显示"1 条评论"；下一条消息发出后，用户消息显示"1 条评论"，宿主记录的内容带上了这条评论和它的文件。
   - **E2E-SEC-revert-confirm**（nightly）：
     - 审查文档里文件的还原只弹出确认，不自动获得焦点；
     - 已有一个确认时，再点 hunk 的还原被忽略，标题不变；
     - "还原全部"触发时默认焦点在"确认"；
     - 取消之后文件不变。
   - 剪贴板核实：见上文第 4 项。
   - 截图工具新增一个行为用例：文件和 hunk 的按钮只发请求，还原只弹确认；评论框的 Esc 和 ⌘Enter；草稿显示在行下，附件条显示"1 条评论"。
   - 测试输入的两条路径分开用（report-q29 第 6 节第 3 条）：审查文档里的点击和按键走 Playwright 的 CDP，因为 `sendInputEvent` 进不了进程外的 iframe；这些都不算真实输入，确认控件也只是打开，不需要真实点击。

## `turn` 范围（T132）

- **按钮**：仍取 `ACTIONS[scope]`，`turn` 只有还原，所以文件头和 hunk 末尾只出现还原按钮。还原照样经主文档的确认控件，令牌是这一轮 diff 的令牌，宿主按它校验。
- **行内评论**：`turn` 范围也允许写。行号和引用的行文来自这一轮的 diff；如果文件后来又改了，行号可能对不上，但引用的行文仍能让 agent 认出说的是哪一行。不为 `turn` 单独禁用评论。
- 已核对：T132 的"改动卡片打开审查面板"，无头本机看过，基线也核对过，都只出现还原按钮。

## 两次 merge

1. **merge T131 分支**（c3267511，其中已含 main ad61061d 和 T132），合并提交 `8ee947cc`，没有冲突：T132 改的三个文件 T131 已经解决过，T238 只改审查文档和 `ReviewFrame` 下发的 `files`。
2. **merge main**（b8e9a4b4，T131 合并之后），合并提交 `03a54c43`，没有冲突。合进来的本机 main 比当时的 origin/main 多一个文档提交，已提醒你先推 main，你已推送。

## 关键取舍

- **hunk 按钮放在 hunk 末行下面**：`CodeView` 的分隔符不能自定义（不支持 `custom`），annotation 只能挂在某一行下面。挂在第一行下面会把 hunk 切开，挂在末行下面就像 hunk 的页脚。长 hunk 要滚到末尾才看得到它的按钮；文件头的整文件按钮一直在。
- **用 pierre 自带的"+"**，不自己画：它支持拖选多行，和 Codex 的交互一致。代价是"+"写在 pierre 的 shadow DOM 里（库内部用 innerHTML），我们给它加不了 aria-label。
- **剪贴板的核实用测试钩子写入**，不新增面向用户的复制按钮：设计里审查文档没有复制按钮，report-q29 写的是"pierre 的复制入口或一个测试用的复制动作"。

## 验证方式

本机，都是单份：
- tsc（ui-core、desktop、ui-screenshots、e2e）、vitest（ui-core 121 个、desktop 103 个）、全仓 eslint 和 prettier 都通过。
- E2E-review-comment（评论部分）、E2E-SEC-revert-confirm、E2E-SEC-review-iso：本机构建 e2e 版本后各单份跑过，都通过。都是后台模式，没有真实按键，不碰剪贴板；剪贴板用例在本机跳过。
- 截图工具：用临时配置在无头 Chromium 里跑了 review 和 stream 两个 spec，都通过。

runner 上：
- e2e-repeat，E2E-review-comment × 10（含 `@focus` 的剪贴板用例，各 10 次全过，读回 10 次都是 `"t238 copied"`）：https://github.com/boris-w/cupper/actions/runs/41794397923
- e2e-repeat，E2E-SEC-revert-confirm × 10，全过：https://github.com/boris-w/cupper/actions/runs/41794402045
- ui-baseline 41794406105（21719ac2）：14 张变了，review 系列加上按钮，逐张核对后提交（2c4a651d）；review-empty 没有 diff，没变。
- ui-baseline 42591084166（03a54c43）：只变了 `stream-changes-review.png`，`turn` 范围只有还原按钮，已提交（8802c8ae）。
- PR #88 在 8802c8ae 上 8 个检查全过：https://github.com/boris-w/cupper/actions/runs/42604667804

对照验收标准：

| 验收项 | 结果 |
|---|---|
| 两个 E2E 通过，并在 runner 上连跑 10 次 | 通过（41794397923、41794402045） |
| iframe 发来的操作经主文档校验 | 是（T131 的 `ReviewFrame`，E2E-SEC-review-iso 覆盖） |
| 还原只经主文档的确认控件 | 是（E2E-SEC-revert-confirm） |
| 文案走 i18n，agent 给的字符串不进样式 | 是：审查文档的文案全部由主文档经 `strings` 下发；评论正文、引用、路径只作为文本显示 |
| 剪贴板用 e2e 核实，并记下实际读到的值 | 是：10 次读回都是 `"t238 copied"` |
| PR 上 8 个检查全过 | 是（42604667804） |

## 规模

- 相对 main：+443 −45（不含文档、基线、lock 文件），停止线 500，硬线 600。

## 提交列表

```
8802c8ae test(ui-screenshots): the turn's review with the review document's revert buttons
03a54c43 Merge branch 'main' into feat/T238-review-comments
d58dc700 docs(T238): the turn scope after merging the T131 branch
8ee947cc Merge branch 'feat/T131-review-document' into feat/T238-review-comments
7cf18045 docs(T238): the runner's results and the clipboard's read-back value
2c4a651d test(ui-screenshots): baselines with the review document's actions
21719ac2 docs(T238): notes on the actions, the comments and the clipboard check
de97b09e test(review): E2E-review-comment, E2E-SEC-revert-confirm and the clipboard
c3747b15 feat(review): hunk and file actions and inline comments
8f5233d9 docs(T238): the estimate before work starts
```

## 遗留问题 / 范围外发现

- pierre 自带的"+"按钮没有可访问名称（库内部渲染，我们改不了）。要补的话，可以改用 `renderGutterUtility` 自己渲染一个带 `aria-label` 的按钮；代价是要自己处理拖选多行。
- 长 hunk 的按钮在末尾。如果用户希望在 hunk 开头就能操作，可以考虑在 pierre 支持自定义分隔符之后，把按钮挪到分隔符里。
- T145 给输入框加了"+"，T238 合并后，T145 还要 merge main，并核对重叠的 `stream-changes-review` 和 review-commit 三张基线。
