# T153 笔记

## 开工前的估算（2026-11-16）

**现状**（main e5bb2fc3）：
- 菜单（`apps/desktop/src/main/menu.ts`）已注册 5.7.7 的全部加速键，但 ui-core 只处理了 `new_session`、`settings`、`toggle_*`、`open_review_tab`。`interrupt`、`next_needs_action`、`prev/next_sidebar_item`、`prev/next_recent`、`session_n`、`archive`、`rename`、`model_picker`、`open_in_terminal` 都没人接。
- 菜单的角色项（撤销、复制、退出等）和顶层标题是 Electron 的英文；分组是 T074 暂定的。
- 侧边栏：项目行悬停时有"+"和一个空菜单；会话行没有菜单按钮、右键菜单和悬停卡片。会话行现在的 tooltip 写 agent 名。
- 顶栏的"打开"和"⋯"挂的是空菜单。
- 协议已经有 `session/rename`、`session/archive`（`worktree: remove | keep`）、`worktree/check`（`removable`、`blockers`、`warnings`）、`project/remove`、`workspace/open`（terminal / editor / finder，要 `session_id`）。
- `SessionSummary.last_activity_at` 可以做悬停卡片的相对时间，不用改宿主。
- 宿主的 `workspace/open` 固定调 `/usr/bin/open`，e2e 里没有替身。E2E-workspace-open 要求"mock 打开动作"，需要宿主在 test-harness 构建里换成替身（只改 test-harness 构建）。
- 项目行菜单里的"在 Finder 中显示"没有对应的宿主方法：`workspace/open` 只认会话。

**按模块估行数**（`cargo xtask size` 口径，产品代码加测试）：

| 模块 | 内容 | 行数 |
|---|---|---|
| M1 菜单 | 静态读 ChatGPT.app 的菜单模板定分组（F2，不计行数）；`menu` 命名空间（顶层标题、角色项）；menu.ts 按 Codex 分组、角色项配上文案；测试 | 130 |
| M2 快捷键 | ui-core 接上 9 个命令：⌘. 中断、⌘⌥A 下一个需要处理的会话、侧边栏上下一个、最近会话来回、⌃1–9、⌘⌥R 重命名、⌘⇧A 归档、⌃⇧M 模型菜单、⌘⌥T 在终端打开；最近会话的记录；测试 | 170 |
| M3 侧边栏菜单 | 会话行悬停菜单按钮（20 宽）和右键菜单（重命名、归档）；项目行菜单（移除项目，遇到 `conflict/turn_running` 提示重试）；测试 | 110 |
| M4 弹窗 | 共用的 Dialog；重命名弹窗；归档流程（`worktree/check` → "同时删除 worktree？"或列出不能删的原因 → `session/archive` → 结果提示，当前会话归档后离开）；测试 | 190 |
| M5 顶栏菜单 | "⋯"（重命名、归档、复制工作目录）；"打开"（终端、编辑器、Finder → `workspace/open`）；测试 | 90 |
| M6 悬停卡片 | 会话行右侧的卡片：标题和相对时间、项目、agent，surface-elevated 底；测试 | 90 |
| M7 宿主替身 | test-harness 构建里 `workspace/open` 用数据目录下的替身脚本，不真的打开 | 25 |
| M8 E2E | E2E-archive-cleanup、E2E-next-needs-action、E2E-shortcuts、E2E-workspace-open | 180 |
| M9 PERF | PERF-ui-switch 加"下一个需要处理的会话" | 25 |
| 合计 | | 约 1,010 |

合计远超停止线 650（硬线 780），建议拆成两个任务：
- **T153a 快捷键与菜单**：M1 + M2 + M9 + E2E-shortcuts、E2E-next-needs-action，约 425。
- **T153b 会话管理界面**：M3 + M4 + M5 + M6 + M7 + E2E-archive-cleanup、E2E-workspace-open，约 585。

编排者 2026-11-16 决定：会话管理界面（M3–M7 和 E2E-archive-cleanup、E2E-workspace-open）拆成 T265，派给另一位执行者。本任务改名"快捷键与菜单"，只留 M1 + M2 + M9 和 E2E-shortcuts、E2E-next-needs-action；停止线 500，硬线 600。`workspace/open` 的替身和项目行菜单都归 T265，"在 Finder 中显示"这次不做。`archive`、`rename`、`open_in_terminal` 三个命令接到 T265 将提供的入口：本任务先只发出意图，接口见下。

编排者随后更正：不放空的占位文件。`archive`、`rename`、`open_in_terminal` 三个快捷键的处理由 T265 自己接（它拥有 `packages/ui-core/src/session/actions.tsx` 的 `useSessionActions`），本任务不接这三个，E2E-shortcuts 也不测它们。菜单里这三个加速键照旧注册、照旧发命令，T265 在 ui-core 里加处理即可。

## M1 菜单：Codex 的分组（F2，静态读包）

只读了 ChatGPT.app 包里主进程构建产物的菜单模板和 `native-menu-locales/zh-CN.json`，没有启动 App；临时目录读完即删，仓库里没有拷入任何文件。结论：

- 顶层：App 菜单、文件、编辑、视图、窗口、帮助。cupper 没有帮助菜单的内容（文档、新功能、故障排除、系统状态都是 OpenAI 的链接），不加。
- App 菜单：关于、分隔、设置、（检查更新、退出登录）、分隔、服务、分隔、隐藏、隐藏其他、全部显示、分隔、退出。cupper 没有服务菜单（5.7.8），也没有登录。
- 文件：新聊天、新建临时聊天、新建项目、新建独立聊天（隐藏）、分隔、打开文件夹、分隔、关闭。cupper 对应：新会话（⌘N，⌘⇧O 隐藏）、分隔、中断回合（⌘.）、在终端中打开（⌘⌥T）、分隔、关闭。后两项是 cupper 的别名（5.7.7），放文件菜单。
- 编辑：Electron 的标准项，Codex 给每个角色项都配了自己的文案（撤销、重做、剪切、复制、粘贴、粘贴并匹配样式、删除、全选、替换子菜单、语音子菜单）。cupper 照此给全部角色项配上 `menu` 命名空间的文案。
- 视图：侧边栏、底部面板、终端、文件树等开关；查找；返回、前进；上一个 / 下一个侧边栏项目（第二组按键是隐藏项）；标签切换；开发者工具；缩放（放大、缩小、实际大小）；全屏。重命名、归档、置顶、⌃1–9、命令菜单、复制路径这些都是隐藏项（`visible: false`，靠加速键触发），不在菜单上显示。cupper 照此把"下一个需要处理的会话"、上一个 / 下一个侧边栏项目、最近会话放进视图菜单，重命名、归档、模型菜单、⌃1–9 只做隐藏项。
- 窗口：最小化、缩放、分隔、全部置于顶层。
- 中文用词取自 Codex 的 zh-CN 菜单文案的同义项（"全部置于顶层""切换全屏""实际大小"等），但只用通用词，命令名仍用 `command` 命名空间里 cupper 自己的词（"会话"不是"聊天"）。

## M2 快捷键的行为

- **侧边栏的上一个 / 下一个**（⌘⌥[ / ⌘⌥]）：照 Codex（它在侧边栏的 DOM 里找 `aria-current` 的那一行，±1，点下去）：只走显示出来的行（"展开显示"后面的不算），当前会话不在显示的行里时，下一个取第一行、上一个取最后一行；到两头就停，不绕回。cupper 按数据算，所以侧边栏收起时也能用（Codex 收起时什么都不做）。为此把"展开显示"的状态从 `ProjectGroup` 的局部 state 移到 UI store 的 `expanded`。
- **⌃1–9**：显示出来的第 n 行。
- **下一个需要处理的会话**（⌘⌥A）：按侧边栏的顺序，从当前会话往后找第一个 `needs_action`，到尾再从头找，不回到当前会话自己；"展开显示"后面的也算，找到时展开它所在的项目，让它的行显示出来。
- **最近的会话**（⌃Tab / ⌃⇧Tab）：Codex 的"切换聊天…"是按住修饰键连按、松开才算数。cupper 没有那个浮层，但保留同样的走法：UI store 记录访问过的会话（`recent`，当前的在前，不保存）；按住 ⌃ 连按时按开始那一刻的顺序往后（⌃⇧Tab 往前）走，绕回；⌃ 抬起、点击或窗口失焦时走完，停在哪个就记哪个为最近访问。所以单按一下 ⌃Tab 是在最近两个会话之间来回。已经不在索引里的会话跳过。
- **中断**（⌘.）：当前会话 `running` 或 `needs_action` 时，和停止按钮一样调 `session/interrupt`。
- **模型菜单**（⌃⇧M）：会话页上打开（再按关闭）输入框的模型菜单，走已有的 `openMenu("model")`。
- 归档、重命名、在终端中打开：归 T265（见上）。

## 规模

| 模块 | 估算 | 实际（`cargo xtask size`，新增行） |
|---|---|---|
| M1 菜单 | 130 | 156（menu.ts 120 行里约一半是改写；`menu` 命名空间 37 行） |
| M2 快捷键 | 170 | 315：产品代码 200（shortcuts.tsx 73、order.ts 70、ui.ts 46、其余 11），测试 115 |
| 小计 | 300 | 471，离停止线 500 只剩 29 |
| M9 + 两个 E2E | 125 | 推算 125–135 |
| 合计推算 | 425 | 约 600，到硬线 |

M2 超出的主要原因：⌃Tab 照 Codex 做成"按住连按"的走法（UI store 约 25 行、按键监听 15 行、测试约 50 行），估算时按最简单的"最近两个来回"算的；另外侧边栏顺序抽成纯函数并单测（order.ts 和测试共 133 行），估算时没有算单测。
