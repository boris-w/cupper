# T165 工作笔记

## 开工前的核对

- `handoff-notes.md` 里第一列含 T165 的行：T082（行的形状以 `rows/stream.ts` 为准，`registerRow(kind, Component)`，在 `rows/index.ts` 加 import；`request` 行在登记前显示占位，UI-request-panel 的基线里就是这样）。任务书另列 T083 的接入点、T029 / T164 的 blob 约定、T070 的不可见字符写法。
- `tests/ids.toml` 的 T165 段：UNIT-request-followup、UI-request-records。
- `request` 行现在只有 `requestId`，`user_message` 行有 `replyTo`（被拒绝的请求 id）。行组件要是 `row` 的纯函数，所以记录行需要的数据（请求和它的结局）、气泡上"附于对 … 的拒绝"的说明文字，都要在 `flatten` 里放进行里。

## 规模估计（开工前）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| RP-12 | 自由输入行（编号为选项数 + 1，数字键聚焦）；先发不持久的拒绝，成功后才发跟进消息（`reply_to`，一律排队）；`already_resolved`、`expired` 时文字放回输入框并提示"已在别处处理"；刚打完字 1 秒内的 Enter 只提示"点击提交" | 85 | — |
| 请求记录行 | `flatten` 把请求和结局放进 `request` 行；登记 `request`："已批准：…""已拒绝""已询问 n 个问题""未回答"，待处理时一行"等待处理" | 60 | — |
| 附于拒绝的标注 | `flatten` 给 `user_message` 加被拒请求的标题；气泡上方"附于对「…」的拒绝" | 15 | — |
| 多请求 | 面板标题行的"1 / 3" | 10 | — |
| 文案 | i18n | 12 | — |
| UNIT-request-followup | RP-12 的顺序、`reply_to`、别处已处理、编号与数字键、"点击提交"、"1 / 3" | — | 120 |
| UI-request-records | 记录行的三种状态、多请求的面板标题，两种主题 | — | 50 |
| 合计（不含展开） | | 182 | 170 |

不含"展开看全文"约 **350 行**。展开（host-client 的 `getBlob`、`Terminal` 展开时下载、看完全文才放开允许类选项，加用例）估约 140 行，合计约 490，超过约 450 的停止线。先做前面几项，做完按实际行数决定展开放不放得下，放不下就在汇报里提出。

## 实现记录

- **RP-12**（`request/kinds.tsx` 的 `OptionsKind`、`request/RequestPanel.tsx` 的 `rejectWithMessage`）：权限请求有不持久的拒绝类选项时，选项列表后多一行自由输入，编号为选项数 + 1，按这个数字聚焦它；计划确认没有这一行。提交时先 `interaction/respond`（那个拒绝选项），成功后才 `session/send`（`mode: queue`，`reply_to` 为请求 id）；跟进消息发送失败时文字放回输入框并 toast。`respond` 新增结果 `elsewhere`（`already_resolved`、`expired`）：toast"已在别处处理"，RP-12 时文字放回输入框、不发跟进。自由输入行里最近 1 秒内打过字时，Enter 只在提交按钮旁提示"点击提交"，点击提交照常。
- **请求记录行**（`request/RequestRow.tsx`，在 `rows/index.ts` 里 import）：`flatten` 把 `RequestView` 放进 `request` 行；记录文字由纯函数 `record()` 决定：待处理"等待处理"，选了拒绝类"已拒绝"，选了其他"已批准：{请求标题}"，回答了提问"已询问 n 个问题"，其余（关闭、过期、取消）"未回答"。
- **附于拒绝**：`flatten` 给带 `reply_to` 的用户消息加 `replyTitle`（权限请求的标题或提问的问题），气泡上方写"附于对「…」的拒绝"，没有标题时写"附于一次拒绝"。改了 T082 的 `rows/stream.ts`、`rows/flatten.ts`、`rows/basic.tsx` 各几行。
- **多请求**：`ComposerArea` 把待处理请求的数目传给面板，多于一个时标题行显示"1 / n"（`data-request-count`，文案走 i18n）。
- **截图**：UI-request-records（记录行四种、附于拒绝的用户消息、"1 / 2"，两种主题）。UI-request-panel 的五张基线变了两处：面板上方那行占位变成"等待处理"；权限请求多了第 4 行自由输入。重新生成后逐张看过。
- **审阅截图**：`screenshots/`，截图页换回系统字体后截取，1280 × 820：记录行亮色、暗色，带自由输入行的权限请求。按 F2，没有查看本机 Codex App 包。

### 行数

| 模块 | 估计 | 实际 |
|---|---|---|
| RP-12（含 `respond` 的 `elsewhere`） | 85 | 98 |
| 请求记录行（含 `flatten`、行类型） | 60 | 60 |
| 附于拒绝的标注 | 15 | 23 |
| 多请求 | 10 | 9 |
| 文案 | 12 | 13 |
| UNIT-request-followup | 120 | 95 |
| UI-request-records | 50 | 49 |
| 合计 | 352 | **347** |

### "展开看全文"没有做

加上它（host-client 的 `getBlob` 约 50、`Terminal` 展开时下载并在看完后放开允许类选项约 40、用例约 50）约 140 行，合计约 490，超过约 450 的停止线。接入点见 T083 的 notes"展开看全文的接入点"：`Body` 传给 `OptionsKind` 的 `truncated`、`logic.ts` 的 `isEnabled`、`kinds.tsx` 的 `Terminal`。在那之前保持"只能拒绝"。
