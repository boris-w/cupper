# T128 工作笔记

## 开工前的核对

- `docs/tasks/handoff-notes.md` 里第一列含 T128 的行：没有。T084 的 notes 里与本任务有关的：只有 `tokens.css` 里定义过的 token 能生成工具类；颜色是 `light-dark()`。
- `tests/ids.toml` 的 T128 段已有 UNIT-ui-components、UNIT-ui-row-registry。
- 规格：ui-spec 2.1（工具栏按钮：高 28、左右 8、`--radius-lg`、tertiary 色、悬停 `--color-hover`、打开状态 `--color-btn-secondary`、tooltip 延迟出现并带快捷键）、3.4（菜单外边距 4，条目 5 × 8）、3.5（菜单与条目 `--radius-xl`，tooltip `--radius-2xl`）、3.6（`--shadow-menu`）、4.3（警告框：`--radius-lg`，1px 警告色 30% 的边框，内边距 12 × 8，最大宽 77%）、4.7（开关 32 × 19，滑块 13，开启时强调色）。
- 行类型：设计 5.7.5 的消息流行（回合标题、用户消息、活动组、工具行、助手正文、思考、计划、提示、错误、迟到的工具输出、请求记录、回合改动），具体组件由 T082、T083 等注册。

## 规模估计（开工前）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 依赖与测试环境 | Radix 三个包、jsdom 与 Testing Library；Vitest 收 `.tsx` 用例；jsdom 缺的 `ResizeObserver` 等 | 20 | — |
| 基础组件 | 工具栏按钮（含 tooltip）、菜单、tooltip、开关、警告框、两个自绘图标 | 160 | — |
| 替换顶栏占位 | 五个按钮改用工具栏按钮，"打开"和"⋯"挂空菜单；`App` 加 tooltip 的 Provider | 35 | — |
| 行类型注册表 | 注册、查找、重复注册报错、未知类型的占位 | 45 | — |
| 文案与导出 | i18n 键、`index.ts` | 15 | — |
| 测试 | UNIT-ui-components、UNIT-ui-row-registry | — | 165 |
| 合计 | | 275 | 165 |

合计约 **440 行**。按以往约 15% 的偏差推算约 500，贴近停止线：写得从紧，每个模块完成后统计，推算到 500 就停下汇报。

## 实现记录

### 组件

- **工具栏按钮**（`components/ToolbarButton.tsx`）：原生 `<button>`，`label` 同时是无障碍名称和 tooltip 文字，`shortcut` 跟在 tooltip 文字后面；`pressed` 写成 `aria-pressed`。按下状态和"菜单打开"（Radix 的 `data-state="open"`）都用 `--color-btn-secondary` 底；焦点是 `--color-ring` 的 2px 环；禁用时半透明、不响应点击。
- **tooltip**（`Tooltip.tsx`）：Radix Tooltip。`TooltipProvider` 放在 `App` 里一次，延迟 500 ms；底色和文字用 `--color-tooltip` / `-fg`，圆角 `--radius-2xl`，`text-sm`。ui-spec 3.6 的 tooltip 阴影没有进 5.7.10 的 token，暂不加。
- **菜单**（`Menu.tsx`）：Radix DropdownMenu。`<Menu trigger={<ToolbarButton …/>}>` 把按钮当触发器（`asChild`），触发器同时套着 tooltip：两个 Radix 的 Slot 都把属性合并到同一个按钮上，菜单的 `data-state` 覆盖 tooltip 的。内容区 `--color-surface-elevated`、`--radius-xl`、`--shadow-menu`、外边距 4；条目 5 × 8、`--radius-xl`、高亮用 `--color-hover`、禁用用 tertiary 色。
- **开关**（`Switch.tsx`）：Radix Switch，32 × 19，滑块 13，开启时 `--color-accent`，关闭时 `--color-active`；滑块用 `--color-surface-elevated`（暗色里是深色滑块，T106 核对暗色时再看）。
- **警告框**（`Alert.tsx`）：`role="status"`（错误时 `role="alert"`），`--radius-lg`，1px 边框为警告色（或危险色）的 30%，内边距 12 × 8，最大宽 77%，`action` 放在框里的右侧。
- **图标**（`icons.tsx`）：自绘的面板、"⋯"、下拉箭头，不用 Codex 的图标（F2）。

### 顶栏

侧边栏开关（⌘B）、弱认证标记（tooltip 是完整说明）、"打开"（带下拉箭头）、侧边面板开关（⌥⌘B）、"⋯"都改成工具栏按钮。"打开"和"⋯"挂着空菜单，菜单项由 T153 填。快捷键文字与主进程菜单的加速键一致，是符号，不走 i18n。

### 行类型注册表

`rows/registry.tsx`：`RowRegistry<R extends { kind: string }>`，`register(kind, component)`，重复注册同一个 kind 直接抛错（是代码错误，不是运行时状态）；`resolve(kind)` 找不到时返回 `UnsupportedRow`，只显示"此版本不支持显示这项内容"（`stream:unsupported_row`），不渲染行里的任何数据。`<RowView registry row>` 按 kind 画出对应组件。T082、T083 各自建一个注册表实例或共用一个，在模块加载时注册。

### 测试环境

组件测试是 `components/components.test.tsx`，首行 `// @vitest-environment jsdom`，用 Testing Library 和 user-event；jsdom 没有 `ResizeObserver`，用 `vi.stubGlobal` 补一个空实现。Vitest 的 `include` 改为 `src/**/*.test.{ts,tsx}`。

### 截图基线

外观变了，四张 UI-shell 基线都只在顶栏右侧一组有差异：弱认证标记改成按钮的 13 号字、"打开"后多了下拉箭头、"⋯"换成自绘图标。用 `pnpm ui:baseline` 重新生成（Chromium、macOS 版本不变），逐张看过。UI-csp-zero 加了打开菜单和显示 tooltip 两步，Radix 经 portal 渲染并插入自己的样式，违规仍为 0。

### 审阅截图

`screenshots/`：在使用系统字体的桌面构建（未打包的 e2e 构建）里，1280 × 820 截取：顶栏（侧边面板打开）、键盘焦点下的 tooltip（"切换侧边栏 ⌘B"）、打开的"⋯"菜单（还是空的）亮色和暗色。宿主没起来，状态点是红色。开关和警告框目前没有界面用到，没有截图。按 F2，没有查看本机 Codex App 包。

### 行数

| 模块 | 估计 | 实际 |
|---|---|---|
| 依赖与测试环境 | 20 | 9 |
| 基础组件与图标 | 160 | 175 |
| 替换顶栏占位（含 `App`） | 35 | 31（另删 37） |
| 行类型注册表 | 45 | 43 |
| 文案与导出 | 15 | 14 |
| 测试（含 UI-csp-zero 加的 6 行） | 165 | 141 |
| 合计 | 440 | **413** |
