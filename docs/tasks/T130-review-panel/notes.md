# T130 工作笔记

- 执行者：子 agent（前端工程师，ui-core 与桌面；T160、T225、T173 的执行者接手）

## 开工前核对到的现状

- 侧边面板（T084）已有标签条和空的 `tabpanel`；主进程菜单已注册 `open_review_tab`（⌃⇧G）、`toggle_file_tree`（⌘⇧E），ShellAPI 的命令联合里也有这两个（M1a 冻结）。
- 输入框已有附件条，`review_comment` 每条一个标签；设计要的是合成"{n} 条评论"、展开列出文件、行号、正文，逐条删除，要改。
- `Menu`、`ToolbarButton`、`Tooltip`、`Switch`、toaster 已有；TanStack Query 已接好，`turn.completed` 让会话的查询失效。
- 协议（冻结）：`git/diff {session_id, scope}` → `{caveats, diff_token, files, snapshot?, stat}`；`git/stage`、`git/unstage`、`git/revert` 共用 `GitApplyParams {command_id, diff_token, session_id, target, input?}`；`git/commit {message}`、`git/push {set_upstream?}`；`git/status` 给分支、ahead/behind。
- T123 会给 `git/status` 加 `repo_config_changed?: [paths]`、给 `ErrorReason` 加 `signing_failed`；合并前在 mock 里按这个形状做。
- mock 宿主（`@cupper/host-client/mock`）目前没有 git 方法，截图回归和单测要补。

## 规模估计（停止线 1,100，硬停止线 1,300）

行数指 Prettier 展开后的行，含注释和空行。文案（`packages/i18n` 的 JSON）计入。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| 数据与状态 | 审查面板的界面状态（范围、文件树开合与筛选、换行、diff 视图、展开集合、选中文件）、每个会话的评论草稿；`git/diff` 按范围查询并持有 `diff_token`，`git/status` 取分支和 `repo_config_changed` | 110 |
| 头部、提示与空状态 | "查阅来源"下拉、增删统计、文件树开关、选项菜单（换行、diff 视图、展开收起全部、刷新）；`caveats`、`repo_config_changed` 的醒目提示、快照排除的说明；空状态 | 150 |
| 文件树 | 由路径建树、目录吸顶、中间截断、Git 状态图标、"筛选文件…"、⌘⇧E、右键菜单（复制路径、在外部应用中打开）；文件行上的操作按钮 | 200 |
| 操作执行与整组药丸 | 暂存、取消暂存、还原的执行（`command_id`、`diff_token`、target；`stale_diff` 时刷新并提示；结果提示）；底部药丸按范围变化 | 150 |
| 还原确认（RP-11） | 500 ms 待命、按触发来源决定焦点、同一时间只显示一个且忽略新的、只认就绪后的按键和点击；确认后才发 `git/revert` | 110 |
| Git 操作菜单与提交弹窗 | 菜单、弹窗（分支、提交信息、"包含未暂存的更改"即先 `git/stage` 再 `git/commit`、三选一、hooks 与 `repo_config_changed` 提示）；错误：`hook_failed`、`nothing_to_commit`、`signing_failed`（pinentry 指引）、`auth_failed`、`rejected`；无上游时推送带 `set_upstream` | 230 |
| 评论草稿与附件条 | 草稿进附件条，合成"{n} 条评论"并可展开、逐条删除；给 T131 的添加入口 | 70 |
| diff 视图接口 | 给 T131 的组件接口（下发 `diff_token`、文件、草稿、展开状态；收 hunk 操作、评论草稿、`need_file`），T131 之前用占位 | 50 |
| 接线、图标、文案 | 侧边面板里挂上审查面板、两个菜单命令；Git 状态与操作图标；中英文案约 70 个键 | 200 |
| **实现小计** | | **1,270** |

**测试**

| 项 | 内容 | 草估 |
|---|---|---|
| UNIT-review-actions | 三种范围下的整组操作、`stale_diff`、还原确认的焦点与待命、一次只一个、"包含未暂存"先暂存后提交、`signing_failed` 指引、草稿进附件条 | 220 |
| UI-review | 文件树开合、整组操作在三种范围下、空状态、提交弹窗 | 110 |
| mock 宿主的 git 方法与夹具 | `git/diff`、`file_diff`、`status`、`stage`、`unstage`、`revert`、`commit`、`push` | 70 |
| 复刻度审阅截图 | e2e 构建、系统字体，不进基线 | 40 |
| **测试小计** | | **440** |

**合计约 1,710**，按前几个 L 任务的偏差（多 10–30%）可能到 1,900–2,200。远超 1,100，需要拆分。

## 拆分方案（报编排者）

| 任务 | 内容 | 估计 |
|---|---|---|
| T130（关键链，T131、T132 等它） | 数据与状态、头部与提示与空状态、操作执行与整组药丸、还原确认（RP-11）、评论草稿与附件条、diff 视图接口、接线与相应文案；UNIT-review-actions 的这部分、UI-review 的"整组操作三种范围、空状态" | 约 1,080（可能到 1,250） |
| T130b 文件树 | 文件树全部、文件行上的操作按钮；UI-review 的"文件树开合" | 约 350 |
| T130c Git 操作与提交弹窗 | 菜单与弹窗、`repo_config_changed` 的确认文案、`signing_failed` 指引；依赖 T123（正好等它合并） | 约 450 |

编号建议：UI-review 拆成子编号（`-panel`、`-tree`、`-commit`）；UNIT-review-actions 留给 T130，T130c 另起 UNIT-review-commit。
