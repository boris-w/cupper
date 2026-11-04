# Q29 判定报告（T104，SP-6 场景 9）

- 日期：2026-11-04
- 原型：`spikes/T104-review-iframe/`（runner 上跑的是 0151aa5d）
- runner 运行：https://github.com/boris-w/cupper/actions/runs/41769669515 （`spike-run`，`macos-15`，冷启动 30 次，全部成功）
- 原始数据：本目录 `results/`（`run-1.json` 至 `run-30.json`、`summary.json`，以及第一次运行截的亮、暗两张图）

## 结论与 K9 建议

**建议 K9 通过，做 T131。**

- 功能项全部通过：
  - constructable stylesheet 在审查文档的 CSP 下可用；
  - Worker 和模块脚本能在跨源 iframe 里加载；
  - 两个文档的 `securitypolicyviolation` 计数都是 0（30 次合计 0，控制台里也没有 CSP 报错）。
- 8 点里的硬性项都满足：
  - 跨源隔离；
  - 只认第一个端口；
  - inputGuard 不借用 iframe 里的输入；
  - 首次加载之后的导航被拦截；
  - 菜单加速键在焦点位于 iframe 时照样生效。
- 30 次冷启动之间，没有一项检查结果不一致。
- 性能只记录，不判定（第 5 节）。runner 上看不出数量级的问题：
  - 滚动帧时间 p50 16.7 ms，p95 18.7 ms；
  - 审查文档的渲染进程约 300 MB；
  - 从创建 iframe 到第一屏带高亮，p50 2.5 s。

## 1. 版本与许可证

- `@pierre/diffs` 1.6.4（设计核对时是 1.5.2），依赖 shiki 4.6.0。原型锁的是精确版本。
- 在空目录里 `npm install --ignore-scripts`，逐个读了 52 个包的 `package.json`：
  - Apache-2.0：`@pierre/diffs`、`@pierre/highlights`、`@pierre/theme`、`@pierre/theming`，`package.json` 里写成小写 `apache-2.0`；
  - MIT：46 个（shiki 与 `@shikijs/*`、hast/mdast/unist 一系、`lru_map`、`oniguruma-to-es`、`regex*` 等）；
  - ISC：`@ungap/structured-clone`；
  - BSD-3-Clause：`diff` 9.0.0。
- 全部在 ADR-11 的允许清单里。
- `@pierre/highlights` 的 NOTICE 列出的随包主题都是 MIT 或 Apache-2.0。文中提到 GPL-3.0，只是说明没有收录 Aurora X 的原版。
- 审查文档的产物里没有 `.wasm`：高亮器固定为 `shiki-js`，`@pierre/highlights` 的 wasm 后端不会进入产物。
- T131 真正引入依赖时，`license` 作业会再查一遍。

## 2. 原型怎样搭

- **主进程直接 import `apps/desktop` 的现有代码**，没有复制：
  - `security/app-protocol.ts`：`APP_SCHEME_PRIVILEGES` 为 `standard`、`secure`；`appProtocolHandler` 每个主机一个根目录、一份 CSP；
  - `security/permissions.ts`：只放行 `clipboard-sanitized-write`；
  - `window.ts`：`mainWindowOptions` / `webPreferences`，`nodeIntegrationInSubFrames: false`；
  - `input/index.ts`：`watchInput`，即 inputGuard。
  - 主文档的 CSP 用 `@cupper/shell-api` 的 `MAIN_DOCUMENT_CSP`；Electron 用 `apps/desktop` 的 44.9.0。
- **两个源**：
  - `app://cupper`：主文档，带 `require-trusted-types-for`；
  - `app://cupper-review`：审查文档，CSP 照 5.7.8 的表（原型里的 `src/csp.ts`），不启用 Trusted Types；
  - iframe 带 `sandbox="allow-scripts allow-same-origin"`、`allow="clipboard-write"`。
- **导航**：`navigation.ts` 现在拦截一切 `will-frame-navigate`。原型加了 M1b 的那个例外：只放行审查文档的首次加载（`app://cupper-review/index.html`），之后的导航一律拦截。
- **审查文档**：React 加 pierre 的 `WorkerPoolContextProvider`。
  - `workerFactory` 为 `() => new Worker(new URL("./worker.js", import.meta.url), {type: "module"})`，worker 入口就是 `@pierre/diffs/worker/worker.js`；
  - 高亮用 `preferredHighlighter: "shiki-js"`；
  - `Virtualizer` 里放 `MultiFileDiff`，Virtualizer 本身就是滚动容器。
- **夹具**：合成的 TypeScript 文件，5,000 行，每五行改一处，得到一个约 6,000 行的 hunk；其中一行写的是 `<style>…</style><img src=x onerror=…>`。
- **一次运行**：
  1. 主进程加载主文档，等审查文档渲染出带高亮的行；
  2. 读两个文档的计数（`executeJavaScript`），取进程内存（`getAppMetrics`）；
  3. 在审查文档里从顶部滚到底部，记录每帧的间隔；
  4. 做第 3、4 节的检查；
  5. 写 JSON，退出。
- `measure.mjs` 循环冷启动，每次用全新的 userData。截图、剪贴板、真实按键三步只在 runner 上（`GITHUB_ACTIONS=true`）做。

## 3. 功能验证（任务书第 2 项）

| 项 | 结果 | 证据 |
|---|---|---|
| constructable stylesheet | 可用，没有违规 | pierre 在 shadow root 上用了 1 个 `adoptedStyleSheets`，另有 1 个 `<style>`；在审查文档的 CSP（`style-src 'self' 'unsafe-inline'`）下渲染正常，违规计数为 0 |
| Worker 在跨源 iframe 里加载 | 可以 | `workerFactory` 启动了 2 个 module Worker（`worker-src 'self'`）；高亮在 Worker 里完成，带颜色的 token 出现 |
| 模块脚本在跨源 iframe 里加载 | 可以 | 审查文档的入口是 `<script type="module">`，拆出约 300 个按需加载的 chunk（各语言的语法、主题），都从 `app://cupper-review/` 加载 |
| `securitypolicyviolation` 为 0 | 是 | 30 次运行：主文档 0，审查文档 0，控制台的 CSP 报错 0 |
| 跨源隔离（安全确认 (ii)） | 是 | 两个文档的 origin 分别是 `app://cupper`、`app://cupper-review`；iframe 访问 `parent.document` 抛 `SecurityError`；preload 只在主框架（主文档有标记，审查文档没有） |
| 注入按文本显示（安全确认 (i)） | 是 | 带 `<img>`、`<style>` 的那一行按文本显示；shadow root 里没有 `img` 元素 |
| 只认第一个端口（安全确认 (iii)） | 是 | 审查文档接受了 1 个端口，忽略了 2 条：带另一个端口的第二个 `init`、经 `window` 发来的 `theme_changed`。主文档在伪造端口上和 `window` 上什么也没收到 |
| 导航限制 | 是 | 首次加载之后，`location.href = …/elsewhere.html` 被 `will-frame-navigate` 拦住，iframe 仍停在 `index.html` |

## 4. 桌面确认的 8 点（任务书第 3 项）

| # | 问题 | 结论 |
|---|---|---|
| 1 | 进程与内存 | 进程外 iframe，多一个渲染进程，约 300 MB |
| 2 | 焦点与快捷键 | 菜单加速键在焦点位于 iframe 时照样生效；Esc 留在 iframe，其余按键经 `key_forward` 交给主文档 |
| 3 | 真实输入被"借用" | 不会：焦点在 iframe 时 inputGuard 不记证据 |
| 4 | 主题、语言、字号同步 | 可行 |
| 5 | 行内评论与 hunk 操作 | 通道可行，交互留给 T131 |
| 6 | 剪贴板 | 写入成功，读回没有对上，留给 T131 用 e2e 核实 |
| 7 | 滚动联动 | 可行 |
| 8 | 复刻度 | 原样使用 pierre，见截图 |

各点的依据：

1. **进程与内存**
   - iframe 的 `osProcessId` 与主框架不同：站点隔离把 `app://cupper-review` 放进了单独的进程。
   - runner 上的内存（p50/p95/max）：审查文档的渲染进程 296/304/305 MB，主文档的渲染进程 86 MB，全部进程合计约 650 MB。
   - 内存的大头是一份 React、pierre、shiki 语法，以及 Worker 产出的结果；DOM 里只有约 400 行（虚拟化）。
   - 设计的做法照旧：首次打开后常驻，空闲时预加载。
2. **焦点与快捷键**
   - 先把焦点放进审查文档，此时 `webContents.focusedFrame` 就是 iframe。
   - runner 上用 `osascript` 经 System Events 发一次真实的 ⌘⇧P，测试菜单的加速键被调用了 1 次（`keystroke: "menu 1"`）。
   - 经 DevTools 送进 iframe 的 `a`、Esc、Enter、⌘⇧P，审查文档都收到了；其中 `a`、Enter、`p` 以 `key_forward` 到达主文档，Esc 没有转发。
3. **真实输入被"借用"**
   - 用 `sendInputEvent` 发按键，这条路会触发 `before-input-event`，正是 inputGuard 用的那条。
   - 焦点在 iframe 时发 `a`、Esc、Enter、⌘⇧P，`guard.take()` 为空；焦点回到主框架后发 Enter，得到 `{kind: "key", key: "enter"}`。
   - `key_forward` 只是端口上的一条消息，不经过 inputGuard，本来就不算真实输入。
4. **主题、语言、字号同步**
   - 主题只经端口同步：经 `window` 发来的 `theme_changed` 被忽略，审查文档仍是亮色；经端口发出之后变成暗色。
   - 系统外观能传到进程外 iframe：`nativeTheme.themeSource = "dark"` 之后，iframe 里 `prefers-color-scheme: dark` 成立。
   - `init` 带来的语言写进了 `lang`（`zh-CN`）。
   - 字号和缩放走同一条消息，原型没有单独测。
5. **行内评论与 hunk 操作**
   - 端口是双向的，`ready`、`key_forward` 由审查文档发往主文档；消息形状照 5.7.5。
   - pierre 1.6.4 提供 `lineAnnotations`、`renderGutterUtility`、`selectedLines` 和 hunk 的 accept/reject 配置，够做"悬停行号出现 +"和 hunk 操作。
   - 具体的交互和 zod 校验由 T131 实现。
6. **剪贴板**
   - **现象**：在 runner 上，先把焦点放进审查文档，再以带用户手势的 `executeJavaScript` 调用 `navigator.clipboard.writeText('t104')`，promise 正常 resolve（`wrote: "ok"`）。说明 `allow="clipboard-write"` 和只放行 `clipboard-sanitized-write` 的权限处理器能配合，写入没有被拒。主进程紧接着 `clipboard.readText()`，读到的不是 `t104`（`read: false`）。
   - **原型的不足**：只记了"是否相等"，没有记下读到的值。
   - **对照**：焦点不在审查文档时，写入得到 `NotAllowedError`（本机第一次试跑时遇到）。
   - **怀疑的原因**（都没有验证）：
     1. 时序。Chromium 的异步剪贴板写入在浏览器进程里分两步提交（写入数据，然后 commit）。渲染进程的 promise 可能在 commit 真正落到系统剪贴板之前就 resolve 了，而 `executeJavaScript` 的结果经另一条 IPC 回到主进程，紧接着的 `readText()` 可能读在 commit 之前。
     2. runner 虚拟机的剪贴板服务（pasteboard）在无人值守的会话里行为异常。
   - **T131 要用 e2e 核实**：
     - 焦点在审查文档时，经 pierre 的复制入口（或一个测试用的复制动作）写入一段已知文本；
     - 在主进程里带等待地轮询 `clipboard.readText()`，或者把它粘贴进输入框，确认确实到了系统剪贴板，并记录实际读到的值；
     - 确认权限处理器对审查源只放行 `clipboard-sanitized-write`；
     - 确认焦点不在审查文档时写入被拒。
   - 复制按钮应在审查文档有焦点时使用。
7. **滚动联动**：主文档经端口发 `scroll_to {line: 4000}`，审查文档按行高把 Virtualizer 滚过去，第 4000 行出现在视口里。
8. **复刻度**
   - 截图见 `results/review-light.png`、`review-dark.png`。
   - 用的是 pierre 默认的左右对照布局：行号、变更行底色、词级高亮、shiki 配色。
   - 视觉和交互都是 pierre 原样的，这正是选这个方案的理由。

## 5. 场景 9 的测量（任务书第 4 项；runner，只记录）

条件：
- `macos-15` 托管 runner；
- 冷启动 30 次，每次全新的 userData；
- 5,000 行的 diff；
- 两份 CSP 都用发布值，webPreferences 用 e2e 构建的。

数值是 p50 / p95 / max：

| 指标 | 数值 | 说明 |
|---|---|---|
| 审查文档 `ready`（从主文档创建 iframe 算起） | 156 / 218 / 221 ms | iframe 加载、模块脚本执行、端口握手 |
| 第一屏出现行 | 560 / 753 / 765 ms | 还没有高亮 |
| 第一屏带高亮 | 2,462 / 2,957 / 3,381 ms | Worker 初始化 shiki、解析 diff、高亮 |
| App 启动到第一屏带高亮 | 3,000 / 4,105 / 5,585 ms | 包括主进程和主文档 |
| 审查文档渲染进程内存 | 296 / 304 / 305 MB | working set |
| 主文档渲染进程内存 | 86 / 86 / 86 MB | |
| 全部进程内存 | 643 / 654 / 654 MB | 浏览器、GPU、utility、两个渲染进程 |
| 滚动帧时间（每帧滚 120 px，从顶到底） | 16.7 / 18.7 / 115.3 ms | 共 24,870 帧；超过 33.4 ms（掉两帧以上）的有 184 帧，占 0.7% |
| `securitypolicyviolation` | 0 | 30 次合计 |

**对照 5.9.6，仅供参考**：
- **滚动帧时间**：预算 p95 ≤ 16.7 ms，runner 上 18.7 ms，略高。这台共享虚拟机比作者的机器慢得多：T220 在 6 路洪流下只测到 4 帧/s。
- **内存**：渲染进程的上限是 600 MB，审查文档的渲染进程约 300 MB，在上限之内。
- **冷启动**：审查文档没有单独的预算。第一屏的行在 p95 0.75 s 内出现；高亮要 2.5–3 s，可以靠空闲时预加载、常驻不销毁（第 4 节第 1 点）提前做掉。

没有数量级的问题。

## 6. 留给 T131 的事项

1. **导航**：`navigation.ts` 加上审查文档首次加载的例外。原型的写法是只放行一次，URL 必须等于 `app://cupper-review/index.html`。
2. **Virtualizer 必须就是滚动容器**：
   - 外面另套可滚动的 div 时，pierre 把所有行都放进 DOM，渲染进程约 800 MB；
   - Virtualizer 自己承担 `height` 和 `overflow: auto` 时，DOM 里约 400 行，约 300 MB。
3. **测试输入的两条路径**：
   - `sendInputEvent` 走 `before-input-event`，inputGuard 看得到，但送不进进程外 iframe；
   - DevTools 的 `Input.dispatchKeyEvent`（Playwright 用的）能送进 iframe，但不经过 `before-input-event`；
   - 菜单加速键只能用真实按键验证。
   - E2E-SEC-review-iso 和 `key_forward` 的用例要按这个分开写。
4. **剪贴板**：按第 4 节第 6 条用 e2e 核实写入是否真的到达系统剪贴板。
5. **冷启动**：高亮要 2.5–3 s。首次打开前在空闲时预加载 iframe，预热 Worker（`WorkerPoolContextProvider` 的初始化），只加载 diff 里出现的语言（`highlighterOptions.langs`）。
6. **依赖的写法**：
   - `@pierre/diffs` 锁 1.6.4；
   - `highlighterOptions` 传 `preferredHighlighter: "shiki-js"` 和 `langs`；
   - worker 入口用 `@pierre/diffs/worker/worker.js`。
