# T257 汇报

- 执行者：子 agent（前端，ui-core）
- 分支：`fix/T257-client-presence`
- 完成日期：2026-11-13

**CI 状态（交接时）**：
- PR #110 https://github.com/boris-w/cupper/pull/110 ，运行 https://github.com/boris-w/cupper/actions/runs/43381301003 ：8 个检查全部通过。
- e2e-repeat（spec=src/notification.spec.ts，test=E2E-presence-suppresses-notification，times=10）：https://github.com/boris-w/cupper/actions/runs/43381298828 ，`10 passed`。
- 之前那次 43379694100 测的是评审修正前的提交，已取消。

## 做了什么

1. **在场上报**（4cc37629，新文件 `app/presence.ts`，由 App 里的 `<Presence />` 挂一次）：
   - 从哪条连接发：宿主的索引（T100，`index/status.rs`）按连接记在场，只采信 `desktop_ui` 的、`foreground` 为真且带 `session_id` 的上报，连接关闭就忘掉。窗口的 `HostClient` 走的正是 `desktop_ui` 的窗口连接，所以经 `HostClient.notify` 发 `client/presence {foreground, session_id?}`。只在会话页带 `session_id`，设置页和新建会话页都不带。
   - 什么时候发：窗口焦点变化、页面变化时发一次；同一条连接上同样的内容不重发。
   - 连接重建：`HostClient` 每次 `attach` 都会发一次 `ready`，所以每次状态变化都清掉"上次报过的"、就绪时立即补报，新连接上一定有一次。
   - 给 T258：焦点信号 `useFocused()` 单独导出，可以直接用来"窗口回到前台时重查通知授权"；上报只在 `usePresence` 这一处。
2. **评审后的修正：焦点进入审查文档的 iframe**（b5c26a0c）：
   - 问题：审查文档是独立 origin 的 iframe，焦点进去时顶层 window 会收到 `blur`，原来会因此上报 `foreground: false`。
   - 实测：本机无头 Chromium 跑了个临时脚本（没有入库）。父页面在 `127.0.0.1`，iframe 在 `localhost`，两者是不同站点，所以是跨进程的 iframe。焦点移进 iframe 时父页面收到 `blur`，但父文档的 `document.hasFocus()` 在 blur 事件里和下一个 task 里都是 true；焦点回来时收到 `focus`。Electron 的渲染进程是同一个 Chromium，规则相同。
   - 修法：`focus` 时按 `document.hasFocus()` 设；`blur` 之后在下一个 task 里用 `document.hasFocus()` 复核，计时器在卸载时清掉。这样焦点在本窗口任何 frame 里都算前台，T258 拿到的信号也准了。
3. **用例**：
   - 单元测试 `app/presence.test.tsx`：首报带 `session_id`；blur 之后 hasFocus 仍为 true 时不上报 false；真正失焦上报 false；回焦；进新建会话页时不带 `session_id`；同一状态不重发；切会话；断开重连后补报一次（总数正好 6 条）。
   - E2E-presence-suppresses-notification（notification.spec.ts，`@focus`，只在 runner 上跑），record 模式：
     - 先等第一回合（没人看着）的通知意图，作为基准，此前的都已写下。
     - 打开这个会话、取得焦点（确认 `document.hasFocus()`）；再打开侧边面板，点审查文档里文件的"收起"，确认焦点在 iframe 里（iframe 的 `hasFocus` 为 true，主文档的 `activeElement` 是 IFRAME）。为了让审查文档里有内容，第一回合让假 agent 改一下 README.md。
     - 发一条要跑 500 ms 的消息。等索引依次写出这个会话"running"和"不是 running"的摘要（意图和摘要是一起写的），这时断言没有新的通知意图。让回合跑过索引 100 ms 的合并窗口，是为了"running"一定会被单独写出来，否则等不到这个屏障。
     - 切到另一个会话，再给原会话发一条，等到它的通知意图出现。
     - 没有 sleep，只等宿主写下的事件和窗口的焦点状态。

## 关键取舍

- **在场放在一个不渲染内容的组件里，挂在 App 根部**：它需要宿主连接和 UI store 里的当前页面，放在 StateProvider 里面正好两样都有，而且全应用只有一处。
- **按"每次状态变化都清掉上次报过的"做补报**：不用自己判断是不是新连接，`HostClient` 每次 attach 都会发 ready，这就够了。
- **iframe 的焦点用 `document.hasFocus()` 复核**，不去跟 iframe 互通消息：浏览器本身就给出"本窗口有没有焦点"的答案，实测也确认了跨进程 iframe 的情况。

## 验证方式

- **本机**：ui-core 的 vitest 全部 156 条通过，tsc、eslint、prettier 通过。e2e 只在 runner 上跑（守则第 10 条）。
- **对照验收标准**：
  - [x] E2E-presence-suppresses-notification 通过，在 runner 上连跑 10 次全部通过。
  - [x] 渲染进程的约束都守住，协议和宿主都没改。
  - [x] PR #110 的 8 个检查在 829e3d30 上全部通过。
- **规模**：开工前估约 160，实际 +221 −3（`cargo xtask size`）。评审修正（iframe 焦点和对应的用例）之后超过停止线 200，你已允许，在硬停止线 260 以内。

## 提交列表

```
829e3d30 docs(task): T257 notes: the frame's blur, probed
b5c26a0c fix(ui-core): the focus in a frame of the window keeps it in front
29ee19e4 docs(task): T257 notes: the estimate, the connection, the barriers
4b5fb92b test(e2e): E2E-presence-suppresses-notification
4cc37629 feat(ui-core): the window tells the host where it is
```

## 遗留问题 / 范围外发现

1. **给 T258**：用 `app/presence.ts` 的 `useFocused()` 作为"窗口回到前台"的信号。焦点只是在本窗口的 frame 之间移动时，它不会变化。
2. **只看了窗口失焦和切会话**：窗口最小化、被别的窗口挡住但仍有焦点这类情况，按的都是系统给的焦点；最小化时窗口会失焦，所以按"不在前台"处理。
