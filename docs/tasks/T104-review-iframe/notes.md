# T104 notes

## 许可证（开工前核对，2026-11-04）

`@pierre/diffs` 最新版 1.6.4（设计核对时是 1.5.2）。在空目录里 `npm install --ignore-scripts` 后，逐个读取 52 个包的 `package.json`：

| 许可证 | 包 |
|---|---|
| Apache-2.0（`package.json` 里写成小写 `apache-2.0`） | `@pierre/diffs`、`@pierre/highlights`、`@pierre/theme`、`@pierre/theming` |
| MIT | 46 个：shiki 4.6.0 与 `@shikijs/*`、hast/mdast/unist 一系、`lru_map`、`hast-util-to-html`、`oniguruma-to-es`、`regex*` 等 |
| ISC | `@ungap/structured-clone` |
| BSD-3-Clause | `diff` 9.0.0 |

全部在 ADR-11 的允许清单里。`@pierre/highlights` 的 NOTICE 逐一列出了随包主题的来源，都是 MIT 或 Apache-2.0；文中提到的 GPL-3.0 只是说明他们没有收录 Aurora X 的原版。`@pierre/theme` 的 NOTICE 附有 primer/github-vscode-theme 的 MIT 原文。`@pierre/highlights` 带一个 `highlights.wasm`；设计固定用 `shiki-js`，原型会核对它是否进入审查文档的产物。

## 规模估计（开工前，报给编排者等回复）

停止线 600，硬停止线 700（原型代码计入；`docs/` 不计）。

| 模块 | 行数 |
|---|---|
| `package.json`、esbuild 构建脚本（主进程、主文档、审查文档、Worker 四个入口） | 50 |
| 主进程：两个源的 `app` 协议（直接 import `apps/desktop` 的 `app-protocol`、`permissions`、`webPreferences`、inputGuard）；审查文档首次加载的导航例外；测试用的菜单加速键；用 `executeJavaScript` 读两个文档的检查结果，用 `getAppMetrics` 取进程和内存；写 JSON | 150 |
| preload（只在主框架；用来核对 iframe 里没有它） | 10 |
| 主文档：iframe（sandbox、allow）、MessageChannel 握手与伪造端口检查、`init`/`render`/`theme_changed`/`scroll_to`、接收 `key_forward`、违规计数 | 100 |
| 审查文档：只认第一个端口；pierre（`workerFactory`、`shiki-js`）渲染 5,000 行 diff；主题；`scroll_to`；按键转发；违规计数；滚动帧时间采样；剪贴板 | 130 |
| Worker 入口、审查文档的 CSP 常量、合成 diff 夹具 | 40 |
| 测量循环与汇总（冷启动 30 次，p50/p95/max） | 50 |
| workflow | 40 |
| README（跑法，详细的写在本目录） | 15 |
| 合计 | ~585 |

照 T141 偏高 38% 的经验，可能到 700 左右。控制办法：说明写在 `docs/`（不计行数）；汇总只出原始 JSON 和三个分位数；检查项尽量用 `executeJavaScript` 读，不另写消息。

## 依赖与运行方式（方案）

- 原型自带 `package.json` 和 `package-lock.json`（像 SP-4a 那样，不进 pnpm 工作区，根 lockfile 不动）：`@pierre/diffs` 1.6.4、`react`/`react-dom` 19.3.1（与 ui-core 相同）、`esbuild`，都锁精确版本。
- Electron 不另装，直接用工作区里 `apps/desktop` 的 44.9.0，与产品同一个二进制。
- 本机：构建后启动一次，看检查项的 JSON，不跑测量循环。
- runner（`macos-15`）：
  1. `pnpm install --frozen-lockfile`；
  2. 在原型目录 `npm ci`；
  3. 构建；
  4. 冷启动 30 次（每次全新的 userData），每次打开 5,000 行 diff 并滚到底；
  5. 结果 JSON 作为 artifact 上传，我下载后提交到 `docs/tasks/T104-review-iframe/results/`。

两种触发方式请编排者选：
- (a) 一个通用的 `workflow_dispatch` workflow，例如 `spike-run.yml`，输入为原型目录和命令。`workflow_dispatch` 要求 workflow 文件已经在默认分支上，所以要你先把它放进 main 才能在本分支上触发。
- (b) 只在 `spike/T104-review-iframe` 分支 push 时运行的 workflow：放在本分支就能跑，不用动 main；合并前删掉。

## 实现记录

- 编排者选了 (a)：通用的 `spike-run.yml`。它单独作为分支上的第一个提交 ed759a8，经 PR #65 先合进 main。
- 原型提交后，`cargo xtask size` 显示 +1,792，其中 1,197 行是原型自己的 `package-lock.json`。工具按文件名只排除 `Cargo.lock`、`pnpm-lock.yaml`，不排除 npm 的 lockfile；按守则"不含 lock 文件"计算是 595 行（workflow 76，原型 519）。
- **Virtualizer 必须就是滚动容器**：一开始在外面另套了一个可滚动的 div，pierre 的 `Virtualizer` 以为所有行都可见，6,000 行全部进了 DOM，审查文档的渲染进程约 800 MB。改成由 `Virtualizer` 自己承担 `height: 100vh; overflow: auto` 之后，DOM 里约 400 行，内存约 310 MB。T131 要照这个写。
- **两条输入路径**：
  - `webContents.sendInputEvent` 会触发 `before-input-event`（inputGuard 用的那条路），但按键到不了进程外 iframe 里的文档；
  - 经 DevTools 的 `Input.dispatchKeyEvent`（Playwright 的做法）能送到 iframe，但不经过 `before-input-event`。
  - 所以第 3 点用前者验证（焦点在 iframe 时 inputGuard 不记证据），`key_forward` 和 Esc 的留存用后者验证。菜单加速键这两条路都触发不了，要靠真实按键（`osascript` 经 System Events）。
- **剪贴板**：第一次在 iframe 里 `writeText` 得到 `NotAllowedError`，因为当时焦点已经回到主文档；改成写之前先把焦点放进审查文档。
- **本机误跑了一次侵入性步骤**：本机试跑 `measure.mjs 2` 时，第一次运行打开了 `T104_KEYSTROKE` 和 `T104_CLIPBOARD`：
  - 用 `osascript` 发了一次 ⌘⇧P，并把 App 抢到前台约 1 秒，菜单加速键触发了，原型拿到了结果；
  - 剪贴板写入因为上一条原因失败（`NotAllowedError`），用户的剪贴板没有被改。
  - 之后改成只有 `GITHUB_ACTIONS=true` 时才开这几步，本机不会再发生。
