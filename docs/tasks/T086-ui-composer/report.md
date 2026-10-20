# T086 汇报（ui-core VII：输入框与排队消息，M1a）

- 执行者：子 agent（前端工程师）
- 分支：`feat/T086-ui-composer`
- 完成日期：2026-10-20

输入框和排队消息已完成，接到了 T084 的两个输入框插槽上；长文本按裁定拆给了 T164。UNIT-composer-ime、UNIT-composer-esc、UI-composer 通过，UI-csp-zero 为 0。在干净的 worktree 里跑 `cargo xtask ci pr --record` 通过，摘要已提交。规模 791 行，在约 900 的停止线以内。

## 做了什么

**输入框**（`packages/ui-core/src/composer/Composer.tsx`）
- 接到 `newSessionComposer`、`sessionComposer` 两个插槽，从上到下是附件条、文字区、底栏。
- 外形：圆角 22，底色 `--color-surface-composer`，阴影 `--shadow-composer`。
- 文字区用 `field-sizing: content`，高度在 40 到 25dvh 之间随内容伸缩，行高 1.625，左右内边距 12，占位文字透明度 50%。
- 占位文案：会话里写"和 {agent} 一起工作"；运行中写"处理中…"；新建会话页写"描述你要做的事"，没有项目时写"先添加一个项目"，这时发送不可用。
- 发送按钮三态，用 `data-send-state` 标出：
  - `disabled`：空输入且没有运行中的回合，半透明；
  - `send`：主按钮色圆形，向上箭头；
  - `stop`：运行中且输入为空，实心方块，点击发 `session/interrupt`。
- 另有两个 DOM 标记供 e2e 使用：`data-composer`、`data-esc-armed`。
- 草稿和附件按会话存在 `useUi()`，新建会话页的键是 `new`。

**按键**（`composer/keys.ts`，纯函数）
- Enter 发送，⇧Enter 和 ⌥Enter 换行。
- 偏好 `composer.sendKey` 设为 `mod_enter` 时，改成 ⌘Enter 发送；偏好变化会跟随设置页。
- 输入法组合过程中不发送（`isComposing` 或 keyCode 229），包括用 Enter 选词。
- M1a 没有 Steer，所以 ⌘Enter、⌘⇧Enter 这些排队键也直接发送。
- Esc 的处理顺序：
  1. 先关闭浮层；
  2. 回合运行中按第一次，提示"再按一次 Esc 停止"，按第二次发 `session/interrupt`；中间按别的键或输入框失焦，就退出这个状态；
  3. 焦点不在输入框（也不在别的输入框、菜单、对话框）时，Esc 把焦点移回输入框。

**发送**（`composer/send.ts`）
- 会话里调 `session/send`，`mode` 一律是 `queue`；空闲时这条消息直接开始回合。
- 新建会话页调 `session/create`：agent 为 Claude、本地工作区，项目取路由里的项目，没有就取第一个项目。建好后导航到新会话。
- 发送时先清空草稿和附件。失败时，如果这期间没有再输入，就把原内容放回去，并弹 toast 显示失败码。

**排队消息**（`composer/QueueList.tsx`）
- 每条显示预览的第一行，以及图片数、评论数。
- 队列暂停时显示原因和"继续"按钮（`session/queue_resume`）。
- 队列暂停时再发送，输入框上方先出一个警告框（T128 的 `Alert`）："发送"会发出消息，成功后再让队列继续；"取消"或 Esc 收起警告框，草稿保留。这是界面内的确认，不走原生确认框。

**扩展点**（`composer/extensions.ts`）
- 底栏左右两组：用 `import.meta.glob` 收集 `composer/extensions/*.tsx` 的默认导出（`side`、`order`、`Component`），M1b 的任务只需新增文件。
- 弹出层挂载点和 `OverlayContext`：浮层打开时登记自己的关闭函数，Esc 先调它。

**给 T083 的结构**
- 会话页插槽的默认值是 `ComposerArea`：上面是排队消息，下面是输入框，暂停状态由它读会话视图后传给输入框。
- T083 在 `ComposerArea` 里按有没有待处理的请求，选择显示请求面板还是输入框。草稿和附件都在 store 里，切回来原样还在。

**其他**
- 新增 `composer` 文案命名空间和两个自绘图标（向上箭头、方块）。
- `HostContext` 改为导出，只改了这一行，供组件测试注入连着 MockHost 的真 `HostClient`。

## 关键取舍

- **文字区自动增高用 CSS**：用 `field-sizing: content`，不用 JS 去量高度，Electron 的 Chromium 支持它。
- **草稿先清空、失败再放回**：发送过程中用户如果又输入了新内容，失败时不会覆盖这部分新内容。
- **"运行中"看索引里会话的 `status`**：`running` 或 `needs_action` 都算，后者是回合在等审批。
- **附件条不截图**：按裁定只在 Vitest 里覆盖，截图留给 M1b 第一个能产生附件的任务，也没有给 `App` 加测试专用的入口。
- **长文本拆给 T164**：按裁定，本任务不判断帧预算，也不上传 blob。

## 验证方式

- **UNIT-composer-ime**（2 条）：
  - 判定函数：Enter、⇧Enter、⌥Enter、⌘Enter 在两种发送快捷键设置下的结果，以及 `isComposing` 和 keyCode 229 时不发送；
  - 组件：输入法选词的 Enter 不发出请求，之后的 Enter 发出一条 `session/send`（`mode: queue`），草稿清空。
- **UNIT-composer-esc**（3 条）：
  - 处理顺序：浮层、进入确认、停止、移回焦点、无动作五种结果；
  - 运行中按两次 Esc 发出 `session/interrupt`；中间按别的键或失焦会解除，之后要重新按两次；
  - 焦点在外面时 Esc 把焦点移回输入框；空闲时 Esc 没有动作。
- **另外两条单元测试（不带编号）**：
  - 附件条：显示附件、删除一个、剩下的随消息一起发出；
  - 队列暂停时发送：先点取消（不发请求，草稿还在），再点发送（发出 `session/send`，之后 `session/queue_resume`）。
  - 组件测试用 MockHost 连一个真的 `HostClient`，经真的 `StateProvider` 订阅索引流和会话流。
- **UI-composer**（3 张）：暂停的队列加一条可发送的草稿（亮色、暗色），运行中的停止态。
- **其余基线**（UI-shell、UI-sidebar、UI-banners、UI-session-banners）因为每页都多了输入框而变化，差异只在主区。已重新生成并逐张看过，`meta.json` 没有变。
- **UI-csp-zero** 为 0，T136 的规则通过。
- **审阅截图**在 `docs/tasks/T086-ui-composer/screenshots/`：截图页换回系统字体后截取，1280 × 820，三张与 UI-composer 相同的场景。
- **F2**：没有查看本机的 Codex App 包。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T086-ui-composer/ci/`，全部通过，摘要 `pr-980fd2d94a4d.json` 已提交：

  | 作业 | 结果 |
  |---|---|
  | rust | 1,825 通过 |
  | protocol | 185 通过 |
  | ts | 431 通过 |
  | license | 通过 |
  | testdata-scan | 通过 |
  | ui | 16 通过 |

  e2e 作业还没实现。用 `git merge-tree` 试合最新的 main，没有冲突。
- **规模**：791 行（实现约 545，测试约 246；不含 notes、lock、截图基线），在约 900 的停止线以内。

## 提交列表

```
6282544 docs(T086): record the ci pr summary
980fd2d docs(T086): record the T164 access points and the review screenshots
e8efa5c Merge branch 'main' into feat/T086-ui-composer
d26dbeb docs(T086): record the paused-queue confirmation and the size
691e898 feat(ui-core): ask before sending while the queue is paused
06a9171 Merge branch 'main' into feat/T086-ui-composer
c6918f4 docs(T086): record the progress before T029
a12b1c0 test(ui): add UI-composer and refresh the baselines for the composer
ea6feb6 feat(ui-core): add the composer and the queued messages
8531717 docs(T086): record the checks, the size estimate and a split option
```

两个合并提交是你把 main 合进来的。

## 遗留问题 / 给后续任务的接入点

**给 T164（长文本）**
- **在哪里判断"超过帧预算"**：`packages/ui-core/src/composer/send.ts` 的 `useSend().send(text, attachments)`。这是唯一组装内容块的地方（文字在前、附件在后），新建会话的 `session/create`（`message`）和会话里的 `session/send`（`content`）都经过它。预算可以取握手结果的 `limits.max_frame_bytes`（`useHost().handshake`），按整条请求 JSON 的 UTF-8 长度判断，并给 JSON-RPC 外壳留出余量。
- **在哪里换成 `text_blob`**：同一个函数里，组装完内容块之后、调用 `client.command(...)` 之前。
  - 先把超出预算的 `text` 块上传：`HostClient` 新增上传方法，算 sha-256，经端口的 `sendBlob` 发 `0x02` 分片，再调 `blob/put {hash, size, media_type}`。
  - 然后把这个块换成 `{type: "text_blob", blob: {hash, media_type, size}}`。
  - 上传失败走现有的失败路径：草稿放回，弹 toast 显示失败码。
- 发送按钮和草稿逻辑都不用改。
- 组件测试可以沿用 `composer/composer.test.tsx` 的 `setup()`：`requests(method)` 能取到 `blob/put` 和 `session/send` 的参数，MockHost 收到的分片在 `conn.frames` 里。

**给其他任务**
- **T083**：在 `ComposerArea`（`composer/QueueList.tsx`）里按有没有待处理的请求，选择请求面板还是 `Composer`；排队消息保持在两者上方。
- **M1b（T140、T141、T142、T145、T149）**：
  - 底栏的项目放进 `composer/extensions/` 作为新文件；
  - 浮层挂到 `OverlayContext.container`，打开时用 `setClose` 登记关闭函数；
  - 附件条已能显示 `image`、`file_ref`、`review_comment` 三类块，附件条的截图留给其中第一个能产生附件的任务。
- **"运行中按 Enter 插话"不在 M1a 范围**：M1a 一律排队；插话（`mode: steer`）、"已转为排队"的提示、Claude 第一次插话时的提醒，都归 T140。