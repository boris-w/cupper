# T143 notes

## 范围的变化（2026-11-03，编排者裁定）

- 宿主目前不写 `error` 条目（`git grep 'Item::Error' crates/cupper-host` 没有构造；失败的回合只在 `turn.completed.error_code` 里带 FailureCode），E2E-error-guidance 在真实宿主上跑不通。
- 编排者另开 T226（宿主写 `error` 条目与恢复指引），E2E-error-guidance 移到 T226，T226 依赖本任务的界面。
- 本任务只交界面：错误条目的警告框与每种 guidance 的按钮或文字指引、UI-error（截图加组件测试）、代码高亮、计划、计划药丸。`tests/ids.toml` 由编排者在 main 上改。

## 规模估计（开工前）

停止线 900，硬停止线 1100。

| 模块 | 实现 | 测试 |
|---|---|---|
| 代码高亮：Shiki 懒加载、语言表、七类配色的主题、缓存、hook | 130 | — |
| Markdown 接入：schema 放行已知语言、`pre` 组件、流式未闭合时不高亮 | 45 | — |
| 样式：七个 `hl-*` class、计划行 | 25 | — |
| 计划行：编号步骤与状态图标、Markdown 计划的摘要条 | 100 | — |
| 行数据：`plan` 带 `running`，`error` 带可重发的用户消息 | 25 | — |
| 计划药丸与 `<ComposerOverlays/>`（连同回到底部按钮）、滚动到计划 | 75 | — |
| 错误条目：每种 guidance 的按钮或文字、复制命令、重试 | 80 | — |
| 文案、图标 | 45 | — |
| 组件测试：高亮映射、未闭合不高亮、计划行、药丸、每种 guidance | — | 230 |
| 截图：UI-code-highlight、UI-plan、UI-error | — | 110 |
| 合计 | 525 | 340 |

合计约 865，离 900 近：每个模块提交后跑 `cargo xtask size`，推算越线就停下汇报。

实际 +968（实现约 547，测试约 421），越过停止线后停下报告；编排者接受，停止线放宽到 1000，硬停止线 1100 不变。

## 实现记录

### 代码高亮

- 依赖：`@shikijs/primitive`、`@shikijs/engine-javascript`、`@shikijs/langs`，4.5.0，MIT，锁精确版本。没用 `shiki` 主包和 `@shikijs/core`：前者带 WASM 引擎和全部主题，后者带 `hast-util-to-html`（`codeToHtml` 用的），都用不上。新增的间接依赖 `oniguruma-to-es`、`oniguruma-parser`、`regex`、`regex-recursion`、`regex-utilities`、`@shikijs/types`、`@shikijs/vscode-textmate` 也都是 MIT。
- JavaScript 正则引擎只用 `new RegExp`，不用 `eval`、`new Function`（扫过这几个包的源码），CSP 和 Trusted Types 都不用动；截图用例在主文档的 CSP 下违规数为 0。
- 懒加载：Shiki 和每种语法都是动态 import，release 构建里各自成块，入口块里只有引用。37 种语言加 18 个别名；别的语言照原样显示。
- 配色：主题给七个类别各一个占位颜色（`#000001`–`#000007`），token 的颜色只用来查类别，页面上是固定的 `hl-*` class，颜色取 tokens.css 里已有的 `--color-code-*`（5.7.10 的表），亮暗两套都由 token 的 `light-dark()` 给出。没有用 Shiki 的双主题输出：它靠内联样式和 CSS 自定义属性。
- 什么时候高亮：streamdown 的 `useIsCodeFenceIncomplete()` 为真（最后一块、正在流式、围栏没闭合）时原样显示；围栏闭合后高亮一次。结果按"语言 + 文本"缓存 200 条，虚拟列表把行滚出再滚回时不重做。超过 5 万字符的代码块不高亮（分词在渲染进程的主线程上）。
- 语言从哪来：sanitize 的 schema 只放行已知语言的 `language-*` class，`pre` 组件读它选语法，不写进 DOM（UNIT-md-injection 照旧断言 DOM 里没有 `language-` class）。

### 计划与药丸

- `plan` 行带 `running`（条目还没结束）。有 `entries` 时显示编号步骤，状态不认识的显示为未开始；只有 Markdown 时显示"计划"摘要条，进行中写"编写计划"带闪光，展开后是 Markdown，blob 形式的在展开时下载。
- 药丸：最后一个回合在运行、并且有带步骤的计划时显示"第 n / m 步"。n 取进行中的那一步；没有进行中的取第一个未完成的；全部完成取 m。点击后滚到计划行，并停止贴底。
- `<ComposerOverlays/>` 放在消息列表框的底部（不是设计组件树里 SessionPage 的直接子节点）：药丸和"回到底部"都操作列表的滚动，放在一起不需要跨组件传滚动接口；两者在同一行居中并排，不会重叠。"回到底部"按钮的样式和文案没变。

### 恢复指引

| guidance | 界面 | 入口归属 |
|---|---|---|
| `retry` | 会话最后一个回合里：按钮"重试"，经 `session/send`（`mode: queue`）重发该回合的第一条用户消息，内容块原样（文字、`@` 文件、图片、长文本 blob、审查评论都一起）；更早的回合里只写文字 | 本任务 |
| `run_login_in_terminal` | 文字，加上 agent 的登录命令（Claude `claude`、Codex `codex login`）和"复制命令" | 打开终端的入口在 T153（`workspace/open` 在 T126） |
| `upgrade_cli`、`install_cli` | 文字 | 设置页的 agent 状态在 T151 |
| `quota_exhausted` | 文字 | — |
| `change_model` | 文字 | 模型选择器在 T141 |
| `restart_agent` | 文字 | `agents/restart` 的界面入口在 T151 |
| `none`、未知取值 | 不加任何内容 | — |
