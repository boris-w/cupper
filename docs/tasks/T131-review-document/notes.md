# T131 notes

## 开工前的估算（2026-11-04，报给编排者，等回复后再写代码）

已有、可以直接用的：
- ShellInfo 里冻结了的 `reviewDocumentUrl`（浏览器 mock 现在给的是 `about:blank`）；
- T130 的视图接口（`review/view.ts`）、还原确认（`RevertConfirm`，来源为 `view` 时不抢焦点，同一时间只显示一个）、`act` 与 `comment` 的接线；
- `app-protocol.ts` 的多主机支持；
- T104 原型的写法：导航例外、Worker、Virtualizer、两条测试输入路径。

开工前发现的一个问题：许可证门禁按 `allow.has(text)` 精确比较，`@pierre/*` 的 `package.json` 写的是小写 `apache-2.0`，会被判为不在清单里。门禁要改成不区分大小写，或者按 SPDX 规范化后比较。

| 模块 | 实现 | 测试 |
|---|---|---|
| A. 入口与 CSP、构建：`REVIEW_DOCUMENT_CSP`、vite 第二个 client 环境与入口 HTML、主进程加 `cupper-review` 主机、`reviewDocumentUrl`、许可证门禁登记新环境并修大小写、浏览器 mock 与截图页的第二个源 | 120 | 10 |
| B. 导航：`navigation.ts` 只放行一次审查文档的首次加载 | 25 | 20 |
| C. 协议与 zod：两个方向的消息、版本、解析（主文档与审查文档共用一份 schema） | 90 | 50 |
| D. 主文档一侧组件：iframe（sandbox、allow）、空闲时预加载、常驻、端口握手；props 译成 `init`/`render`/`scroll_to`/主题语言缩放；`need_file` 时取 `git/file_diff`（必要时取 blob）；按取到的 hunk 校验 `hunk_action`，译回 `act`；`draft_comment` 译回 `comment`；`key_forward` | 220 | 100 |
| E. 审查文档：入口、只认第一个端口、无状态视图、pierre（Worker、`shiki-js`、只加载用到的语言、Virtualizer 就是滚动容器）、按文件的头部与折叠、hunk 渲染（hunk、mode、whole_file）、`scroll_to`、懒加载、主题与缩放 | 260 | — |
| F. 审查文档里的操作：按范围的 hunk 按钮与文件按钮；行内评论（悬停行号出现"+"、评论框、Esc、⌘Enter、已有草稿显示在行上）；其余按键转发 | 130 | — |
| G. 文案 | 25 | — |
| H. 用例：UNIT-review-injection；E2E-SEC-review-iso；E2E-review-comment；E2E-SEC-revert-confirm；截图用例的改动（基线由 ui-baseline 生成，不计行数） | — | 270 |
| 合计 | ~870 | ~450 |

合计约 1,320，超过硬停止线 1,300；按前几个 L 任务的偏差上浮 20–30%，会到 1,600 左右。需要拆分。

### 拆分方案

- **T131（本任务）审查文档 I：入口、协议与只读视图**
  - 包括 A、B、C、D、E、G，以及 F 里的按键转发；
  - D 里 `hunk_action`、`draft_comment` 的接收与校验也在这里（E2E-SEC-review-iso 要用：iframe 发来的 `revert` 不直接执行）；
  - 用例：UNIT-review-injection、E2E-SEC-review-iso、review 与文件树的截图基线；
  - 约 960，上浮后约 1,150–1,250。
- **新任务 审查文档 II：hunk 按钮与行内评论**
  - 包括 F（审查文档里的按钮、评论交互）；
  - 用例：E2E-review-comment、E2E-SEC-revert-confirm、剪贴板的 e2e 核实；
  - 约 360，上浮后约 450。

## 实现记录

### 结构

- **审查文档**（`packages/ui-core/src/review/document/`，经 `@cupper/ui-core/review-document` 导出）：
  - `index.ts`：只认第一个端口：`event.source` 是父窗口，`event.origin` 等于 `location.ancestorOrigins[0]`，并且带着一个端口、内容是合法的 `init`。收下之后就移除 `window` 上的监听。每条消息都用 zod 解析，不合格的丢掉。全部按键经 `key_forward` 转给主文档。
  - `ReviewDocument.tsx`：用 pierre 的 `CodeView`，不用 T104 原型里的 `Virtualizer` 加 `MultiFileDiff`。`CodeView` 是 pierre 自带的多文件虚拟列表，它自己的 div 就是滚动容器（满足 report-q29 第 6 节第 2 条），按文件给条目，自带按文件折叠、滚到文件或行、吸顶的文件头。
    - 文件头由我们自己渲染（`renderCustomHeader`），包括折叠按钮、状态图标、路径、增删行数，以及二进制、文件模式的说明。文件头只在文件进入渲染窗口时才渲染，渲染时再发 `need_file`，这就是懒加载。
    - 还没取到 hunk 的文件先给空 diff。只有带行的 diff 才设 `cacheKey`：pierre 按 `cacheKey` 缓存布局，空 diff 和后来的 diff 如果用同一个键，后者不会重新排版（实测踩到）。
    - `WorkerPoolContextProvider` 随文档挂载，不等第一个 diff，所以预加载的同时也预热了 Worker。`langs: []`，语言在第一次高亮某种语言的文件时才解析加载，所以只加载 diff 里出现的语言。
    - hunk 之间的分隔用 `simple`：pierre 默认的 `line-info` 会显示英文的 "N unmodified lines"，不能走 i18n。
    - `auto` 视图的阈值：iframe 宽度不小于 800 时分栏，小于 800 时统一视图。
  - `worker.ts`：`import "@pierre/diffs/worker/worker.js"`，由 `workerFactory` 以 module Worker 从审查源启动。
- **主文档一侧**（`ReviewFrame.tsx`）：
  - iframe：`sandbox="allow-scripts allow-same-origin"`，`allow="clipboard-write"`。每次加载都新建一个 `MessageChannel`，只听自己建的 `port1`。
  - props 译成 `files`（文件、折叠、视图、换行）、`render`（回应 `need_file`）、`scroll_to`；`theme_changed` 跟随 `onThemeChange`。
  - 取 `git/file_diff` 用 T130 约定的查询键，`lines_blob` 先取 blob。按自己取到的 hunk ID 校验 `hunk_action`，同时要求令牌一致、操作在范围之内；不带 `hunk_ids` 表示整个文件。
  - `draft_comment` 要求令牌一致、路径在 diff 里、行范围有效、正文不为空。
  - `key_forward` 在 iframe 元素上派发一个合成的 `keydown`，可以冒泡。
- **常驻**：
  - 侧边面板第一次打开之后、或者窗口第一次空闲时（`requestIdleCallback`）挂载，之后只隐藏、不卸载。
  - `ReviewPanel` 里的 `Review` 不再按会话加 `key`，iframe 因此跨会话保留；没有会话、没有改动时，iframe 也只是隐藏。会话相关的状态单独重置：文件树按会话加 `key`，`useActions` 换会话时清掉待确认的还原。
  - 面板隐藏或没有会话时，`git/diff`、`git/status` 不发请求（`enabled`）。
- **入口**：
  - vite 新增 `review` 环境（`consumer: "client"`）：不加的话，非 `client` 名字的环境默认按 server 构建，`new Worker(new URL(...))` 不会被处理。输出到 `dist/review/review.html`。
  - Worker 是 vite 单独打的包，环境名是 `client`；元数据插件按名字 `review-worker` 写，否则会覆盖主文档的 `client.json`。
  - 许可证门禁登记 `review` 和 `review worker` 两项，本机核对 36 个和 31 个 npm 包，没有问题。
- **截图工具**：
  - 审查文档从第二个源 `https://cupper-review.test` 加载，两个文档各按生产 CSP（`mainDocumentCsp(审查源)`、`reviewDocumentCsp(主源)`）。
  - 违规收集覆盖所有 frame。截图用例给 `git/file_diff` 夹具，并等 diff 出行。

### 与设计 5.7.5 消息表的差异（汇报里列出）

- `render {diff_token, path, hunks, drafts, expanded}` 拆成两条：
  - `files {diff_token, files, collapsed, view, wrap}`：整个 diff 的文件和视图状态；
  - `render {diff_token, path, hunks}`：一个文件的 hunk。
  - 草稿要到 T238 才在行上显示，所以 T131 不下发草稿（不留没人用的字段）。
- `need_file`、`hunk_action`、`draft_comment` 都带 `diff_token`，主文档按令牌丢掉过期的消息。
- `hunk_action` 带 `path`，`hunk_ids` 可以不带，不带表示整个文件（文件级按钮在 T238）。
- `draft_comment` 直接用协议里的 `ReviewComment`。`key_forward` 的修饰键拆成四个布尔值。
- 没有 `locale_changed`：外壳没有运行时切换语言的事件，加了也不会有人发。
- `init` 不带缩放：页面缩放（菜单的放大、缩小）由 Chromium 传给进程外 iframe。E2E-SEC-review-iso 里核对两个文档的 `devicePixelRatio` 一致。
- 地址是 `app://cupper-review/review.html`，不是 `index.html`：vite 的 `root` 是全局的，两个入口在同一个根目录下，文件名不能重复。
- 审查文档认的源是 `location.ancestorOrigins[0]`，也就是实际嵌入它的文档的源。生产里由 CSP 的 `frame-ancestors app://cupper` 保证这个源只能是 `app://cupper`；mock 下则是截图页的源。

### 体积

- `dist/review` 约 17 MB，主要是 shiki 的语法和主题，每种一个 chunk，按需加载。
- Worker 包里又带了一份语法：pierre 的 worker 代码里有动态 import。实际上语言由主线程解析后发给 Worker，Worker 里那份不会加载。

## 依赖

- `@pierre/diffs` 1.6.4，2026-11-04 00:39 UTC 发布，当天加入，没过 pnpm 12 默认的最短发布时间（1 天）。`pnpm add` 自动在 `pnpm-workspace.yaml` 加了 `minimumReleaseAgeExclude: ['@pierre/diffs@1.6.4']`。编排者裁定（选项 (a)）：开发期间保留这条临时例外，最终推送的提交里去掉；合并以 2026-11-05 00:39 UTC 之后、去掉例外的那次 CI 为准。
- **1.6.3 → 1.6.4 的核对**：两个 tarball 各自 `npm pack` 到空目录里解包比较。文件清单相同，内容只有 `package.json` 不同，`dist/` 的代码逐字节一致：
  - 1.6.3 发布时依赖还是 `workspace:*`、`catalog:`，装不上，并带一个 `prepublishOnly` 脚本；
  - 1.6.4 把依赖写成精确版本（`diff` 9.0.0、`hast-util-to-html` 9.0.5、`lru_map` 0.4.1、`@pierre/highlights` 0.4.0、`@pierre/theme` 2.0.0、`@pierre/theming` 1.0.2），`shiki` 仍是 `^3.0.0 || ^4.0.0`，没有任何 `scripts`。
  - 所以 1.6.4 只是修正了 1.6.3 的发布元数据，没有新增安装脚本，代码没有变化，谈不上新增的网络或文件访问。
- `zod` 4.6.7，与 `@cupper/protocol`、`@cupper/host-client` 相同。
