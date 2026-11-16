# T153 会话管理界面、快捷键与"打开"按钮

- 派发日期：2026-11-16
- 执行者：子 agent（前端，ui-core；T106、T152 的执行者）
- 分支：`feat/T153-session-ui`
- 依赖：T125、T240、T241、T126、T152（都已合并）。T148 依赖本任务

## 范围

**做**（计划 T153 一行；设计 5.7.7 快捷键、5.5.9、PS-4、UI-0、UI-4）：
1. 会话行的悬停菜单和右键菜单、顶栏"⋯"会话菜单：重命名弹窗、归档与 worktree 清理提示、复制工作目录。
2. **会话行的悬停卡片**（T106 核对：Codex 悬停时行尾显示固定和归档按钮，并弹出卡片，标题与相对时间、项目，底色 surface-elevated）：照 Codex 做；相对时间用 `SessionSummary` 里已有的时间戳，没有的话先报给 `main`。
3. 5.7.7 按 Codex 默认值的快捷键子集与 cupper 的别名（⌘.、⌘⌥T），菜单加速键经 `onCommand`，含 ⌘⌥A 跳到下一个需要处理的会话（`next_needs_action`）。
4. 顶栏的"打开"（UI-0：在终端或编辑器里打开工作区）。
5. 菜单分组按 Codex 定稿，菜单文案走 `@cupper/i18n` 的 `menu` 命名空间（中文；英文由 T154 补）。
6. PERF-ui-switch 补上"下一个需要处理的会话"一项（T155 留的位置）。
7. 用例：E2E-archive-cleanup、E2E-next-needs-action、E2E-shortcuts、E2E-workspace-open（已登记）。

**不做**：新建会话页（T144）；复刻度的整体修正（T148）；英文（T154）。

## 交接事项（逐条带入）

- 菜单顶层标题和角色项（撤销、复制、退出等）是 Electron 自带的英文，需要给 `@cupper/i18n` 加 `menu` 命名空间；菜单分组是 T074 暂定的，按 Codex 定稿（T074）
- 顶栏的"打开"和"⋯"目前是占位按钮（T128 换成工具栏按钮并能挂菜单），菜单内容归本任务（T084）
- 顶栏"打开"和"⋯"已是挂着空菜单的工具栏按钮（`<Menu trigger={<ToolbarButton …/>}>`），菜单项归本任务（T128）
- 侧边栏项目行悬停时的"+"旁有一个空菜单、会话行还没有菜单按钮和右键菜单，菜单项归本任务（`ui-core/src/sidebar/`）（T081）
- 错误条目的恢复指引：`change_model` 的入口在 T141（模型选择器），`upgrade_cli`、`install_cli`、`restart_agent` 在 T151（设置页 agent 状态、`agents/restart`），`run_login_in_terminal` 的"打开终端"在 T153；T143 现在只给文字和可复制的命令（T143（已写入 T151 的 brief））
- 默认编辑器是 TextEdit（T036 定的），`open -a TextEdit <目录>` 打不开文件夹；改成清单里第一个已安装的编辑器，或由界面引导用户选一个（T126（已写入 T151 的 brief））
- `project/remove` 先问遍所有会话再逐个归档，问完到归档之间仍有很小的窗口：某个会话恰好开始回合时，排在它前面的会话可能已经归档。界面收到 `conflict/turn_running` 时提示重试；彻底消除要让会话在被问时进入"不接新回合"的状态（T241）
- worktree 会话恢复时复用新建的 `add`，actor 要等 1 秒 refresh；恢复入口要显示进行中的状态（T241）
- 菜单里有 `next_needs_action`（Command+Alt+A），ui-core 的命令处理里没有它；T103 的场景 7 因此测不了"下一个需要处理的会话"（T103）
- PERF-ui-switch 还缺"下一个需要处理的会话"一项，T153 接上 `next_needs_action` 后在 PERF-ui-switch 里补上（T155）
- Codex 的会话行悬停时，行尾显示固定和归档按钮，并在行右侧弹出悬停卡片：标题与相对时间、项目（底色 surface-elevated）；cupper 是否做由 T153 定，做的话相对时间要宿主在 `SessionSummary` 里已有的时间戳（T106）

## 验收标准

- [ ] 四个 E2E 用例在 runner 上通过（e2e-repeat 各 10 次）；PERF-ui-switch 的新项在分支上的 nightly 里有记录。
- [ ] 截图基线只由 ui-baseline 生成并逐张说明原因（取值在阈值以内的改动用 `all=true`）。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 停止线：650；硬停止线：780。开工前在 `notes.md` 里按模块估行数报给 `main`，等回复再写代码；**每完成一个模块就跑一次 `cargo xtask size`**，推算越线就停下汇报，按模块分提交。

## 参考资料

- 设计：5.7.4（侧边栏、顶栏）、5.7.7（快捷键表）、5.5.9、5.2.10（worktree 清理）。
- 卷宗：T074、T081、T084、T106（`check.md`）、T125、T126、T128、T240、T241、T155。

## 注意

- e2e 在 runner 上跑（守则第 10 条）；本机只跑单元测试和单份短冒烟，PERF 不在本机跑。
- 不运行真实的 Claude Code、Codex CLI。"打开"在测试里不要真的拉起本机的终端或编辑器。
- 要推送、触发 e2e-repeat、ui-baseline、nightly 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T043`，编排者已切到新分支 `feat/T153-session-ui`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
