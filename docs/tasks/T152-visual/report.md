# T152 汇报

- 执行者：子 agent（前端，ui-core）
- 分支：`feat/T152-visual`，PR #121 https://github.com/boris-w/cupper/pull/121 ，头提交 0da88a4a
- 完成日期：2026-11-16
- **状态**：代码、用例、基线都已推送。
  - PR 在 0da88a4a 上的检查：已通过 6 项（compat-framework、license、protocol、testdata-scan、ts、ui）；e2e、rust 交汇报时还在跑。
  - MAN-ui6-shell 由你安排作者做，结果出来前不合并。

## 做了什么

| 模块 | 提交 | 内容 |
|---|---|---|
| M6 暗色 token | 7561d9ff | 暗色输入框 #363636；拖文件到输入框上时用 surface-elevated（#2D2D2D） |
| M5a 侧边栏范围 | 4df0445c | 188–468，默认 288，主区至少 240 |
| M5b 回到底部按钮 | bd8a29d3 | 32 圆，离输入框 24；回合运行中宽 40、三个跳动的点，悬停变回箭头；离底部超过 112 才出现。原来的 `bg-bg` 写错了，按钮实际是透明的，已修 |
| M1 主题偏好 | 321631b1 | `appearance.theme`（跟随系统 / 浅色 / 深色）驱动 `nativeTheme.themeSource`，在第一个窗口之前、以及每次改动时设置；另加 `appearance.translucentSidebar` |
| M2 外观分区 | eda9c035 | 新增"界面"一节：主题分段控件、半透明侧边栏开关 |
| M3 半透明侧边栏 | 758a499d | 窗口 `vibrancy: 'menu'`、透明底。只在 Electron 里、开关打开时加 `data-translucent`：侧边栏和顶栏透出材质，会话区和设置页内容画 surface。系统"降低透明度"时，CSS 媒体查询让侧边栏回到不透明；窄窗口浮层始终不透明 |
| M4 交通灯随缩放 | 9d5068d2 | 菜单自己处理缩放：⌘0，⌘+ 和 ⌘=，⌘−，每步半级，与原来的 role 一致。每步之后、以及页面加载时，交通灯放到 (16, (44 × 缩放 − 14) / 2)；坐标公式有单元测试 |
| M7 用例 | 973893be | 新增 `layout.spec.ts`：UI-layout、UI-theme 的 DOM 断言，加运行中回合的亮色、暗色截图 |
| M8 侧边面板 spring | 82cb72cf | 0.5 s、bounce 0.1，用 Web Animations 和 `linear()` 缓动；减弱动态效果时没有动画；拖动边缘不触发 |
| 文档 | da1087c1 | notes.md、man-ui6-shell.md 检查表 |
| 基线一 | 90421d53 | 7 张：settings-appearance 亮暗（新增"界面"一节）；stream-plan-steps 亮暗和 stream-late-original（药丸和回到底部按钮从离输入框 8 上移到 24，按钮是运行中的三点形态）；新增 theme-running 亮暗 |
| ui-baseline 加 `all` 输入 | 0d1b65ef | 布尔，默认 false；true 时用 `--update-snapshots=all`，输入值经 env 传入 |
| 基线二 | 0da88a4a | all=true 跑出的结果（43816293235）里有 20 张字节不同，逐张做了像素差，提交其中 18 张。见下方"截图基线" |

## 截图基线

- **基线一**（ui-baseline 43814431340）：7 张，原因见上表。
- **基线二**（ui-baseline 43816293235，all=true）：提交的 18 张，差异都是暗色输入框和请求面板变色（#2D2D2D → #363636，浅色部分 #3D3D3D → #454545）：
  - 窗口底部的输入框：composer-dark、session-banners-dark、sidebar-dark、shell-dark；
  - 新建会话页的输入框：new-session-agent-picker / no-project / project-picker / projects 的暗色；
  - 输入框位置上的请求面板：request-permission-dark、request-records-dark；
  - 消息流下方的输入框：stream-changes / code / error / fold / markdown / plan-markdown / tools 的暗色；
  - review-commit-dark：提交对话框遮罩下透出的同一个输入框，#161616 → #1B1B1B。
- **没提交的 2 张**：settings-agents 的亮色和暗色，差异是零散像素、最多差 1 级，属于渲染噪声。

## 关键取舍

- **截图阈值**：保持 0.2，细微差异用 DOM 断言，写在 notes 里。阈值以内的取值变化，用 ui-baseline 的 `all` 输入刷新基线。
- **"降低透明度"**：先用 CSS 媒体查询。Chromium 是否支持，留给 MAN-ui6-shell 第 2.3 项确认；不支持就改走 `nativeTheme.prefersReducedTransparency`。
- **截图里的侧边栏**：浏览器没有窗口材质，`data-translucent` 只在 Electron 里设，所以截图不受半透明影响。
- **缩放菜单项**：文案走 i18n（实际大小、放大、缩小），和视图菜单里 cupper 自己的命令一致。
- **代码高亮**：本来就是 `hl-*` 加 `light-dark()` token，暗色已经生效，没改。
- **截图页字体栈**：T143 已经整套接好中文子集，没改。
- **测试 ID**：主题偏好、外观分区、交通灯坐标这几组单元测试挂在 UI-theme、UI-layout 下；分支上不改 `tests/ids.toml`。

## 验证方式

- **本机**：ui-core（25 个文件、170 个用例）、desktop（125 个）、shell-api、i18n、截图工具的 vitest 全过；tsc 全过；全仓 prettier、eslint 都过。
- **新 spec**：`layout.spec.ts` 的 DOM 断言在本机 headless Chromium 上全部通过：
  - 顶栏 44；侧边栏 288、188、468；
  - 回到底部按钮 40 × 32、悬停 32、离输入框 24、在列中轴；
  - 侧边面板 spring 时长 500 和 `linear()`，减弱动态效果时为 0；
  - 暗色输入框 rgb(54, 54, 54)，运行时切到亮色后变回白色。
- **runner**：两次 ui-baseline 都成功；PR 的 ui 检查在 0da88a4a 上通过。
- **PR #121 的检查**（交汇报时）：已通过 compat-framework、license、protocol、testdata-scan、ts、ui；e2e、rust 还在跑。

## 提交列表

```
0da88a4a test(ui): dark baselines with the composer's #363636, from ui-baseline 43816293235
0d1b65ef ci: ui-baseline rewrites every baseline when asked
90421d53 test(ui): baselines for T152, from ui-baseline 43814431340
da1087c1 docs(task): T152 notes and the MAN-ui6-shell checklist
82cb72cf feat(ui-core): the side panel opens and closes on Codex's spring
973893be test(ui): UI-layout and UI-theme for T106's sizes and dark values
9d5068d2 feat(desktop): the traffic lights stay centred in the title bar at each zoom
758a499d feat: the translucent sidebar over the window's menu material
eda9c035 feat(ui-core): the theme and the translucent sidebar in the appearance section
321631b1 feat(desktop): the theme follows the system unless the settings pin it
bd8a29d3 feat(ui-core): the way back to the bottom as Codex has it
4df0445c fix(ui-core): the sidebar is dragged between 188 and 468
7561d9ff fix(ui-core): the composer's dark surface as T106 checked it
```

## 规模

`cargo xtask size`：+708 −77，基线图片不计；超过停止线 700（你同意了），在硬线 820 以内。

## 遗留问题 / 范围外发现

- **PR 检查**：e2e、rust 交汇报时还在跑，请以 PR 上的最终结果为准。
- **MAN-ui6-shell**：检查表在 `docs/tasks/T152-visual/man-ui6-shell.md`，重点看四处：
  - 半透明侧边栏和顶栏的观感，顶栏与会话区之间要不要细线；
  - "降低透明度"是否生效；
  - 缩放后交通灯的位置；
  - 侧边面板 spring 的手感，以及动画期间面板内容的重排。
