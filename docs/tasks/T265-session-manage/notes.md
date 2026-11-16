# T265 笔记

## 开工前的估算（2026-11-16）

**现状**（main e6f625c5）：
- 会话行是一个整行的 `<button>`，外面套着写 agent 名的 tooltip；菜单按钮不能嵌在按钮里，行要改成"容器 + 行按钮 + 绝对定位的菜单按钮"，和项目行一样。
- 项目行悬停时"+"旁边的菜单是空的；`ContextMenu`、`ContextMenuItem` 已有（文件树在用）。
- 顶栏的"打开"和"⋯"挂着空菜单，顶栏现在不知道当前会话（要从路由取）。
- 没有共用的弹窗。`GitActions` 的提交弹窗用原生 `<dialog>` 加 `showModal()`，jsdom 里由测试补上 `showModal`；共用的 Dialog 照这个做法，不引入新依赖。
- 协议都有：`session/rename`、`session/archive`（`keep | remove | remove_case_only_dirty`，结果 `kept | removed | blocked` 加 `blockers`）、`worktree/check`（`removable`、`blockers`、`warnings.ignored_files`、`warnings.case_only_dirty`）、`project/remove`、`workspace/open`。四个写方法窗口连接都能直接调，不走特权操作。
- 复制工作目录：会话流的 `configured.workspace.path` 有路径。
- 悬停卡片的相对时间用 `SessionSummary.last_activity_at`；i18n 里还没有相对时间的格式化。
- 宿主 `FilesService` 的 `opener` 由 supervisor 固定填 `/usr/bin/open`；只有 e2e 构建的宿主开 `test-harness`（`host_bundle.rs`），开发构建和自用构建都不开。
- 新建会话页现在只能建本地会话（worktree 选项在 T144），CLI 也不能建 worktree 会话：E2E-archive-cleanup 经 harness 连接直接调 `session/create`（`notification.spec.ts` 里已有 `asHarness`，挪进 helpers 共用）。

**按模块估行数**（`cargo xtask size` 口径，产品代码加测试）：

| 模块 | 内容 | 行数 |
|---|---|---|
| M1 侧边栏菜单 | 会话行改结构；悬停时行尾出 20 宽的菜单按钮，右键菜单同样的项（重命名、归档）；项目行菜单"移除项目"：确认弹窗 → `project/remove`，`conflict/turn_running` 时 toast 提示重试；文案；测试 | 130 |
| M2 弹窗与会话操作入口 | 共用 Dialog；会话操作入口 `useSessionActions()`（重命名、归档、打开，T153 的快捷键接这里）；重命名弹窗；归档流程：本地会话直接归档；worktree 会话先 `worktree/check`，能删就问"同时删除 worktree？"（写上会删掉的被忽略文件数，仅大小写冲突时用 `remove_case_only_dirty`），不能删就列出原因、只能保留归档 → `session/archive` → 结果 toast（`removed`、`kept`、`blocked` 加原因）；当前会话归档后回新会话页；文案；测试 | 250 |
| M3 顶栏菜单 | "⋯"：重命名、归档、复制工作目录；"打开"：终端、编辑器、Finder → `workspace/open`，失败时 toast；两个按钮只在会话页显示；测试 | 90 |
| M4 悬停卡片 | 行右侧的卡片：第一行标题与相对时间，第二行文件夹图标加项目名，agent 名并入卡片（原 tooltip 的内容，AG-3）；surface-elevated 底；i18n 加相对时间格式化（`Intl.RelativeTimeFormat`）；测试 | 90 |
| M5 宿主替身 | test-harness 构建里 `opener` 改为数据目录下的替身脚本，发布构建不变；测试 | 25 |
| M6 E2E | `asHarness` 挪进 helpers；E2E-archive-cleanup：harness 连接建两个 worktree 会话，干净的从行菜单归档并删掉 worktree，有改动的列出原因后保留归档；E2E-workspace-open：夹具写替身脚本，三个目标各点一次，核对替身收到的参数 | 130 |
| 合计 | | 约 715 |

比拆分时的 585 多约 130：弹窗和归档流程的分支（本地、能删、仅大小写冲突、不能删、三种结果、当前会话离开）比原估的多；E2E 要自己经 harness 建 worktree 会话。超过停止线 650，未到硬线 780。

## 范围调整与实际规模

编排者 2026-11-16：停止线先提到 750、硬线 820（照 715 做）；M2 做完实际 +598，推算全任务超过硬线，按建议拆分：T265 只留 M2 + M1 + M3，停止线 800、硬线 860；悬停卡片、`opener` 替身和两个 E2E 拆成 T267。`archive`、`rename`、`open_in_terminal` 三个菜单命令的处理归本任务（T153 只注册加速键、照旧发命令）。

| 模块 | 估算 | 实际（`cargo xtask size`，新增行） |
|---|---|---|
| M2 弹窗与会话操作入口 | 250 | 598；测试挂载并进 `review/test-harness` 后累计 581 |
| M1 侧边栏菜单 | 130 | 166（累计 747） |
| M3 顶栏菜单 | 90 | 104（累计 851；其中约 20 行是原有两个菜单包进 `session &&` 后的缩进） |

M2 超出的原因：产品代码 356 行（原估约 170）：provider 连同命令接线、三种结果和错误的文案、两种归档弹窗、共用 Dialog；测试 242 行（原估约 80），其中约 70 行是 mock host 的挂载，后来并进了已有的 test-harness。
