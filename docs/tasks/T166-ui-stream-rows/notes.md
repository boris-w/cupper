# T166 工作笔记

- 执行者：子 agent（前端工程师，安全）
- 日期：2026-10-20

## 读到的现状（开工前）

- **行的形状**：按 T082 分支 `rows/stream.ts` 的约定（只读核对）。本任务的组件按各自用到的字段接收属性（例如 `AgentMessage({ text, streaming })`），T082 合并后再写一层登记：`registerRow("agent_message", ({ row }) => …)`。这样在 T082 合并之前不需要复制它的类型。
- **streamdown 2.7.0**（Apache-2.0，2026-09-30 发布）：
  - 默认的 rehype 插件是 `raw`、`sanitize`（hast-util-sanitize 的默认 schema 加 `streamdown` 协议）、`harden`。harden 的默认参数全是 `*`，即全部放行，还允许 data 图片。传 `rehypePlugins` 就整体替换默认值，所以显式给出 `[sanitize(自己的 schema), harden(只放行 http/https、不放行图片前缀、不许 data 图片)]`，不给 `raw`。
  - 默认 remark 插件是 GFM，加一个内部的代码 meta 插件；显式只给 GFM。
  - 默认 `linkSafety` 开启，点链接弹它自己的确认框；`controls` 默认开启，代码块和表格带复制、下载按钮，下载会造 blob URL 再点链接。两者都关掉：链接由自己的 `a` 组件经 `ShellAPI.openExternal` 打开，代码块的按钮留给 T143。
  - 代码高亮、Mermaid 在包内是动态 import 的分块，不给 `plugins` 就不会加载。
  - 默认组件用 Tailwind 的默认主题类名。本仓库不引默认主题，而且 Tailwind 不扫描 `node_modules`，这些类名不会生成样式。排版用一份静态 CSS（`.md` 下的选择器），按 5.7.10。
- **要新加的依赖**：`streamdown`，以及显式插件要 import 的 `rehype-sanitize`、`rehype-harden`、`remark-gfm`（都是 streamdown 自己的依赖，版本与它解析出的一致，许可证 MIT）。
- **截图**：会话页的行列表是 T082 的。UI-stream-markdown、UI-stream-tools 要等 T082 合并后接上会话页再写。
- **T136 的"静态检查拦不下的写法"**（在 `docs/tasks/T136*/notes.md`；注意 T136 不是我做的，任务书写错了执行者）：样式值来自 agent 文本、class 名拼 agent 文本、间接写 `<style>`、`window` 的别名、自定义组件的文案属性。本任务逐条自查，结果写进汇报。

## 开工前的规模估计

实现按以往偏差乘 1.1，测试按写出来的估。

| 模块 | 实现 | 测试 |
|---|---|---|
| 依赖与导出 | 10 | — |
| 助手正文：插件与清洗 schema、`a`（经 `openExternal`）与 `img`（显示成链接）、代码与排版 CSS | 120 | — |
| ANSI：SGR 解析，颜色与粗体只映射到固定的 class | 60 | — |
| 工具行：图标、按 `tool_kind` 的过去时与进行时文案、闪光、命令输出块 | 130 | — |
| 活动组：汇总、悬停箭头、展开后最高 224 与上下渐隐 | 60 | — |
| 思考：正在思考 / 已思考 {时长}、展开 | 40 | — |
| 提示与错误：`Alert`、按语义码的文案 | 40 | — |
| 文案（`stream` 命名空间约 40 个键） | 50 | — |
| 登记进注册表、接会话页（T082 合并后） | 25 | — |
| UNIT-md-injection（`<script>`、`<img onerror>`、`javascript:`、远程与 data 图片、ANSI、CSS 片段）与 ANSI、工具文案的单元用例 | — | 150 |
| UI-stream-markdown、UI-stream-tools（夹具、两种主题） | — | 120 |
| **合计** | **约 535，乘 1.1 约 590** | **约 270** |

合计约 **860 行**，在约 1,000 的停止线以内，开工。先做不依赖 T082 的部分（组件、Markdown 配置、ANSI、文案、UNIT-md-injection），做完时若 T082 还没合并，停下汇报。
