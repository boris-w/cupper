# T082 工作笔记

- 执行者：子 agent（工程师；T077、T134、T136、T135、T029、T164 的执行者接手）

## 开工前核对现状

- **会话视图**（host-client 的 `SessionView`，T072）：
  - JSON 是 `{turns: [{started, entries, completed?, snapshots}], tail, items, requests, detached, late_by_ref, unknown…}`；
  - 条目是 `{item}` 或 `{request}`；
  - `ItemView` 是 `{item, text, output, end?}`，`item.kind` 取 `user_message`、`agent_message`、`thought`、`tool_call`、`late_tool_output`、`plan`、`notice`、`error`，以及未知种类。
- **历史翻页**：host-client 没有 `history/page` 的辅助函数。结果是 `{ops, next?}`，要逐个 `apply` 进视图；视图会把更早的回合插在前面。"加载更早的历史"要自己发请求并喂给视图，约 30 行。
- **行类型注册表**：T128 只有类 `RowRegistry` 和 `RowView`，还没有共用实例。本任务先开工，就由本任务在 `rows/` 下建（brief 的约定），T165 照用。
- **依赖**（都要新加，锁精确版本）：
  - `streamdown` 2.7.0（2026-09-30 发布，Apache-2.0）：2.x 把代码高亮、Mermaid、数学公式拆成了单独的插件包，主包的依赖是 unified/remark/rehype 一族、`rehype-raw`、`rehype-sanitize`、`rehype-harden`、`marked`；关掉 `rehype-raw` 要显式给出 `rehypePlugins`；
  - `@tanstack/react-virtual` 3.14.x（MIT）。
- **截图**：照 `sidebar.spec.ts` 的写法，用 `open()` 带上流的夹具。

## 开工前的规模估计

行数指格式化后的新增行，含注释和空行。最近几个任务的实现是草估的 1.0–1.1 倍，按 1.1 算。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| 拍平成行 | 回合标题 → 条目（消息、思考、工具、提示、错误），尾部；同一 `group_id` 里连续的工具合成活动组行，折叠时只占一行；未知事件的占位；"加载更早的历史"行 | 150 |
| 共用的行注册表实例 | `rows/` 下的实例与各种行的登记 | 30 |
| 行列表与滚动 | TanStack Virtual 按行虚拟化、动态测量；向前插入历史后以视口顶部的行为锚点保持不跳；流式时贴底，向上滚动后停止贴底、显示 ↓；`history/page` 请求与喂给视图 | 200 |
| 用户气泡 | 最大 70%，按行数折叠与"显示更多/收起"，悬停"复制" | 70 |
| 助手正文与 Markdown | streamdown 的配置：去掉 `rehype-raw`，白名单清洗；链接只放行 `http`/`https` 并经 `openExternal`；图片一律显示成链接；排版参数 | 110 |
| ANSI | 颜色只映射到固定的 class | 50 |
| 工具行 | 图标；按 `tool_kind` 的过去时和进行时文案（语义码经 i18n）；运行中的闪光；命令输出块 | 130 |
| 活动组 | 汇总与展开，展开后最高 224、上下渐隐 | 70 |
| 思考、提示与错误、回合标题、未知占位 | "正在思考/已思考 {时长}"；`Alert`；"已处理/你在 … 后停止了/已中断/失败"；占位 | 140 |
| 文案 | `stream` 命名空间加约 40 个键 | 50 |
| 接进会话页 | | 15 |
| **实现小计** | | **1,015**；乘 1.1 约 **1,120** |

**测试**

| 项 | 内容 | 草估 |
|---|---|---|
| UNIT-md-injection | `<script>`、`<img onerror>`、`javascript:`、远程图片、data 图片、ANSI 与 CSS 片段 | 80 |
| 拍平成行 | 回合、活动组、尾部、未知、请求条目留给 T165 | 80 |
| UNIT-stream-anchor | jsdom 里要替行高和滚动容器打桩：插入历史不跳、贴底与停止贴底、↓ | 130 |
| UI-stream-markdown、UI-stream-tools | 截图夹具（两种主题）和用例 | 120 |
| **测试小计** | | **410** |

**合计约 1,425 行**（草估）到 **1,530 行**（实现乘 1.1），贴着约 1,500 行的线，偏高的估计超线。按任务书，开工前停下汇报，没有写代码。

## 拆分建议

按任务书举的拆法：

| 拆分 | 模块 | 用例 | 估计 |
|---|---|---|---|
| **T082a 行列表与锚点** | 拍平成行；共用的行注册表实例；行列表（虚拟化、锚点、贴底、↓、加载更早的历史）；回合标题、未知占位、用户气泡这几种简单的行；接进会话页 | 拍平成行的用例；UNIT-stream-anchor | 实现约 395，乘 1.1 约 435；测试约 210；合计约 **645** |
| **T082b 消息行与 Markdown** | 助手正文与 Markdown 的安全配置；ANSI；工具行与活动组；思考、提示与错误；文案 | UNIT-md-injection；UI-stream-markdown、UI-stream-tools | 实现约 620，乘 1.1 约 680；测试约 200；合计约 **880** |

先后关系：
- T082b 的各种行登记进 T082a 建的注册表实例，所以 T082a 先做；
- 两份的接口就是 `Row` 的形状（`kind` 加上该行的数据）和注册表实例。
