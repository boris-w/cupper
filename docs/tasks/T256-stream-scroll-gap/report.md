# T256 汇报

- 执行者：子 agent（前端，ui-core）
- 分支：`fix/T256-stream-scroll-gap`
- 完成日期：2026-11-10

**CI 状态（交接时）**：
- PR #108 https://github.com/boris-w/cupper/pull/108 ，运行 https://github.com/boris-w/cupper/actions/runs/42796184992 ：8 个检查全部通过。
- ui-baseline：https://github.com/boris-w/cupper/actions/runs/42793933247 。

## 做了什么

1. **发送后回到底部**（009a7f3d）：
   - UI store 按会话记"本窗口在这个会话里发过几条"（`sent`、`markSent`）。以下几种发送在发出时各记一次：
     - 经 `useSessionSend` 的：输入框的新回合、插话、排队，以及 RP-12 的跟进消息；
     - 排队行上的"插话"。
   - 消息流看到计数变了，就和 ↓ 按钮一样回到底部、恢复贴底。两者都走新加的 `ScrollKeeper.follow()`：设为贴底，同时清掉最近的滚轮意图，免得回到底部途中的滚动被当成用户又滚开了。这是 T245 意图机制的反方向，没有另起一套。
   - 宿主自己开的回合（例如队列出队）不跳；排队行的"编辑"也不算发送。
2. **滚动条**（5a14b164，a4d9fbb4）：
   - 会话页不再整体是一列：横幅、消息流的内容、输入框各自按内容栏居中（宽度和以前一样）。消息流的滚动容器占满会话区，滚动条贴在最右侧，不再压住行尾。
   - 样式和 Codex 一致，只用标准属性：`scrollbar-width: thin`，轨道透明，滑块色为新 token `--color-scrollbar-thumb`（`--color-fg` 的 30%，oklab 混色）；悬停或按住时滑块换成已有的 `--color-border-strong`。是否悬浮、何时显示交给系统。
3. **底部间距**（5a14b164）：内容栏底部留 `--layout-item-gap`（16px），和 Codex 的量一样。请求面板替换输入框时同样有效，输入框的阴影不再压住内容。
4. **用例**：
   - UNIT-stream-follow 补了两条：往上翻之后用户发消息，回到底部并继续贴底；往上翻之后宿主开回合，不动。原有用例不变。
   - composer 的测试里也检查了输入框的几种发送和排队行的"插话"都会记数。
5. **基线**（f27f1fa5）：换了 7 张，原因见下一节。
6. **notes.md**：估算、滚动条和间距的规格来源（Codex 资源里查到的结论）、每张基线的原因、其他滚动区域。

## 关键取舍

- **滚动条和间距按 Codex 的实际做法**：
  - ui-spec 和设计 5.7.10 都没有滚动条的规格。用户更正 `/Applications/ChatGPT.app` 就是 Codex 桌面 App 之后，我按 F2 的规矩静态读了它的资源：
    - 用临时脚本解析 `app.asar` 的头部索引，CSS 解到 `$TMPDIR` 查看，JS 在内存里检索、不落盘；
    - 没有启动 App，没有复制源码进仓库，解出的文件已删除。
    - notes 里只记结论和位置：`webview/assets/async/` 下定义了 `--app-color-*`、约 1.2 MB 的主样式表。
  - 滚动条：一开始我自定的一版（`::-webkit-scrollbar`，只在悬停时显示）和 Codex 不一致，已改为和它一样，原来加的 `--layout-scrollbar-w` 也删掉了。
  - 底部间距：Codex 是输入框高度加 16px，最后一行到输入框正好 16px，和我的选择一致，没有改。
- **"发过几条"放在 UI store**，而不是让消息流去猜哪些新消息是用户发的：这样宿主开的回合天然不触发，排队行的"插话"也能用同一个入口。

## 验证方式

- **本机**：ui-core 的 vitest 155 条全部通过，tsc、eslint、prettier 通过。截图整套跑过两次（改滚动条前后），都是 79 条通过、7 条不同。
- **基线核对**：ui-baseline 42793933247 的产物下载到新的空目录。82 个文件，文件列表一致；逐字节比对只有以下 7 张不同。看过新旧图，每张都只是整列上移 16px、最后一行和输入框之间多出间距。只提交了这 7 张。滚动条在截图里看不到，改滚动条的提交不影响基线。

| 基线 | 原因 |
|---|---|
| stream-error-light、stream-error-dark | 内容停在底部，最后一行和输入框之间多了 16px，整列上移 16px |
| stream-late-output | 同上 |
| stream-changes-light、stream-changes-dark | 同上 |
| stream-changes-review | 同上（侧边面板打开、消息流变窄时） |
| stream-changes-undo | 同上 |

- **对照验收标准**：
  - [x] UNIT-stream-follow 的新用例通过，原有用例不变。
  - [x] 截图基线只由 ui-baseline 生成，变化的 7 张都有原因。
  - [x] 渲染进程的约束：颜色和尺寸走 token，没有写死像素。新 token 只有 `--color-scrollbar-thumb`，你会在合并时补进设计 5.7.10。
  - [x] PR #108 的 8 个检查在 f27f1fa5 上全部通过。
- **规模**：开工前估约 105（发送后回到底部约 75，滚动条约 27，间距约 2），实际 +135 −28，在停止线 200 以内。多出来的约 20 行是消息流的行渲染因为多包了一层内容栏而整体缩进。

## 提交列表

```
f27f1fa5 test(ui-screenshots): the stream's shots with room above the composer
a4d9fbb4 fix(ui-core): the stream's scrollbar as Codex has its thread's
15210dc6 docs(task): T256 notes: the estimate, the scrollbar's spec, the shots
5a14b164 fix(ui-core): the stream's scrollbar at the session area's edge; room above the composer
009a7f3d fix(ui-core): a message the user sends brings the stream back to its end
```

## 遗留问题 / 范围外发现

1. **其他用系统默认滚动条的滚动区域**（本任务不改）：
   - 侧边栏、设置页、审查文档（iframe）、文件树；
   - 提交弹窗里 git 的输出；
   - 请求面板的全文和选项；
   - `@`/`/` 的弹出列表、工具输出。
   其中只有消息流有"滚动条压住内容"的问题。Codex 是对所有元素统一设细滚动条，cupper 要不要也统一，可以另开任务。
2. **notes.md 里滚动条一节的记录**：先记了"本机没装 Codex"，后面接着写了用户更正后读到的结果，两段都保留了，以便看出改动的来由。
