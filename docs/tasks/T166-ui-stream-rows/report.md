# T166 汇报

- 执行者：子 agent（前端工程师，安全）
- 分支：`feat/T166-ui-stream-rows`（T054 worktree；main 由你合入，合并提交 6ae01f9）
- 完成日期：2026-10-20

T166 做完了：六种消息行已登记进 T082 的行注册表，会话页里能看到；UNIT-md-injection、两组截图和 CI 全部通过。新增约 898 行，在约 1,000 的停止线以内。

## 做了什么

1. **助手正文**（`rows/markdown.tsx`，streamdown 2.7.0）
   - 插件显式给出：remark 只用 GFM；rehype 依次是 `rehype-sanitize`（schema 只放行 http/https，去掉 agent 写在 `code` 上的 class）和 `rehype-harden`，不含 `rehype-raw`，原始 HTML 显示成文字。
   - 链接只在 http(s) 时可点，点击走 `ShellAPI.openExternal`。图片一律显示成链接，从不加载；data 图片只留下文字。
   - 关掉了 streamdown 的链接确认弹窗和代码块按钮。`pre`、`code`、`strong`、`table` 换成普通元素：streamdown 默认组件的工具类在本应用里不生成，粗体不粗、表格是双层边框。
   - 排版是静态 CSS（`styles/stream.css` 的 `.md`），按设计 5.7.10。
2. **ANSI**：前景色、粗体、暗淡、下划线映射到固定列表里的 class；256 色、真彩色、背景色跳过；其他转义序列丢掉。
3. **工具行与活动组**
   - 每行：图标加过去时或进行时文案（按 `tool_kind` 取语义码，经 i18n），agent 给的文本经 `escapeInvisible`；颜色为三级文字色的 80%。
   - 运行中带闪光；失败的行尾标"失败"；命令的输出块可展开（标题条加等宽正文，最高 240，ANSI 着色）。
   - 活动组按种类计数汇总，箭头只在悬停时出现，展开后最高 224，上下渐隐。
4. **思考、提示与错误**：思考显示"正在思考"或"已思考 {时长}"（用 T082 的 `useDuration`），可以展开；提示和错误用 `Alert`，文案按代码取，错误下方以纯文本显示 agent 原文。
5. **登记**：`rows/index.ts` 引入 `messages`、`tools` 两个模块，六种行各自 `registerRow`，属性是 `{ row: RowOf<kind> }`，工具组件用 `ToolData`。

## 关键取舍

- **harden 的前缀用 `*`**：给具体前缀就必须给 `defaultOrigin`，否则它直接抛错；给了 origin 又会把相对链接改写成该 origin 下的绝对链接。所以前缀放开，协议由前面的 schema 限定，链接组件里再检查一次 http(s)。harden 仍会挡下 `javascript:`、`data:`。
- **截图夹具里的 ✓、✗ 换成 √、×**：前两个字符不在固定字体子集里，会回退到系统字体，基线就跟机器有关。

## 对照 T136"静态检查拦不下的样式写法"的自查

1. **样式值**：我的组件没有写 `style`。streamdown 只带固定值的内联样式；会写 CSS 变量的动画没有开；代码块的 `--sdm-*` 随默认组件一起被替换掉了。
2. **class 名**：ANSI 的 class、工具图标、各文案键都先经过固定列表。agent 写的 class 被 schema 去掉；默认 schema 允许的其余 class 都是固定值（`task-list-item`、`sr-only` 等）。
3. **间接写 `<style>`**：没有。
4. **`window` 的别名**：没有，外部链接都经 `ShellAPI.openExternal`。
5. **自定义组件的文案属性**：没有。

## 验证方式

- **UNIT-md-injection**（3 个用例，jsdom，行经注册表渲染）：
  - Markdown 输入含 `<script>`、`<img onerror>`、`javascript:` 和 `mailto:` 链接、远程与 data 图片、`<style>`、带 `style`/`class` 的原始 HTML、CSS 代码块；渲染后没有 script、style、img 元素，没有 `on*`、`style` 属性，链接只有 http(s)，没有 agent 给的 class；点击链接走 `openExternal`。
  - ANSI 只映射到固定 class，其余转义被丢掉。
  - 工具、活动组、思考、错误行里的 agent 文本都是纯文字，不可见字符显示成转义；文案按种类正确。
- **UI-stream-markdown**、**UI-stream-tools**：亮、暗两种主题，每个用例还断言 CSP 违规为 0。旧有截图全部不变（共 23 个），连同 4 个新用例跑过。
- **审阅截图**：`docs/tasks/T166-ui-stream-rows/screenshots/` 下四张，Markdown 与工具行各亮、暗一张，系统字体（SF Pro、苹方），1280 × 820。我没有查看本机的 Codex App 包。
- **CI**：在干净的工作区里 `cargo xtask ci pr --record docs/tasks/T166-ui-stream-rows/ci/` 通过：rust 1860、protocol 185、ts 454、license、testdata-scan、ui 25 全部通过，只跳过了 sensitive-words（本机没有词表）。摘要 `pr-0f8980219282.json` 里有 UNIT-md-injection、UI-stream-markdown、UI-stream-tools、UI-csp-zero，没有失败项。

## 提交列表

```
4bddcf5 docs(task): T166 CI summary
0f89802 docs(task): T166 notes after T082 merged
248470d feat(ui-core): register the message rows in the stream, with their screenshots
6ae01f9 Merge branch 'main' into feat/T166-ui-stream-rows
92f35cf docs(task): T166 progress before T082 merges
006df30 feat(ui-core): the stream's message rows: Markdown, ANSI, tools, thinking, notices and errors
d0f1a36 docs(task): T166 size estimate
```

规模：新增约 898 行（不含 lock 和截图），其中测试约 240。

## 遗留问题 / 范围外发现

- **给 T143**：代码块现在是最简单的 `pre`/`code`，agent 写的 `language-*` class 被 schema 去掉了。做高亮时语言信息要从别处取，例如在 schema 里只放行与已知语言列表相符的值。
- **构建提示**：截图页构建时，rolldown 会提示 TanStack Query 文件头的 `"use client"` 指令不会保留。这在 T080 时就有，只是提示，不影响构建和 CSP。
