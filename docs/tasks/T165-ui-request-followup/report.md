# T165 汇报（ui-core V-b：拒绝并附消息、请求记录行与多请求）

- 执行者：子 agent（前端工程师）
- 分支：`feat/T165-ui-request-followup`
- 完成日期：2026-10-20

T165 已完成：RP-12、请求记录行、多请求的"1 / 3"都做了，UNIT-request-followup、UI-request-records 通过，在干净的 worktree 里跑 `cargo xtask ci pr --record` 也通过。改动 347 行。"展开看全文"放不下，**没有做**，加上它约 490 行，超过约 450 的停止线；在那之前截断的内容仍然只能拒绝，接入点写在下面。

## 做了什么

**RP-12"拒绝并附一条消息"**（`request/kinds.tsx`、`request/RequestPanel.tsx`）
- 权限请求有不持久的拒绝类选项时，选项列表后多一行自由输入，占位是"否，并告诉 {agent} 应该怎么做"。这一行的编号是"选项数 + 1"，按这个数字键会把焦点移到它。计划确认没有这一行。
- 提交时先发 `interaction/respond`（选那个拒绝选项），**收到成功之后**才发 `session/send`：`mode: queue`，`reply_to` 是被拒绝请求的 id。
- 拒绝收到 `already_resolved` 或 `expired` 时，不发跟进消息，文字放回输入框，toast 提示"已在别处处理"。为此 `respond` 新增了结果 `elsewhere`，普通回答遇到这种情况也给同样的提示。
- 跟进消息自己发送失败时，文字同样放回输入框，并弹 toast。
- 自由输入行里最近 1 秒内打过字时，按 Enter 不提交，只在提交按钮旁提示"点击提交"（不提示"再按一次"）；点击提交照常生效。

**请求记录行**（`request/RequestRow.tsx`，在 `rows/index.ts` 里登记为 `request`）
- `flatten` 现在把 `RequestView` 放进 `request` 行，行组件仍然只依赖 `row`。
- 记录文字由纯函数 `record()` 决定：
  - 等待中：等待处理
  - 选了拒绝类选项：已拒绝
  - 选了其他选项：已批准：{请求标题}
  - 回答了提问：已询问 n 个问题
  - 其余（关闭、过期、取消）：未回答
- 带 `reply_to` 的用户消息：`flatten` 给它加上被拒绝请求的标题（`replyTitle`），气泡上方写"附于对「…」的拒绝"；取不到标题时写"附于一次拒绝"。
- 为此改了 T082 的 `rows/stream.ts`、`rows/flatten.ts`、`rows/basic.tsx`，各几行。

**多请求**：`ComposerArea` 把待处理请求的数目传给面板，多于一个时标题行显示"1 / n"（带 `data-request-count` 标记，文案走 i18n）。

## 关键取舍

- **先拒绝、后跟进**：拒绝收到成功才发跟进消息。这样请求已在 CLI 那边批准时，用户不会误以为自己拒绝了，文字也不会丢。
- **"点击提交"在渲染进程这边先判断**：刚打完字的 Enter 不发请求，直接提示。这样不必等主进程以缺少真实输入拒绝后，再去区分该提示"再按一次"还是"点击提交"。
- **记录行的数据放进行里**：保持 T082 的约定，行组件只依赖 `row`。
- **"等待处理"也算一种记录状态**：请求还在面板上时，消息流里那一行写"等待处理"，不留空行。

## 验证方式

- **UNIT-request-followup**（4 条，jsdom，MockHost 连真的 `HostClient`，假时钟）：
  - 自由输入行是第 5 号，按 5 会聚焦它；请求顺序是先 `interaction/respond`（`o4`），再 `session/send`（`reply_to: "r_1"`，`mode: queue`）。
  - `already_resolved` 时不发跟进消息，草稿里是原文，并显示"已在别处处理"。
  - 刚打完字按 Enter 只显示"点击提交"、不发请求；点击提交后依次发出两条请求。
  - 两个请求时显示"1 / 2"；`record()` 的各种结局各有断言。
- **UI-request-records**（亮色、暗色）：四种记录行、附于拒绝的用户消息、面板标题的"1 / 2"。
- **UI-request-panel 的五张基线变了两处**：
  - 面板上方那行占位变成"等待处理"；
  - 权限请求多了第 4 行自由输入。
  
  已重新生成并逐张看过，其他基线没有变。
- **UI-csp-zero** 为 0，T136 的规则通过。
- **审阅截图**在 `docs/tasks/T165-ui-request-followup/screenshots/`：截图页换回系统字体后截取，1280 × 820，三张：记录行的亮色与暗色，以及带自由输入行的权限请求。
- **F2**：没有查看本机的 Codex App 包。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T165-ui-request-followup/ci/`，全部通过，摘要 `pr-627f5272be1e.json` 已提交：

  | 作业 | 结果 |
  |---|---|
  | rust | 1,860 通过 |
  | protocol | 185 通过 |
  | ts | 455 通过 |
  | license | 通过 |
  | testdata-scan | 通过 |
  | ui | 23 通过 |

  e2e 作业还没实现。用 `git merge-tree` 试合并最新的 main，没有冲突。
- **规模**：347 行（实现 203，测试 144；不含 notes、lock、截图基线），开工前估计 352。

## 提交列表

```
581177f docs(T165): record the ci pr summary
627f527 docs(T165): record the work, the review screenshots and the expand that did not fit
b280b5e test(ui): add UI-request-records and refresh the request panel baselines
6a85c57 feat(ui-core): reject with a message, request records and the request count
4a0b6ff docs(T165): record the checks and the size estimate
```

## 遗留问题 / 范围外发现

- **"展开看全文"没有做**：估约 140 行（host-client 的 `getBlob` 约 50、`Terminal` 展开时下载并在看完全文后放开允许类选项约 40、用例约 50）。接入点：
  - `request/RequestPanel.tsx` 的 `Body` 把 `content_truncated` 作为 `truncated` 传给 `OptionsKind`；
  - `request/logic.ts` 的 `isEnabled(option, truncated)` 决定允许类选项能不能选；
  - `request/kinds.tsx` 的 `Terminal` 已有"展开 / 收起"，`blob` 类内容目前 `text()` 返回 `undefined`、不显示。
  
  做法：下载全文，经 `escapeInvisible` 转义后显示，看完后 `Body` 改传 `truncated={false}`。建议单列一个 S 任务，或者并进 T164（它要写对称的 `putBlob`）。
- **提问记录行展开看回答**：设计里"已询问 n 个问题"可以展开看回答，这次只写了数目，展开留给消息流的后续任务（例如 T146）。
