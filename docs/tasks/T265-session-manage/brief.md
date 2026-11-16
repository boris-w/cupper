# T265 会话管理界面：菜单、弹窗与悬停卡片

- 派发日期：2026-11-16
- 执行者：子 agent（宿主与桌面；T150、T250、T251、T254、T258 的执行者）
- 分支：`feat/T265-session-manage`
- 依赖：T125、T240、T241、T126、T152（都已合并）。由 T153 拆出（开工前估算见 `docs/tasks/T153-session-ui/notes.md`）。T148 依赖本任务

## 范围

**做**（计划 T265 一行；设计 5.7.4 侧边栏和顶栏、5.5.9、PS-4、UI-0）：
1. **侧边栏菜单**：会话行的悬停菜单和右键菜单（重命名、归档）；项目行菜单（移除项目；遇到 `conflict/turn_running` 提示重试）。项目行的"在 Finder 中显示"没有宿主方法，这次不做（记进汇报）。
2. **弹窗**：共用的 Dialog；重命名弹窗；归档流程：`worktree/check` → 询问或列出原因 → `session/archive`（`worktree: remove | keep`）→ 结果提示。
3. **顶栏**："⋯"会话菜单（重命名、归档、复制工作目录）；"打开"（终端、编辑器、Finder，经 `workspace/open`）。
4. **会话行的悬停卡片**：照 Codex（T106 核对：悬停时行尾显示固定和归档按钮，行右侧弹出卡片，标题与相对时间、项目，底色 surface-elevated），相对时间用 `SessionSummary.last_activity_at`。
5. **`workspace/open` 的替身**：只在 test-harness 构建里，`workspace/open` 改为执行数据目录下的替身脚本（e2e 夹具写入，记下参数），发布构建不变；作为 5.9.9 的一个注入点，名字和形态在汇报里写清，编排者补进设计。
6. 快捷键 `archive`、`rename`、`open_in_terminal` 由 T153 接到这里提供的入口，接口和 T153 的执行者约定（经编排者转达）。
7. 用例：E2E-archive-cleanup、E2E-workspace-open（已登记在 `[T265]`）。

**不做**：快捷键与应用菜单（T153）；新建会话页（T144）；复刻度的整体修正（T148）。

## 交接事项（逐条带入）

- 顶栏的"打开"和"⋯"目前是占位按钮（T128 换成工具栏按钮并能挂菜单），菜单内容归本任务（T084）
- 顶栏"打开"和"⋯"已是挂着空菜单的工具栏按钮（`<Menu trigger={<ToolbarButton …/>}>`），菜单项归本任务（T128）
- 侧边栏项目行悬停时的"+"旁有一个空菜单、会话行还没有菜单按钮和右键菜单，菜单项归本任务（`ui-core/src/sidebar/`）（T081）
- 错误条目的恢复指引：`change_model` 的入口在 T141（模型选择器），`upgrade_cli`、`install_cli`、`restart_agent` 在 T151（设置页 agent 状态、`agents/restart`），`run_login_in_terminal` 的"打开终端"在 T153；T143 现在只给文字和可复制的命令（T143（已写入 T151 的 brief））
- 默认编辑器是 TextEdit（T036 定的），`open -a TextEdit <目录>` 打不开文件夹；改成清单里第一个已安装的编辑器，或由界面引导用户选一个（T126（已写入 T151 的 brief））
- `project/remove` 先问遍所有会话再逐个归档，问完到归档之间仍有很小的窗口：某个会话恰好开始回合时，排在它前面的会话可能已经归档。界面收到 `conflict/turn_running` 时提示重试；彻底消除要让会话在被问时进入"不接新回合"的状态（T241）
- worktree 会话恢复时复用新建的 `add`，actor 要等 1 秒 refresh；恢复入口要显示进行中的状态（T241）
- Codex 的会话行悬停时，行尾显示固定和归档按钮，并在行右侧弹出悬停卡片：标题与相对时间、项目（底色 surface-elevated）；cupper 是否做由 T153 定，做的话相对时间要宿主在 `SessionSummary` 里已有的时间戳（T106）

## 验收标准

- [ ] 两个 E2E 用例在 runner 上通过（e2e-repeat 各 10 次）；替身只在 test-harness 构建里存在（发布构建的检查照旧通过）。
- [ ] 截图基线只由 ui-baseline 生成并逐张说明原因（取值在阈值以内的改动用 `all=true`）。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 停止线：650；硬停止线：780。拆分时估约 585；开工前自己再按模块估一次报给 `main`，等回复再写代码；**每完成一个模块就跑一次 `cargo xtask size`**，推算越线就停下汇报，按模块分提交。

## 参考资料

- 设计：5.7.4（侧边栏、顶栏）、5.5.9、5.2.10（worktree 清理）、5.9.9（注入点）。
- 卷宗：T081、T084、T106（`check.md`）、T125、T126、T128、T240、T241；T153 的 notes（拆分前的估算和现状）。

## 注意

- e2e 在 runner 上跑（守则第 10 条）；本机只跑单元测试和单份短冒烟。测试里不要真的拉起本机的终端、编辑器或 Finder。
- 宿主的约束照旧：子进程只经 `cupper-sys::spawn`；替身在编译期门控，只在 test-harness 构建里。
- 要推送、触发 e2e-repeat、ui-baseline 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T150`，编排者已切到新分支 `feat/T265-session-manage`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
