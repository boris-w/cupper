# T086 工作笔记

## 开工前的核对

- `handoff-notes.md` 里第一列含 T086 的行：T084（插槽名、token 写法）、T080（`useStreams`、`useEntry`、`useUi()`）、T118 / T081 / T137（截图用例、按流名给 op 的 `streams` 夹具）、T071（`HostClient` 还没有上传 blob 的方法）。
- `tests/ids.toml` 的 T086 段已有 UNIT-composer-ime、UNIT-composer-long-text、UNIT-composer-esc、UI-composer。
- 协议：`session/send {session_id, mode, content: PromptBlock[]}` → `{disposition: started | queued | steered, fallback_reason?}`；M1a 没有 Steer，`mode` 一律 `queue`（空闲时即开始回合）。新建会话是 `session/create {agent, project_id, workspace, message}`。停止是 `session/interrupt`，继续队列是 `session/queue_resume`。队列在会话视图的 `queue`（`entries[].preview`、`paused.reason`）。
- 长文本：协议 5.3.4"单个 `text` 块超过帧预算时，先 `blob/put` 再用 `text_blob` 引用"；`blob/put` 之前先经 `0x02` 分片发内容。`ShellAPI` 的端口已有 `sendBlob`，`HostClient` 还没有对应的上传方法；宿主的 `blob/put` 由 T029 做。
- 附件来源：图片（"+"）、`@` 文件引用、审查评论都是 M1b 的；M1a 的附件条只负责显示和删除已有的附件。

## 规模估计（开工前）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 输入框外形与文字区 | 圆角 22、底色与阴影、文字区自动增高（40 到 25dvh）、按 agent 和运行状态的占位；草稿按会话存在 `useUi()` | 90 | — |
| 按键 | Enter / ⇧Enter / ⌥Enter；"发送快捷键"偏好（Enter 或 ⌘Enter）；输入法组合中不发送（`isComposing`、keyCode 229） | 40 | — |
| Esc | 处理顺序（先关浮层，运行中第一次进入"再按一次 Esc 停止"、第二次 `session/interrupt`，别的键或失焦退出；焦点不在输入框时移回）与提示 | 50 | — |
| 发送 | 组装内容块（文字加附件）、`session/send`（一律 `queue`）、新建会话页走 `session/create` 后导航；成功清空草稿与附件，失败保留并 toast | 70 | — |
| 发送按钮三态 | 不可用、发送、停止；`data-send-state` 等 DOM 标记 | 35 | — |
| 扩展点 | 底栏左右两组插槽（`import.meta.glob` 收 `composer/extensions/` 下的文件，M1b 只新增文件）、弹出层挂载点 | 25 | — |
| 附件条 | 按块类型显示（图片、文件引用、审查评论）与删除 | 45 | — |
| 排队消息 | 预览、暂停提示与"继续"（`session/queue_resume`） | 60 | — |
| 接线与文案 | 两个插槽的默认值、会话页取会话 id；i18n | 40 | — |
| 长文本（等 T029） | `HostClient` 的 blob 上传（sha-256、`0x02` 分片、`blob/put`）；超过帧预算时走 `text_blob` | 40 | — |
| UNIT-composer-ime / -esc / -long-text | 按键判定、Esc 状态机与两次停止、长文本的上传与引用；发送流程 | — | 170 |
| UI-composer | 三态、附件条、排队与暂停；两种主题（会话流夹具给 `queue`、索引给运行状态） | — | 70 |
| 合计 | | 495 | 240 |

合计约 **735 行**。以往实际比估计多 0–17%（T081 是 +17%），推算约 735–860，偏上时贴近约 900 的停止线。

### 拆分建议（如需）

长文本那一项本来就要等 T029，可以拆成一个小任务：`HostClient` 的 blob 上传、输入框的 `text_blob` 路径、UNIT-composer-long-text，约 110 行（实现 40、测试 70）。T086 留下其余约 625 行（推算 625–730），和 T029 解耦，做完就能交；小任务在 T029 合并后接着做。

不拆的话，按现在的顺序：先做其余部分，T029 合并后再补长文本，每个模块完成后统计，推算到 900 就停。

### 附件条的截图

M1a 没有产生附件的入口（图片、`@`、审查评论都在 M1b），截图页又拿不到 `useUi()` 的 store。UI-composer 里的"附件条"要么给截图页一个预置附件的办法（例如 `App` 接受初始的 UI 状态，和已有的 `initialRoute` 同类），要么只在 Vitest 里覆盖、截图留给第一个产生附件的 M1b 任务。倾向后者，请定。

## 进度（T029 合并之前）

不依赖 T029 的部分已完成并提交，改动约 715 行（实现约 507，测试约 208）。长文本那一项（`HostClient` 的 blob 上传、`text_blob` 路径、UNIT-composer-long-text）估约 110 行，推算合计约 825。

- **输入框**（`composer/Composer.tsx`）：附件条、文字区（`field-sizing: content`，40 到 25dvh）、底栏和发送按钮三态（`data-send-state`：`disabled`、`send`、`stop`）。占位按 agent 和运行状态；新建会话页没有项目时写"先添加一个项目"，发送不可用。草稿和附件按会话存在 `useUi()`，新建会话页的键是 `new`。
- **按键**（`composer/keys.ts`，纯函数）：`sends(event, sendKey)` 与 `escapeAction(state)`。偏好键 `composer.sendKey`（`enter` 或 `mod_enter`），经 `prefs.onChange` 跟随设置页（T036）的改动。
- **发送**（`composer/send.ts`）：会话里 `session/send`（`mode: queue`）；新建会话页 `session/create`（agent `claude`，本地工作区，项目取路由里的 `projectId`，没有就取第一个项目），成功后导航到新会话。发送时先清空草稿和附件，失败时如果这期间没再输入就放回去，并 toast 失败码。停止是 `session/interrupt`，继续是 `session/queue_resume`。"运行中"按索引里的 `status`：`running` 或 `needs_action`。
- **排队消息**（`composer/QueueList.tsx`）：会话视图的 `queue`，每条显示预览的第一行和图片数、评论数；暂停时显示原因和"继续"。`ComposerArea` 是会话页插槽的默认值：排队消息在上，输入框在下。T083 在 `ComposerArea` 里按是否有待处理的请求选择请求面板或输入框，草稿和附件都在 store 里，切回来原样。
- **扩展点**（`composer/extensions.ts`）：`import.meta.glob` 收 `composer/extensions/*.tsx` 的默认导出（`side`、`order`、`Component`），放进底栏左右两组；弹出层挂载点和 `OverlayContext`（`container`、`setClose`），打开的浮层登记自己的关闭函数，Esc 先调它。
- **测试**：UNIT-composer-ime（判定函数，和组件里输入法选词的 Enter 不发送、之后的 Enter 发送）、UNIT-composer-esc（处理顺序，运行中两次 Esc 发 `session/interrupt`，别的键和失焦解除，空闲时 Esc 无动作，焦点在外面时 Esc 移回输入框）、附件条（显示、删除、随消息发出）。组件测试用 MockHost 连一个真的 `HostClient`，经导出的 `HostContext` 放进组件树，用真的 `StateProvider` 订阅索引。
- **截图**：UI-composer（暂停的队列加待发送的草稿，两种主题；运行中的停止态）。其余基线（UI-shell、UI-sidebar、UI-banners、UI-session-banners）因为每页都多了输入框，只在主区有变化，重新生成后看过。UI-csp-zero 为 0。
- 没做：队列暂停时再发新消息的"发送消息？"确认框（设计里 M1 有，任务书没列）；附件条的截图（见上文"附件条的截图"，等裁定）。

## 裁定后补做：队列暂停时再发送

- 裁定：附件条只在 Vitest 里覆盖，截图留给 M1b 第一个能产生附件的任务；"发送消息？"确认在本任务做，用界面里的确认，不走原生确认框。
- 做法：队列暂停时按发送，输入框上方先出一个警告框（T128 的 `Alert`）："发送"发出消息、成功后 `session/queue_resume`；"取消"或 Esc 收起并保留草稿。Esc 的处理顺序里它算作浮层。输入框经 `ComposerArea` 得到队列是否暂停（`ComposerArea` 读会话视图，排队消息改为按 props 画）。
- 用例：先取消（不发送、草稿还在），再发送（一条 `session/send`，随后一条 `session/queue_resume`）。
- 规模：这一项约 76 行，现在合计 **791 行**。长文本估约 110 行，合计推算约 900，贴线，按裁定停下。
