# T256 notes

## 开工前的估算（2026-11-10）

| 处 | 估算（新增行） |
|---|---|
| 1. 发送后回到底部：`ScrollKeeper.follow()`，UI store 记"本窗口在这个会话里发过几条"，输入框（`useSessionSend`）和排队行的"插话"记一次，消息流见到就回到底部；UNIT-stream-follow 两条 | 约 75（实现约 25，用例约 50） |
| 2. 滚动条：会话页改成全宽，横幅、消息流的内容、输入框各自按内容栏居中；滚动条样式和两个 token | 约 27 |
| 3. 底部间距：内容栏底部留 `--layout-item-gap` | 约 2 |
| 合计 | 约 105 |

在停止线 200 以内。

## 滚动条的规格

- ui-spec（R06）里没有滚动条的条目，设计 5.7.10 也没有。
- 一开始以为本机没有装 Codex 桌面 App，自定了一版（宽 10px 的 `::-webkit-scrollbar`，指针在列表上才显示）。用户更正：`/Applications/ChatGPT.app` 就是改了名的 Codex 桌面 App。于是按 F2 的规矩静态读了它的资源：只读，不启动 App，不复制任何源码进仓库。读法是用临时脚本解析 `app.asar` 的头部索引，把 `webview/assets` 下的 CSS 解到 `$TMPDIR` 的临时目录里查看，JS 在内存里检索、不落盘；看完已删除。
- 找到的（只记结论，不抄源码）：
  - **滚动条**：主样式表（`webview/assets/async/` 下约 1.2 MB、定义了 `--app-color-*` 的那一份）对所有元素统一用标准属性：`scrollbar-width: thin`，`scrollbar-color` 为"滑块色、轨道透明"。滑块色是一个变量，取正文色的 30%（oklab 混色）。可滚动的容器（包括消息流的滚动容器）在悬停或按住时，滑块色换成 `border-strong`。没有用 `::-webkit-scrollbar`，也没有自己控制显隐：要不要悬浮、什么时候显示，交给系统（macOS"滚动时显示"时是悬浮、滚动时才出现；"始终显示"时是常驻的细滚动条）。消息流的滚动容器占满会话区，内容按最大宽度居中，所以滚动条在会话区最右侧。
  - **底部间距**：消息流的滚动容器底部留出"输入框高度 + 16px"（非紧凑布局；紧凑布局是 0），由脚本量出输入框高度后写进一个 CSS 变量。也就是说最后一行和输入框之间是 16px。
- 和我原来的选择对比：
  - 底部间距一致（`--layout-item-gap`，16px），不改，ui-baseline 那次运行仍然有效。
  - 滚动条不一致，改成和 Codex 一样：去掉 `::-webkit-scrollbar` 和"只在悬停时显示"，改为标准属性。宽度用 `thin`；滑块色用新 token `--color-scrollbar-thumb`（`--color-fg` 的 30%，和 Codex 同样的混法）；悬停或按住时用已有的 `--color-border-strong`；轨道透明；显隐和是否悬浮交给系统。原来加的 `--layout-scrollbar-w` 不再需要，已删掉。截图里看不到滚动条，所以这次改动不影响基线。

## 做法

1. **发送后回到底部**：
   - UI store 加了按会话计数的 `sent`（`markSent`），记"本窗口在这个会话里发过几条"。`useSessionSend`（输入框的新回合、插话、排队，以及 RP-12 的跟进消息）和排队行的"插话"在发出时各记一次。
   - 消息流看到计数变了，就和 ↓ 按钮一样回到底部、恢复贴底。两者都走 `ScrollKeeper.follow()`：设成贴底，同时清掉最近的滚轮意图，免得回到底部途中的滚动事件被当成用户又滚开了。
   - 宿主自己开的回合（例如队列出队）不经过 `markSent`，不跳。
   - 排队行的"编辑"不算发送。
2. **滚动条**（样式见上一节）：会话页不再整体是一列。横幅、消息流的内容、输入框各自按内容栏（`--layout-thread-max-w` 加 `--layout-thread-gutter`）居中，消息流的滚动容器占满会话区，滚动条贴在会话区最右侧。内容栏的宽度和以前一样。
3. **底部间距**：内容栏的底部留 `--layout-item-gap`（16px，和条目间距同一个量）。这段间距在滚动容器里面，所以贴底判定、↓ 按钮都照常；请求面板替换输入框时同样有效。

## 截图基线的变化

本机整套截图跑了一次，7 条用例不同，对应 ui-baseline 会换的 7 张：

| 基线 | 原因 |
|---|---|
| stream-error-light、stream-error-dark | 内容停在底部：最后一张出错卡片和输入框之间多了 16px，整列往上移 16px |
| stream-late-output | 同上 |
| stream-changes-light、stream-changes-dark | 同上 |
| stream-changes-review | 同上（侧边面板打开、消息流变窄时也一样） |
| stream-changes-undo | 同上 |

内容不满一屏、或者没停在底部的截图不变。滚动条平时是透明的，截图里看不到，所以没有因为滚动条变化的图。

## 其他滚动区域（本任务不改）

用系统默认滚动条的还有：侧边栏（`shell/Shell.tsx`）、设置页（`settings/SettingsPage.tsx`）、审查文档（iframe 里，`review/document/ReviewDocument.tsx`）、审查面板的文件树（`review/FileTree.tsx`）、提交弹窗里 git 的输出（`review/GitActions.tsx`）、请求面板的全文和选项（`request/fulltext.tsx`、`request/RequestPanel.tsx`）、`@`/`/` 的弹出列表（`composer/popover.tsx`）、工具输出（`rows/tools.tsx`）。其中只有消息流有"滚动条压住内容"的问题；其余要不要换成细滚动条，可以另开任务统一处理。

## 实际规模

`cargo xtask size`：+139 −28。其中约 20 行是消息流的行渲染因为多包了一层内容栏而整体缩进。
