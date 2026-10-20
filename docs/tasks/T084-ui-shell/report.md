# T084 汇报（A 部分：ui-core II）

- 执行者：子 agent（前端工程师）
- 分支：`feat/T084-ui-shell`
- 完成日期：2026-10-20

T084 的 A 部分已完成：桌面 App 启动后能显示三栏外框，UNIT-ui-shell-resize 已覆盖，`cargo xtask ci pr --record` 在干净的 worktree 里全部通过，摘要已提交。规模 1,212 行，你已裁定接受。

## 做了什么

**`@cupper/ui-core` 包骨架**
- 新建 `packages/ui-core`：`package.json`、`tsconfig`（`jsx: react-jsx`、DOM 库）、Vitest，导出 `.` 和 `./styles.css`。
- 新依赖都锁定精确版本：react 19.3.1、react-dom 19.3.1、@types/react 19.3.0、@types/react-dom 19.3.0、tailwindcss 4.3.3；`apps/desktop` 加了 @tailwindcss/vite 4.3.3。

**token（`src/styles/tokens.css`）**
- 设计 5.7.10 的 token 整套落成一份 CSS：布局常量、颜色、字体、字重、字号、圆角、阴影、动效、代码高亮色。写法是 Tailwind v4 的 `@theme static`，不引 Tailwind 的默认主题。
- 颜色一律写成 `light-dark(亮, 暗)`，每个值只写一处。根元素的 `color-scheme` 默认跟随系统，`data-theme="light"` 或 `"dark"` 可以固定主题。
- 另有三点：`corner-shape: superellipse(1.5)` 用 `@supports` 包住；开启"减少动态效果"时动画时长置 0；`app-drag` / `app-no-drag` 两个工具类用于窗口拖动区。

**布局逻辑（`src/layout/`）**
- `computeLayout` 是纯函数：输入窗口尺寸和偏好，输出侧边栏（并排或浮层，240–520，并给主区留 240）、主区宽度、侧边面板（是否可见、是否自动收起、宽度和范围）。
- 侧边面板：最小 320，最大为主区宽减 352。默认宽度按 5.7.4 的公式。宽度存成最小到最大之间的比例。
- 另有键盘步进（Home / End / 方向键）和拖动的计算。
- `LayoutStore` 经 ShellAPI 的 `init` 快照、`prefs.set`、`prefs.onChange` 读写 4 个偏好键 `layout.*`，坏值回到默认。拖动时只改内存，松开那一下保存一次。

**外框（`src/shell/`）**
- 顶栏高 44，左侧给交通灯和侧边栏开关留位。右侧依次是：弱认证标记（来自握手的 `host.weak_auth`）、"打开"、侧边面板开关、"⋯"。这些都是只引用 token 的朴素占位元素，留给 T128 替换。
- 两个拖拽手柄都是 `role=separator`，支持方向键、Home、End，1px 指示线在悬停、拖动或获得焦点时显示。
- 窗口窄于 768 时侧边栏变成浮层，下面加一层遮罩，点外部或按 Esc 关闭。
- 侧边面板带 46 高的标签条，目前只有"变更"一个标签。
- 菜单的 `toggle_sidebar`、`toggle_side_panel` 已接通。

**Providers 与路由（`src/app/`）**
- 四个 Provider：
  - `ShellProvider`
  - `HostProvider`：经 `connectShell` 开一条数据连接；打不开时把客户端记成 `host_unreachable`，和主进程用同一个语义码。
  - `I18nProvider`
  - `ThemeProvider`：把 shell 报来的主题写进 `data-theme`。
- 小状态路由，`Route = {page:"new", projectId?} | {page:"session", sessionId}`。`new_session` 命令回到新建会话页，`onNavigate` 的会话目标切到会话页（只导航，不聚焦请求面板）。

**页面骨架与插槽**
- `NewSessionPage`（输入框锚在视口高度 42% 处）和 `SessionPage`（会话列最大 768，两侧各 16）。
- 5 个插槽：`sidebarHeader`、`sidebarList`、`sidebarFooter`（给 T081），`newSessionComposer`、`sessionComposer`（给 T086、T083）。
- 默认的侧边栏头部是"Cupper"加宿主状态点。

**挂载与文案**
- `App` 可以接任何 ShellAPI；`mount(api)` 先 `init` 再渲染。
- 渲染进程的 `main.ts` 把 `window.shell` 交给 `mount`，Vite 加了 Tailwind 插件。
- i18n 新增 `shell` 命名空间。

## 关键取舍

- **主题**：用 `light-dark()` 加 `color-scheme`，而不是把暗色值写两遍（一遍给 `prefers-color-scheme`，一遍给 `data-theme`）。这样 CSS 体积减半，也不会出现两份取值不一致。Electron 44 的 Chromium 原生支持这个函数，构建产物里原样保留。
- **宽度写进样式**：只用 React 的 `style={{width}}`（CSSOM 的具体属性），不写 CSS 自定义属性，也不拼 `<style>` 文本。token 全部是静态值，不接受外部输入。
- **自动收起后的恢复**：`sidePanelOpen` 只记用户的意图，放不下时只是不显示（`collapsed` 为真）。窗口变宽后自然恢复，不需要另存"收起前的状态"。
- **浮层不存 prefs**：窗口变宽回到并排模式时把浮层关掉，免得再变窄时它自己弹出来。
- **路由**：小状态路由，不引路由库，文档不产生 URL 导航。形状已写进 notes，交给 T080 接管。
- **CSS 入口**：`ui-core` 只导出 CSS 入口，由渲染进程负责引入，并在 `env.d.ts` 里声明 `*.css` 模块。这样 `ui-core` 的 TS 代码不 import 样式文件，Vitest 也只跑纯逻辑。

## 验证方式

- **UNIT-ui-shell-resize**（`packages/ui-core/src/layout/layout.test.ts`，8 条用例）覆盖：
  - 默认窗口下的各个宽度；
  - 侧边栏的范围限制；
  - 窄于 768 时的浮层；
  - 放不下 352 + 320 时自动收起、变宽后恢复；
  - 比例跨窗口尺寸保持不变；
  - 键盘和拖动；
  - store 的读取、保存，以及跟随其他窗口的改动。
- **`cargo xtask ci pr --record docs/tasks/T084-ui-shell/ci/`**（干净的 worktree，提交 e62ea3c）全部通过：

  | 作业 | 结果 |
  |---|---|
  | rust | 1,721 通过 |
  | protocol | 185 通过 |
  | ts | 361 通过，含 test-ui-core |
  | license | 通过 |
  | testdata-scan | 通过 |

  ui 和 e2e 作业还没实现，跳过；sensitive-words 因为没有本地词表而跳过。摘要 `pr-e62ea3c5b779.json` 已提交。license 这次一遍就过了。
- **桌面 App 启动后显示外框**：用未打包的 e2e 构建（`vite build --mode e2e`）带 DevTools 端口启动，数据目录放在临时目录，用 CDP 截图和操作：
  - 外框显示正常；
  - 侧边面板手柄：Home → 320，End → 640，向右一步 → 624；
  - 700 宽时侧边栏为浮层，Esc 能关掉；
  - 偏好里存的是比例。

  宿主在这次实测里起不来：打包 App 需要 `cargo xtask runtime-pack`，它要联网下载，所以没跑。因此状态点是红色。
- **审阅截图**在 `docs/tasks/T084-ui-shell/screenshots/`：1280×820，系统字体（SF Pro、苹方）；亮色和暗色的外框（侧边面板打开）、手柄获得焦点、700 宽的浮层。
- **F2**：没有查看本机的 Codex App 包，所有取值都来自设计 5.7.10 和 ui-spec。
- **许可证**：`cargo xtask ci license` 目前只查 Rust 依赖，新增的 npm 依赖是我自己核对的：react、react-dom、scheduler、tailwindcss、@tailwindcss/* 都是 MIT；lightningcss 是 MPL-2.0，只在构建时使用。
- **规模**：1,212 行（实现 1,102，测试 110；不含 notes、lock、截图），按你的裁定接受。各模块的估计与实际对照在 notes 里。

## 提交列表

```
5b47932 docs(T084): record the ci pr summary
e62ea3c docs(T084): record part A's notes, findings and review screenshots
d888446 feat(desktop): mount ui-core in the renderer
84b4810 feat(ui-core): add the three-column shell
574ff16 feat(ui-core): add the providers, the state router and the page skeletons
a7df948 feat(i18n): add the shell namespace
2e8d2c7 feat(ui-core): add the package with the design tokens and the layout logic
6c2932b docs(T084): estimate part A after the split
8c1a1c6 Merge branch 'main' into feat/T084-ui-shell
51f9499 docs(task): record the T084 checks, size estimate and split proposal
```

8c1a1c6 这个合并提交是我在你更正前按第一条消息做的，你已核对，确认保留。

## 遗留问题 / 范围外发现

- **交给 T118**：渲染进程经 host-client 和 protocol 打包进了 zod 4。zod 用 `new Function("")` 探测能否 JIT，被 `require-trusted-types-for 'script'` 拦下，控制台报 "This document requires 'TrustedScript' assignment"。zod 自己捕获了异常，功能不受影响，但这算一次 Trusted Types 违规，UI-csp-zero 会计入。修法是在共用的位置调用 `z.config({ jitless: true })`，位置由 T118 定。本任务没改。
- **交接 T080**：连续快速按手柄键时，主进程回传的旧值会让宽度短暂跳回，最后一次回传会纠正过来。接管状态时丢弃过期的回传即可。
- **没做的部分**（等你分到后续任务）：
  - 侧边面板开合的 spring 动画（0.5 s，bounce 0.1）；
  - 顶栏"打开"和"⋯"的菜单（目前只是占位按钮）；
  - 会话标题和 agent 标识。
- **ESLint 规则**：`ui-core` 禁止 import `electron` 和 `node:*` 的规则属于 T080 的安全规则，本任务没加。
- **Electron 二进制下载了一次**：第一次 require `electron` 时，它的 npm 包自动下载了 Electron 44.9.0 本体。这是项目自己声明的依赖，不是 agent CLI。