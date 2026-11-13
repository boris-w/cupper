# T257 notes

## 开工前的估算（2026-11-13）

| 模块 | 估算（新增行） |
|---|---|
| ui-core 的在场上报（`app/presence.ts`：窗口焦点、当前会话、连接就绪时补报、去重），App 里挂上 | 约 50 |
| 单元测试：首报、切会话、焦点变化、同样的状态不重发、连接重建后补报 | 约 60 |
| E2E-presence-suppresses-notification（`@focus`，只在 runner 上跑） | 约 50 |
| 合计 | 约 160 |

在停止线 200 以内。

## 从哪条连接发

宿主的索引（T100 的 `IndexActor`、`index/status.rs`）按连接记在场：只采信 `desktop_ui` 的、`foreground` 为真且带 `session_id` 的上报，连接关闭就忘掉。窗口的 `HostClient` 走的就是 `desktop_ui` 的窗口连接，所以从它发 `client/presence` 通知（`HostClient.notify`，T080 起就有）。主进程的控制连接不发。连接重建后是新的连接，宿主那边没有它的在场，所以要补报一次。

## 做法

- `app/presence.ts`：
  - `useFocused()` 跟着窗口的 `focus`/`blur`，初值取 `document.hasFocus()`。单独导出，T258 要在窗口回到前台时重查通知授权，可以直接用这个信号。
  - `usePresence()` 由 App 里的 `<Presence />` 挂一次。内容是 `{foreground, session_id?}`，只有在会话页时才带 `session_id`（设置页、新建会话页都不带）。焦点或页面一变就报；同一条连接上同样的内容不重发（记住上次报的内容）。
  - `HostClient` 每次 `attach` 都会发一次 `ready`，断开时发的是别的状态。所以每收到一次状态变化就清掉"上次报过的"，就绪时立即补报，新连接上一定有一次。
- 上报点集中在 `usePresence` 这一处，T258 找起来方便。

## 用例

- 单元测试（`app/presence.test.tsx`）：首报带 `session_id`；失焦、回焦、进新建会话页、再进同一页（不重发）、切到另一个会话；断开再连上后补报一次，总数正好 6 条。
- E2E-presence-suppresses-notification（`notification.spec.ts`，`@focus`，只在 runner 上跑）：
  - 第一回合没人看着，先等到它的通知意图，作为基准，此前的都已写下。
  - 打开这个会话、窗口取得焦点（确认 `document.hasFocus()`），再发一条要跑 500 ms 的消息。等索引写出"running"然后"不是 running"的摘要，意图和摘要是一起写的，这时检查没有新的意图。让回合跑过索引的 100 ms 合并窗口，是为了"running"一定会被单独写出来，否则等不到这个屏障。
  - 切到另一个会话，再给原会话发一条，等到它的意图出现。
  - 没有 sleep，只等宿主写下的事件和窗口的焦点状态。

## 实际规模

`cargo xtask size`：+182 −3。

## 评审后的修正：焦点进入审查文档的 iframe

- 问题：审查文档是独立 origin 的 iframe，焦点进去时顶层 window 会收到 `blur`。原来的 `useFocused` 一收到 blur 就置 false，于是用户在会话里写评论时上报了 `foreground: false`。
- 实测（本机无头 Chromium，临时脚本，没有入库）：父页面 `127.0.0.1`、iframe `localhost`，两者是不同的站点，所以是跨进程的 iframe。把焦点放进 iframe 的输入框后：父页面收到 `blur`；在 blur 事件里和下一个 task 里，父文档的 `document.hasFocus()` 都是 true；焦点回到父页面时收到 `focus`。Electron 的渲染进程是同一个 Chromium，规则相同。
- 修法：`focus` 时按 `document.hasFocus()` 设；`blur` 之后在下一个 task 里再用 `document.hasFocus()` 复核。窗口真正失焦时它是 false，焦点只是移进本窗口的 frame 时是 true。
- 用例：
  - 单元测试补了"blur 之后 hasFocus 仍为 true 时不上报 false"，以及 hasFocus 为 false 时照常上报。
  - E2E 在看着会话、取得焦点之后，打开侧边面板，点审查文档里文件的"收起"，确认焦点在 iframe 里（iframe 的 `hasFocus` 为 true，主文档的 `activeElement` 是 iframe），然后才发那条被看着的消息，断言照旧没有通知意图。为了让审查文档里有内容，第一回合让假 agent 改一下 README.md。
- 规模：这次修正之后是 +221 −3，超过停止线 200，编排者已允许（硬停止线 260）。
