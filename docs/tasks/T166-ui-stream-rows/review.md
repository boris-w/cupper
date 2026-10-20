# T166 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增约 898 行（不含 lock 和截图），其中测试约 240，在停止线约 1,000 以内。开工前估了数，T082 的行形状一提交就并行开工，T082 合并后再登记进注册表。
- **CI**：执行者的摘要 `pr-0f8980219282.json` 通过。合并 main（含 T138 等）没有冲突，重跑 `ci pr --record` 通过：`pr-628e7b1aa268.json`（rust 1874、protocol 185、ts 454、license、testdata-scan、ui 25）。第一次重跑的 protocol 失败是编排者在 CI 运行中往工作区写了评审文件，`pnpm gen` 之后的"工作区未变"检查因此失败，与代码无关。
- **依赖**：新增 `streamdown` 2.7.0、`remark-gfm`、`rehype-sanitize`、`rehype-harden`，版本精确锁定；license 作业通过。

## 逐项意见

1. **Markdown 的防线**（设计 5.7.8"Markdown 与不可信内容"）：
   - 插件全部显式给出，不用 streamdown 的默认插件；没有 `rehype-raw`，还开了 `skipHtml`，原始 HTML 不会被解析；
   - `rehype-sanitize` 的 schema 只放行 http(s)，去掉 `code` 上的 class，所以 agent 写不进本应用的样式类（T136 的关切）；
   - `rehype-harden` 的前缀用 `*` 的理由成立：给具体前缀就得给 `defaultOrigin`，相对链接会被改写成绝对链接。协议由 schema 把关，`Link` 组件再查一次 http(s)，两道之外 harden 仍挡 `javascript:`、`data:`；
   - 链接点击经 `ShellAPI.openExternal`，不在文档里导航；图片一律显示为链接，从不加载。
   UNIT-md-injection 覆盖了 `<script>`、`<img onerror>`、`javascript:`、`mailto:`、远程与 data 图片、`<style>`、带 `style`/`class` 的原始 HTML。
2. **ANSI**：只映射到固定列表里的 class，256 色、真彩色、背景色跳过，其余转义丢掉。不经任何样式值，符合"agent 字符串不进样式"。
3. **替换 streamdown 的默认组件**：`pre`、`code`、`strong`、`table` 换成普通元素，避免依赖本应用不生成的工具类，也顺带去掉了代码块里写 `--sdm-*` 变量的路径。
4. **T136 的自查清单**：执行者逐条写了，和代码一致。
5. **截图夹具字符**：✓、✗ 不在固定字体子集里，换成 √、×，基线不随机器变。

## 记下的点

- **交给 T143**：代码块是最简单的 `pre`/`code`，agent 写的 `language-*` class 被去掉了。做高亮时语言要从别处取，或者在 schema 里只放行已知语言列表里的值。
- **构建提示**：rolldown 提示 TanStack Query 文件头的 `"use client"` 不保留，T080 时就有，不影响构建和 CSP。
- **"插入历史后视口不跳"**仍只在单元测试里验证过（T082 的记录），留给 T091。
