# T084 工作笔记

## 开工前的核对

- `packages/ui-core` 不存在；渲染进程只有空的 `index.html`；preload 已暴露 `window.shell`（T076，`SHELL_GLOBAL`）。
- 设计 5.1.3 给 ui-core 列的依赖里没有路由库，会话页、新建会话页的路由用一个小的状态路由即可。
- `handoff-notes.md` 里没有第一列含 T084 的行；任务书"交接事项"里的三条（F2、文案走 i18n、T080 的交接不接）照办。
- `tests/ids.toml` 的 T084 段有 UI-shell、UI-shell-narrow、UI-csp-zero；基础组件、拖动和键盘调整、行类型注册表、元数据比对的 Vitest 用例还没有编号，开工时补进 T084 段（例如 UNIT-ui-shell-resize、UNIT-ui-components、UNIT-ui-row-registry、UNIT-ui-baseline-meta）。
- 5.7.10 的 token 共约 100 个：颜色约 40 个，每个有亮、暗两个值；暗色要写两遍（跟随系统的 `prefers-color-scheme` 和固定的 `data-theme`），这一份 CSS 本身就有 200 多行。

## 规模估计（开工前）

按格式化后的写法估（prettier 宽度 100）。字体、截图基线等夹具不计。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 包骨架 | `package.json`、`tsconfig`、Vitest、Tailwind v4、入口 | 60 | — |
| token CSS | 布局常量、颜色（亮色、暗色两处）、字体与字号、圆角（含 superellipse）、阴影、动效、Tailwind `@theme` 映射 | 240 | — |
| 外框 | 顶栏（交通灯留位）、侧边栏与拖动、窄窗口浮层（点外部、Esc）、主区、侧边面板与标签条、拖动与键盘调整、自动收起与恢复、布局 store（经 ShellAPI 的 prefs） | 380 | 100 |
| Providers 与路由 | `ShellProvider`、`HostProvider`（`connectShell`）、`I18nProvider`、`ThemeProvider`（跟随系统，`data-theme` 固定）；两个页面的路由 | 150 | — |
| 页面骨架与插槽 | `<NewSessionPage>`、`<SessionPage>`，输入框位置、侧边栏头部、项目列表、底部的插槽 | 100 | — |
| 行类型注册表 | 类型 → 组件，未知类型占位 | 50 | 30 |
| 基础组件 | 工具栏按钮、菜单、tooltip、开关、警告框（Radix），样式只引用 token | 200 | 120 |
| 截图设施 | Playwright 配置、截图页（浏览器版 ShellAPI 加 TS mock 宿主）、固定渲染因素、`MAIN_DOCUMENT_CSP` 的复用、基线脚本与元数据比对 | 270 | 60 |
| 截图用例 | UI-shell（亮、暗）、UI-shell-narrow、UI-csp-zero | — | 120 |
| `xtask ci ui` | 作业（机器级锁）、`ci pr` 的按路径过滤 | 80 | 40 |
| 渲染进程挂载与文案 | `renderer` 挂载 ui-core、注入 `window.shell`；i18n 的键 | 70 | — |
| 合计 | | 1,600 | 470 |

合计约 **2,070 行**，超出约 1,500 行的上限约 40%。按 brief，开工前停下来汇报。

### 拆分建议

brief 建议的两分法里，第一部分仍约 1,670 行，超线。建议拆成三份：

| 部分 | 内容 | 估计 |
|---|---|---|
| A（本任务，关键路径） | 包骨架、token、外框（含窄窗口与自动收起）与拖动、键盘调整的 Vitest 用例、Providers 与路由、页面骨架与插槽、渲染进程挂载 | 约 1,110 |
| B（新编号，紧跟 A） | 截图设施（Playwright、字体夹具、基线脚本与元数据比对）、UI-shell、UI-shell-narrow、UI-csp-zero、`xtask ci ui` 与 `ci pr` 的过滤 | 约 570 |
| C（新编号，可与 B 并行） | 行类型注册表、基础组件及其用例 | 约 400 |

- A 合并后 T081、T086、T083 就能在外框和插槽上开工；它们的截图用例要等 B，B 可以在它们开工期间完成。
- 如果更想保持两份：A 与 B 合并为第一部分（约 1,680 行），C 为第二部分。

## 裁定之后：A 部分的规模估计（计划 v1.3.22）

编号只有 UNIT-ui-shell-resize（main 上已登记）。按格式化后的写法估，不计 notes、lock。上限约 1,200，硬停止线 1,300。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 包骨架 | `package.json`、`tsconfig`、Vitest、入口 | 55 | — |
| token CSS | 5.7.10 全部 token；颜色用 `light-dark()` 只写一处，主题由 `color-scheme` 切换；Tailwind `@theme` | 150 | — |
| 布局逻辑与 store | 纯函数（窗口宽 → 布局状态、侧边面板比例与自动收起、键盘步进）；store 经 ShellAPI 的 prefs 读写 | 210 | 100 |
| 外框组件 | 顶栏、侧边栏（拖动、浮层）、主区、侧边面板与标签条、拖拽手柄（指针与键盘） | 270 | — |
| Providers 与路由 | 四个 Provider、状态路由、`onCommand`/`onNavigate` 接线 | 170 | — |
| 页面骨架与插槽 | 两个页面、五个插槽 | 60 | — |
| 渲染进程挂载与文案 | `main.tsx`、Vite 的 Tailwind 插件、`shell` 命名空间 | 60 | — |
| 合计 | | 975 | 100 |

合计约 **1,075 行**。以前的任务实现常超估计 15–30%，所以这次从紧写：每个模块完成后统计，推算超过 1,150 就停下汇报。

## A 部分的实现记录

### 路由状态的形状（交给 T080）

```ts
type Route = { page: "new"; projectId?: string } | { page: "session"; sessionId: string };
```

- `RouterProvider` 持有它，`useRouter()` 给出 `{ route, navigate }`；没有 URL，文档不导航。
- 菜单的 `new_session` 回到新建会话页；`onNavigate` 的 `session` 目标切到会话页，只导航，不聚焦请求面板。`settings` 目标先忽略（设置页不在本任务）。
- 会话页按 `sessionId` 加 `key`，切换会话时组件状态重建。

### 布局状态与 prefs

- 纯函数在 `layout/layout.ts`：`computeLayout(窗口尺寸, 偏好, 浮层是否打开)` 给出侧边栏的模式（并排或浮层）、宽度与范围，主区宽，侧边面板是否显示、是否被自动收起、宽度与范围。UNIT-ui-shell-resize 只测这一层和 store。
- 偏好键：`layout.sidebarOpen`、`layout.sidebarWidth`、`layout.sidePanelOpen`、`layout.sidePanelRatio`（0–1，`null` 表示默认宽度）。启动时读 `ShellInfo.prefs`，坏值回到默认；之后经 `prefs.onChange` 跟随其他窗口。
- "自动收起后恢复"：`sidePanelOpen` 只记用户的意图，放不下时 `visible` 为假、`collapsed` 为真，窗口变宽后自然恢复，不需要另存"收起前的状态"。放不下时按开关只改意图，界面上暂时看不到变化。
- 浮层的开合不存 prefs；窗口变宽回到并排模式时关掉浮层，免得再变窄时它自己弹出来。
- 拖动时每次移动只改内存，松开时存一次；键盘每按一次存一次。已知的小问题：连续快速按键时，主进程回传的旧值会让宽度闪一下，最后一次回传会把它纠正回来。T080 接管状态时可以按序号丢弃过期回传。

### 宽度怎样写进样式

宽度来自用户的拖动和窗口尺寸，经 React 的 `style={{ width }}` 写成元素的具体属性（CSSOM），不写 CSS 自定义属性，也不拼 `<style>` 文本。token 全部是静态 CSS，不接受外部的值。

### token 的写法

- 颜色每个写成 `light-dark(亮, 暗)`，只写一处；根元素的 `color-scheme` 决定取哪个：默认 `light dark` 跟随系统，`data-theme="light"`/`"dark"` 固定。`ThemeProvider` 把 ShellAPI 报来的主题写进 `data-theme`。
- 不引 Tailwind 的默认主题，所以只有本文件定义过的 token 能生成工具类（例如 `bg-surface`、`text-fg-tertiary`、`rounded-lg`、`shadow-card`）。`--spacing` 是 4px。
- 暗色的输入框阴影换成内描边，用 `light-dark()` 在同一个 `--shadow-composer` 里切换。
- 多加了一个 `--layout-traffic-lights-w`（80px）：顶栏左侧给交通灯和侧边栏开关留位。窗口尺寸两个 token 是主进程的事，CSS 里没有写。
- 减少动态效果时把动画和过渡时长全部置 0。`corner-shape: superellipse(1.5)` 用 `@supports` 包住。

### 实测

- 未打包的 e2e 构建（`vite build --mode e2e` 加本仓库的 Electron）带 DevTools 端口启动，数据目录放临时目录，用 CDP 截图和操作。没有运行时包（`cargo xtask runtime-pack` 要联网下载），所以宿主起不来，`host.open()` 失败。`HostProvider` 这时把客户端记成失败（用主进程同一个语义码 `host_unreachable`），状态点为红色；不这样做的话，客户端会一直停在"连接中"。
- 截图在 `screenshots/`：1280×820、系统字体（SF Pro、苹方），亮色与暗色的外框（侧边面板打开）、键盘调整后手柄获得焦点、700 宽的浮层侧边栏。
- 键盘：侧边面板手柄 Home → 320，End → 640，向右一步 → 624；拖动在截图里没有覆盖，靠 UNIT-ui-shell-resize 的纯函数用例。
- 窄窗口：700 宽时侧边栏为浮层，Esc 关闭；侧边面板在 700 宽仍放得下（浮层不占宽度，700 ≥ 352 + 320）。
- **发现（范围外，未改）**：渲染进程打包进了 zod（`@cupper/protocol` 经 `@cupper/host-client` 带进来）。zod 4 在第一次解析对象时用 `new Function("")` 探测能否 JIT，被 `require-trusted-types-for 'script'` 拦下，控制台报 "This document requires 'TrustedScript' assignment"。zod 自己捕获了异常，功能不受影响，但这是一次 Trusted Types 违规，T118 的 UI-csp-zero 会把它计进去。修法是在渲染进程第一次解析之前 `z.config({ jitless: true })`，放在 `@cupper/protocol` 或渲染进程入口，留给编排者决定归属。
- 依赖的许可证：react、react-dom、scheduler、tailwindcss、@tailwindcss/vite 及其 oxide 都是 MIT；lightningcss 是 MPL-2.0，只在构建时使用。`cargo xtask ci license` 目前只查 Rust 依赖。

### 没做的部分

- 侧边面板开合的 spring 动画（0.5 s，bounce 0.1）没有做，留给 T128 或后续视觉任务。
- 顶栏的"打开"和"⋯"只是占位按钮，没有菜单；会话标题、agent 标识等着状态层。
- `ui-core` 禁止 import `electron`、`node:*` 的 ESLint 规则属于 T080 的安全规则，本任务没加。
- 按 F2：没有查看本机 Codex App 包，取值全部来自设计 5.7.10 和 ui-spec。

### 行数（与开工前估计对照，不含 notes、lock、截图）

| 模块 | 估计 | 实际 |
|---|---|---|
| 包骨架 | 55 | 52 |
| token CSS（含 `index.css`） | 150 | 206 |
| 布局逻辑与 store | 210 | 259 |
| 外框组件 | 270 | 292 |
| Providers 与路由（含 `App.tsx`） | 170 | 204 |
| 页面骨架与插槽 | 60 | 41 |
| 渲染进程挂载与文案 | 60 | 48 |
| 测试 | 100 | 110 |
| 合计 | 1,075 | **1,212** |

超出约 1,200 的上限约 1%，低于硬停止线 1,300。功能已经写完；按"推算贴线也先停下"，在跑 CI 记录之前停下汇报。
