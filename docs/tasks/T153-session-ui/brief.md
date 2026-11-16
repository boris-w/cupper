# T153 快捷键与菜单

- 派发日期：2026-11-16
- 执行者：子 agent（前端，ui-core；T106、T152 的执行者）
- 分支：`feat/T153-session-ui`
- 依赖：T125、T240、T241、T126、T152（都已合并）。T148 依赖本任务
- **范围调整**（2026-11-16）：开工前估约 1,010 行，会话管理界面（会话行和项目行菜单、弹窗、顶栏"⋯"和"打开"、悬停卡片、`workspace/open` 的替身）拆到 T265，本任务只留快捷键与菜单

## 范围

**做**（计划 T153 一行；设计 5.7.7 快捷键、UI-4）：
1. **应用菜单**：先按 F2 静态读 ChatGPT.app 的菜单模板定分组；文案走 `@cupper/i18n` 的 `menu` 命名空间（中文，角色项也配中文；英文由 T154 补）。
2. **快捷键**：5.7.7 按 Codex 默认值的快捷键子集与 cupper 的别名（⌘.、⌘⌥T），菜单加速键经 `onCommand` 接到 ui-core。还没人处理的命令：`interrupt`、`next_needs_action`（⌘⌥A）、`prev/next_sidebar_item`、`prev/next_recent`（加最近会话的记录）、`session_n`、`archive`、`rename`、`model_picker`、`open_in_terminal`。其中 `archive`、`rename`、`open_in_terminal` 需要的弹窗和"打开"由 T265 做：本任务把命令接到 T265 将提供的入口；如果 T265 还没合并，这三个命令先只做到发出意图，与 T265 约定接口，在汇报里写清。
3. **PERF-ui-switch** 补"下一个需要处理的会话"一项（T155 留的位置）。
4. 用例：E2E-shortcuts、E2E-next-needs-action（已登记）。

**不做**：会话管理界面（T265）；英文（T154）；复刻度的整体修正（T148）。

## 交接事项（逐条带入）

- 菜单顶层标题和角色项（撤销、复制、退出等）是 Electron 自带的英文，需要给 `@cupper/i18n` 加 `menu` 命名空间；菜单分组是 T074 暂定的，按 Codex 定稿（T074）
- 菜单里有 `next_needs_action`（Command+Alt+A），ui-core 的命令处理里没有它；T103 的场景 7 因此测不了"下一个需要处理的会话"（T103）
- PERF-ui-switch 还缺"下一个需要处理的会话"一项，T153 接上 `next_needs_action` 后在 PERF-ui-switch 里补上（T155）

## 验收标准

- [ ] 两个 E2E 用例在 runner 上通过（e2e-repeat 各 10 次）；PERF-ui-switch 的新项在分支上的 nightly 里有记录。
- [ ] 截图基线如有变化只由 ui-baseline 生成并逐张说明原因。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 停止线：650；硬停止线：750（2026-11-16 写到 471 行时停下汇报后调整；原为 500、600）。开工前的估算约 425（M1 + M2 + M9 加两个 E2E）；**每完成一个模块就跑一次 `cargo xtask size`**，推算越线就停下汇报，按模块分提交。

## 参考资料

- 设计：5.7.7（快捷键表）、5.7.4。
- 卷宗：T074（菜单）、T155（PERF-ui-switch）、T106（`check.md`）。

## 注意

- e2e 在 runner 上跑（守则第 10 条）；本机只跑单元测试和单份短冒烟，PERF 不在本机跑。
- ChatGPT.app 只静态读包（F2），解出的文件放临时目录、结束时删除。
- 要推送、触发 e2e-repeat、nightly 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T043`，分支 `feat/T153-session-ui`（从 main 开出）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
