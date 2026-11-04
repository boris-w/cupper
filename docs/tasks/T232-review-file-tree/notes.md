# T232 工作笔记

- 执行者：子 agent（前端工程师，ui-core 与桌面；T130 的执行者接手）

## 开工前核对到的现状

- 审查面板（T130）已有共享状态 `review/state.ts`（`focus` 还没有生产者）、范围白名单 `ACTIONS`、`act`（按范围过滤，还原经确认控件）、占位视图 `FileList`。
- ⌘⇧E 的菜单命令 `toggle_file_tree` 已注册，在冻结的 ShellAPI 命令联合里；ui-core 还没有处理它。
- 文件树开合按 5.7.6 存 prefs：加进 `LayoutPrefs`（`layout.fileTreeOpen`），与侧边面板开合同一套保存与回声处理。
- 复制路径：渲染进程已有 `navigator.clipboard.writeText` 的用法，主进程只放行 `clipboard-sanitized-write`。
- 在外部应用中打开：T126 的 `workspace/open {session_id, target}` 只能打开工作区，协议里没有路径参数（UI-0 本来就是"打开当前会话的工作区"）。右键菜单照此打开工作区，在汇报里提出。
- 右键菜单：ui-core 只有 Radix 的 dropdown-menu。加 `@radix-ui/react-context-menu@2.4.1`，它依赖的 `@radix-ui/react-menu@2.2.1` 与已装的 dropdown-menu 共用，只多这一个包（MIT）。
- tooltip：Radix DropdownMenu 关闭时在 `onCloseAutoFocus` 里把焦点还给触发按钮，`ToolbarButton` 的 tooltip 随焦点打开。由指针关闭时在这里 `preventDefault`，焦点不还给触发按钮；键盘关闭照旧。

## 规模估计（停止线 400，硬停止线 480）

| 模块 | 内容 | 草估 |
|---|---|---|
| 文件树 | 由路径建树、单子目录压缩、目录吸顶、中间截断、状态图标、筛选、选中与点击设 `focus` | 120 |
| 文件行操作 | 按 `ACTIONS[scope]` 的图标按钮与 tooltip，经 `act` 执行，还原走确认控件 | 25 |
| 右键菜单 | ContextMenu 的薄封装；复制路径、在外部应用中打开 | 35 |
| 开关与偏好 | `fileTreeOpen` 偏好、头部"显示文件/隐藏文件"、⌘⇧E | 25 |
| 图标与文案 | 三种状态图标、暂存/取消暂存/还原图标；约 12 个文案键 | 37 |
| tooltip 修复 | `Menu` 记下最后的输入方式，指针关闭时不还焦点 | 15 |
| 占位视图滚动 | `FileList` 在 `focus` 变化时滚到对应文件 | 10 |
| **实现小计** | | **267** |
| UI-review-tree | 截图用例：开合、筛选、状态图标、亮暗、右键菜单、行操作；去掉 T130 用例里的绕法 | 90 |
| 单测 | UNIT-ui-components 补一条：指针关闭不还焦点、键盘关闭还焦点 | 15 |
| **测试小计** | | **105** |

合计约 372，离停止线 400 只余 28。目录折叠不在任务书里，不做；每个模块做完就跑一次 `cargo xtask size`，推算越线就停下汇报。

## 实际规模（2026-11-04 编排者同意做到约 425，硬停止线 480 不变）

`cargo xtask size`：+454 −38（净 +416）。

| 部分 | 估 | 实际 |
|---|---|---|
| FileTree.tsx（建树、吸顶、截断、图标、筛选、行操作、右键菜单） | 180 | 189 |
| ReviewPanel.tsx（开关、`act` 提到顶层给树和视图共用、占位视图滚动） | 25 | +65 −23（挪动 `act` 重复计了约 13 行） |
| Menu.tsx（tooltip 修复、ContextMenu、共用的 class） | 35 | +37 −3 |
| 图标 | 25 | 29 |
| 偏好与命令 | 25 | 14 |
| 文案 | 12 | 10 |
| UI-review-tree 截图用例 | 90 | +110 −10 |
| UNIT-ui-components 补的断言 | 15 | 7 |

超出估计的主要是截图用例（行操作两种范围、右键菜单的两项都要覆盖）和 ReviewPanel 的挪动。

## 实现中的取舍

- 文件树放在面板左侧一栏（宽 240，自己滚动），与 diff 视图并排；面板窄时仍是这个宽度，T131 接入后再看是否要随面板宽度收起。
- 目录不折叠：任务书没有要求，省下行数。
- 名字的中间截断：头部 `truncate`、尾部（最多 10 个字符）不缩；屏幕阅读器读 `sr-only` 的完整名字，否则两段会被读成"appl y.rs"。
- 筛选框用 `type="text"`：`search` 在 Chromium 里有一个原生的蓝色清除按钮，与界面不符。
- 截图用例里的复制路径改写页面里的 `navigator.clipboard`，不碰本机的系统剪贴板。
