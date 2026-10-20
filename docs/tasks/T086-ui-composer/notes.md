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
