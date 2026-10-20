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

## 独立部分的实现记录（T082 合并之前）

- **Markdown**（`rows/markdown.tsx`）：
  - 插件全部显式给出。remark 只有 GFM；rehype 依次是 `rehype-sanitize`、`rehype-harden`，不含 `rehype-raw`，原始 HTML 显示成文字。
  - sanitize 的 schema 以 hast-util-sanitize 的默认值为底：`href`、`src`、`cite` 只认 http、https；去掉 `code` 上 agent 给的 class（`language-*`），其余默认允许的 class 都是固定值。
  - harden 必须给 `defaultOrigin` 才能用具体的前缀，否则抛错；给了 origin 又会把相对链接解析成那个 origin 下的绝对链接。所以前缀给 `*`，靠前面的 schema 限定协议。harden 仍会挡下 `javascript:`、`data:` 等，被挡的链接和图片只留文字。
  - 组件：`a` 只在 http(s) 时是链接，点击时 `preventDefault` 后经 `ShellAPI.openExternal` 打开；`img` 一律显示成指向图片的链接，从不加载；`pre`、`code` 用最简单的元素（高亮和代码块按钮归 T143）。另外关掉 streamdown 的 `linkSafety` 弹窗、`controls` 按钮，加 `skipHtml`。
  - 排版：静态 CSS `styles/stream.css` 的 `.md`，按 5.7.10。规则不在任何 layer 里，以压过 streamdown 默认组件带的、碰巧在本仓库生成了的工具类。
- **ANSI**（`rows/ansi.tsx`）：SGR 的前景色（30–37、90–97）、粗体、暗淡、下划线映射到固定 class；256 色、真彩色、背景色跳过；其他 CSI、OSC、两字节转义直接丢掉。
- **工具行**（`rows/tools.tsx`）：
  - 图标按 `tool_kind`；文案 `stream:tool_<kind>` 或 `_running`，`target` 依次取命令、查询、URL、路径（多个时"等 n 个文件"），最后是 agent 的标题，经 `escapeInvisible`。不认识的种类按 `other` 处理，直接显示标题。
  - 运行中带闪光；失败时行尾标"失败"；有输出的命令可以展开输出块（标题条加等宽正文，最高 240，ANSI 着色，截断时注明只显示最后一部分）。
  - 活动组按种类计数（"运行了 2 条命令，读取了 1 个文件"），展开箭头只在悬停时显示，展开后的列表最高 224，上下渐隐（静态的 `mask-image`）。
  - 组件的属性 `Tool` 与 T082 的 `ToolData` 结构相同；合并后改用它。
- **思考、提示与错误**（`rows/messages.tsx`）：思考的文字是纯文本（`escapeInvisible`）；提示和错误按代码取文案，不认识的代码用兜底文案；错误下方显示 agent 原文（纯文本）。恢复指引是 T143 的事。
- **UNIT-md-injection**（`rows/messages.test.tsx`，jsdom）：
  - `<script>`、`<img onerror>`、`javascript:` 与 `mailto:` 链接、远程图片、data 图片、`<style>` 与带 `style` 和 `class` 的原始 HTML、CSS 代码块：渲染后没有 `script`、`style`、`img` 元素，没有 `on*` 和 `style` 属性，链接只有 http(s)，没有 agent 的 class；
  - 链接点击走 `openExternal`；
  - ANSI 的映射；
  - 工具、思考、错误行里的 agent 文本都是文字，不可见字符显示成转义。

## 对照 T136"静态检查拦不下的样式写法"的自查

1. **样式值来自 agent 文本**：本任务的组件没有写任何 `style`。streamdown 自带的内联样式只有固定值（表格最高 300 一类）；会带内联样式和 CSS 自定义属性的动画（`animated`）没有开，代码块的 `--sdm-*` 随默认代码块组件一起被替换掉了。
2. **class 名拼 agent 文本**：
   - ANSI 的 class 只来自固定列表；
   - 工具图标按种类查固定的路径表；
   - 文案键里的种类、提示代码、错误代码都先过固定列表；
   - Markdown 里 agent 的 class 被 schema 去掉，剩下的默认 class 都是固定值（`task-list-item`、`sr-only` 等）。
3. **间接写 `<style>`**：没有。
4. **`window` 的别名**：没有；外部链接经 `ShellAPI.openExternal`。
5. **自定义组件的文案属性**：`Alert` 收 `children`，`ToolIcon` 收种类，都不是文案属性。

## 进度与行数（独立部分完成，T082 未合并）

| 模块 | 新增 |
|---|---|
| 依赖（`package.json`，不含 lock） | 4 |
| Markdown | 94 |
| ANSI | 94 |
| 工具行与活动组（含图标 20） | 164 |
| 思考、提示与错误 | 90 |
| 样式 `stream.css` | 145 |
| 文案 | 56 |
| UNIT-md-injection | 131 |
| **合计** | **约 778** |

剩下的是 T082 合并后的部分：登记进注册表、接会话页，约 40；UI-stream-markdown、UI-stream-tools 的夹具和用例，约 120。推算合计约 **940**，在约 1,000 的停止线以内，但贴近。样式文件比估计多，是 prettier 把每个选择器拆成一行的结果。

## T082 合并之后

- **登记**：`rows/index.ts` 加 `import "./messages"`、`import "./tools"`。六种行各自 `registerRow`，组件属性是 `{ row: RowOf<kind> }`：
  - `agent_message`：`Markdown`；
  - `thought`、`notice`、`error`：`messages.tsx` 的三个行组件；
  - `tool`：内部的 `ToolLine`；
  - `activity_group`：组内各条也用 `ToolLine`。
- **类型与时长**：工具组件改用 T082 的 `ToolData`；思考的时长改用 T082 的 `useDuration`（`stream:duration_s`、`stream:duration_ms`），去掉了我自己的格式化函数和两个文案键。
- **截图时发现、已改的三处**：
  - streamdown 把粗体画成带 `font-semibold` 的 `span`，表格外面套一个带边框的 `div`，用的都是本仓库不生成的工具类：粗体不粗，表格是双层边框。改成自己的 `strong` 和 `table`（外层只有横向滚动）。
  - 工具行按 5.7.4 用三级文字色的 80%（`text-fg-tertiary/80`）。
  - 截图夹具里的 ✓、✗ 不在固定字体的子集里，会回退到系统字体，基线因此跟机器有关；改成子集里有的 √、×。
- **截图**：`tools/ui-screenshots/src/stream.spec.ts` 覆盖两种主题：
  - UI-stream-markdown：思考、标题、粗体、行内代码、链接、有序与嵌套列表、代码块、表格、引用、显示成链接的图片、显示成文字的原始 HTML；
  - UI-stream-tools：展开的活动组、编辑、失败并展开输出的命令（ANSI 着色）、提示、错误、运行中的命令。

  每个用例还断言 `securitypolicyviolation` 为 0。回合的开始时间设在页面固定时钟之前 12 秒，回合标题显示"已处理 12 秒"。
- **审阅截图**（`screenshots/`）：用一个用后删掉的临时用例，加载同样的夹具，把 `--font-sans` 换回系统字体（SF Pro、苹方）后截取，1280 × 820（2 倍像素），共四张：Markdown 与工具行，各亮、暗两种。按 F2，没有查看本机 Codex App 包。

## 行数（不含 notes、lock、截图）

| 模块 | 新增 | 估计 |
|---|---|---|
| 依赖 | 4 | 10 |
| Markdown | 106 | 120 |
| ANSI | 94 | 60 |
| 工具行与活动组（含图标 20） | 166 | 190 |
| 思考、提示与错误 | 84 | 80 |
| 样式 | 149 | （在 Markdown 与各行里） |
| 文案 | 约 54 | 50 |
| 登记 | 2 | 25 |
| UNIT-md-injection | 约 135 | 150 |
| UI-stream-markdown、UI-stream-tools | 约 104 | 120 |
| **合计** | **约 898** | 约 860 |
