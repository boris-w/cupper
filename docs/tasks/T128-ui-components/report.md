# T128 汇报（ui-core II-c：基础组件与行类型注册表）

- 执行者：子 agent（前端工程师）
- 分支：`feat/T128-ui-components`
- 完成日期：2026-10-20

T128 已完成：五个基础组件和行类型注册表都已加上，顶栏的占位已全部换成工具栏按钮。`cargo xtask ci pr --record` 在干净的 worktree 里全部通过，摘要已提交。改动 413 行，在约 500 行的停止线以内。

## 做了什么

**五个基础组件**（`packages/ui-core/src/components/`）
- 样式只引用 token，都能用键盘操作，焦点环用 `--color-ring`。
- **工具栏按钮**：原生按钮外面套一个 tooltip。
  - `label` 同时用作无障碍名称和 tooltip 文字，`shortcut` 跟在 tooltip 文字后面。
  - `pressed` 写成 `aria-pressed`。按下时和菜单打开时都用 `--color-btn-secondary` 底。
  - 禁用时半透明，不响应点击。
- **tooltip**：Radix Tooltip。`TooltipProvider` 在 `App` 里只放一次，延迟 500 ms；样式用 `--color-tooltip`、`--radius-2xl`、`text-sm`。
- **菜单**：Radix DropdownMenu，`<Menu trigger={<ToolbarButton …/>}>`。
  - 内容区用 elevated 底、`--radius-xl`、`--shadow-menu`，外边距 4。
  - 条目内边距 5 × 8，高亮用 `--color-hover`，禁用用 tertiary 色。
- **开关**：Radix Switch，32 × 19，滑块 13，开启时用强调色。
- **警告框**：警告时 `role="status"`，错误时 `role="alert"`。圆角 `--radius-lg`，边框是语气色的 30%，内边距 12 × 8，最大宽 77%，右侧可放一个按钮（`action`）。
- **图标**：自绘了面板、"⋯"、下拉箭头三个图标。

**顶栏换成工具栏按钮**
- 侧边栏开关（tooltip 带 ⌘B）、弱认证标记（tooltip 是完整说明）、"打开"（加了下拉箭头）、侧边面板开关（带 ⌥⌘B）、"⋯"。
- "打开"和"⋯"各挂一个空菜单，菜单项留给 T153。

**行类型注册表**（`rows/registry.tsx`）
- `RowRegistry<R extends { kind: string }>`：用 `register` 登记组件，用 `resolve` 查找。
- 同一个 kind 注册两次会直接抛错，因为这是代码错误。
- 不认识的 kind 显示 `UnsupportedRow`，只有一句"此版本不支持显示这项内容"，不渲染行里的任何数据。
- `<RowView registry row>` 按 kind 画出对应的组件。

**文案与测试环境**
- 新增 `stream` 命名空间。
- 弱认证的说明改为"弱认证：会话宿主以降级的本地认证运行"，这样无障碍名称里也有可见的"弱认证"三个字。
- 组件测试用 jsdom 加 Testing Library；Vitest 现在也收 `.test.tsx` 文件。

**新依赖**（版本都锁定，许可证都是 MIT）：`@radix-ui/react-dropdown-menu` 2.2.1、`@radix-ui/react-tooltip` 1.4.1、`@radix-ui/react-switch` 1.4.1；测试用 `jsdom` 30.1.3、`@testing-library/react` 16.3.3、`@testing-library/user-event` 14.6.8、`@testing-library/dom` 10.4.2。

## 关键取舍

- **菜单触发器同时带 tooltip**：两个 Radix 的 Slot 都把属性合并到同一个按钮上。菜单的 `data-state` 会覆盖 tooltip 的，所以"菜单打开"样式匹配 `data-[state=open]` 不会被 tooltip 状态误触发。
- **快捷键是符号**：快捷键文字（⌘B、⌥⌘B）和主进程菜单的加速键一致，不走 i18n。
- **开关滑块颜色**：用 `--color-surface-elevated`，没有为白色滑块另加 token。代价是暗色下滑块偏深，留给 T106 核对暗色时再看。
- **tooltip 阴影**：ui-spec 3.6 有 tooltip 阴影，但没进 5.7.10 的 token，这次没加。
- **重复注册直接抛错**：不做覆盖或忽略，免得两个任务注册了同一个 kind 而不自知。

## 验证方式

- **UNIT-ui-components**（5 条）覆盖：
  - 工具栏按钮的名称、`aria-pressed`、Enter 和 Space 触发、键盘焦点下显示 tooltip（文字加快捷键）；
  - 禁用的按钮不响应点击；
  - 菜单的 `aria-haspopup` 和 `aria-expanded`，键盘打开，Esc 关闭，方向键加 Enter 选中条目，禁用条目选不中，选中后菜单关闭；
  - 开关的 `aria-checked`，Space 切换，禁用后不变；
  - 警告框的两种 role 和框里的按钮。
- **UNIT-ui-row-registry**（2 条）：注册后能查到并画出组件；未知 kind 只显示占位文案，测试里写的原始数据不出现在页面上；重复注册抛错。
- **截图基线**：四张 UI-shell 基线全部有差异，而且差异都只在顶栏右侧一组：
  - 弱认证标记改成按钮的 13 号字；
  - "打开"后面多了下拉箭头；
  - "⋯"换成了自绘图标。

  这些差异是预期的，所以我用 `pnpm ui:baseline` 重新生成并逐张看过。Chromium 和 macOS 版本没变，`meta.json` 内容不变。
- **UI-csp-zero**：在 `tools/ui-screenshots` 的这个用例里加了 6 行，打开一次菜单、显示一次 tooltip，因为 Radix 会经 portal 渲染并插入自己的样式。违规计数仍为 0。
- **审阅截图**在 `docs/tasks/T128-ui-components/screenshots/`：用系统字体的桌面构建，1280 × 820。
  - 有四张：顶栏、键盘焦点下的 tooltip、打开的"⋯"菜单的亮色和暗色两张。
  - 宿主没起来，所以状态点是红色；菜单目前是空的。
  - 开关和警告框还没有界面用到，没有截图。
- **F2**：没有查看本机 Codex App 包，图标都是自绘的。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T128-ui-components/ci/`，全部通过，摘要 `pr-464c7733177b.json` 已提交：

  | 作业 | 结果 |
  |---|---|
  | rust | 1,779 通过 |
  | protocol | 185 通过 |
  | ts | 386 通过 |
  | license | 通过（一遍就过） |
  | testdata-scan | 通过 |
  | ui | 7 通过 |

  e2e 作业还没实现。
- **合并预演**：用 `git merge-tree` 试合并最新的 main，没有冲突。
- **规模**：413 行（实现 272，测试 141；不含 notes、lock、截图），开工前估计 440。

## 提交列表

```
487988b docs(T128): record the ci pr summary
464c773 docs(T128): record the components, the registry and the review screenshots
b623e54 feat(ui-core): build the title bar from the base components
e48eb6b feat(ui-core): add the base components and the row registry
a6c5864 docs(T128): record the checks and the size estimate
```

## 遗留问题 / 范围外发现

- **侧边面板开关的按下状态**：窗口宽度放不下侧边面板、面板被自动收起时，开关仍显示为按下。原因是按钮跟随的是用户意图（`sidePanelOpen`），这是 T084 的取舍。Codex 在这种情况下是否显示为按下没有核实，可以交给 T106 或 T152 一起核对。
- **"打开"和"⋯"的菜单项**：目前菜单是空的，菜单项归 T153。
- **注册表的使用方式**：T082、T083 可以各建一个 `RowRegistry` 实例，或共用一个，在模块加载时注册自己的行组件。不认识的 kind 会自动显示占位，不需要另外处理。