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
