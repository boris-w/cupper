# T125 notes

## 开工前的规模估计（2026-11-04）

停止线 700，硬停止线 850。估计已经把 rustfmt 的展开算进去。

| # | 模块 | 内容 | 行 |
|---|---|---|---|
| 1 | `worktree/check` | 见下文"worktree/check 的明细" | 135 |
| 2 | `session/archive` | 见下文"session/archive 的明细" | 110 |
| 3 | `session/restore` | 见下文"session/restore 的明细" | 90 |
| 4 | `session/rename`、`session/list_archived` | rename：行里写 title 和 `title_source = user`，给索引发 `Note::Title`（T100 的交接）。list_archived：游标 `{archived_at, session_id}`，每页 50，摘要取自行 | 75 |
| 5 | `project/remove` | 项目下任何一个会话有运行中的回合时，先回 `conflict`，不改任何东西；否则逐个归档（保留 worktree），再由索引写 `projects.removed_at` 和 `project.removed` | 50 |
| 6 | 索引 | `Note::Title`；`Note::Archived` 让状态核心删掉这一项并写 `session.removed{archived}`；恢复时重新 `created` | 30 |
| 7 | 接线 | 路由、dispatch 用例里的 Later 行、错误声明 | 15 |
| 8 | 单元用例 | 大小写冲突的判断、带锁原因的 `listed` | 20 |
| 9 | IT-worktree-check-rules | 表驱动：每行一段 git 命令，构造一种情形，写明期望的 blockers 和 warnings；7 条规则加 2 种只提示的情形 | 130 |
| 10 | IT-session-archive-restore | 见下文"IT-session-archive-restore 的明细" | 150 |
| | 合计 | | 约 805 |

### worktree/check 的明细

- 第 1、2 条：用快照档的 `status::files` 判断修改、暂存和未跟踪。脏文件全都只是大小写冲突的，不算第 1 条，记为 `case_only_dirty`。大小写冲突是指：已跟踪的路径里，有两条只差大小写。
- 第 3 条：复用 `status::in_progress(git_dir)`。
- 第 4 条：扩展 `listed`，返回锁的原因；原因不是 `cupper session <id>` 的锁算外来锁。
- 第 5 条：`rev-list --count HEAD --not --exclude=<本分支> --branches --remotes --tags`。
- 第 6 条：仓库有远程时，`rev-list --count HEAD --not --remotes`；没有远程时跳过。
- 第 7 条：git 目录下的 `modules/` 不为空。
- 被忽略文件的数量：快照档新增 `ls-files -z --others --ignored --exclude-standard`。
- 经 `GitService` 提供。

### session/archive 的明细

- 由会话 actor 处理。
- 有运行中的回合，或者还在准备工作区时，回 `conflict/turn_running`。
- 先 Detach，关闭 agent 会话。
- `remove`：
  - 按第 1 项检查，有 blocker 时结果为 `blocked`。
  - 否则在 `RepoLock` 内写 `pending_external`，然后 `worktree unlock`、`worktree remove`。只在 `remove_case_only_dirty`、而且脏文件只是大小写冲突时才加 `--force`。
  - 保留分支。
- `archived_at`、账本，以及给索引的 `Note::Archived`，在同一次提交里。

### session/restore 的明细

- 本地会话：取消归档，回 `full`。
- worktree 会话：
  - 已登记且目录在：直接复用。
  - 未登记、分支还在：用 `Worktrees::add`，即 T236 的接管逻辑，不带 `-b`，加锁并刷新。
  - 分支不在：写 `session.configured{read_only: true}` 并把行里的 `read_only` 置 1，回 `read_only`。
  - `not_found` 的会话见问题 1。
- 只读的执行：会话只读时，`session/send`、`queue_resume`、`set_config` 等会进状态机的调用都拒绝，见问题 4。

### IT-session-archive-restore 的明细

- 归档：保留 worktree；删除 worktree；有 blocker 时 `blocked`；大小写冲突时带 `--force` 删除。
- 有运行中的回合时 `conflict`。
- 恢复：`full`（重建）；`read_only`（先删掉分支）。
- 重命名。
- 列出已归档，含翻页。
- 删除项目：有运行中的回合时 `conflict`；成功后会话都归档，项目从索引移除。

## 拆分方案

估计约 805。按以往偏高 10–20% 的经验，可能越过硬停止线 850。

- **方案 A（推荐）**：
  - **T125**：`worktree/check` 加上 IT-worktree-check-rules，约 285。
  - **新任务**：`archive`、`restore`、`rename`、`list_archived`、`project/remove` 加上 IT-session-archive-restore，约 520。
  - 两个用例 ID 各归一份。
- **方案 B**：
  - **T125**：`check` 加 `archive`，约 450。
  - **新任务**：`restore`、`rename`、`list_archived`、`project/remove`，约 400。
  - IT-session-archive-restore 要拆成两份。

## 开工前要确认的问题

1. **`workspace.failed{not_found}` 的会话怎样恢复**（T236 的交接）。这种会话的 worktree 在 git 里还登记着，但目录不在了。
   - A：直接转为只读。
   - B：在以下两个条件下重建，否则转为只读：
     - 条件：锁是 cupper 自己的，或者没有锁；分支没有在别的 worktree 里检出。
     - 做法：先 `worktree unlock`，再 `worktree add --force <路径> <分支>`。
     - `--force` 只用来越过"路径已登记但目录不在"这一条，不 prune。
   - 推荐 B。
2. **本地会话调用 `worktree/check`**：回 `unsupported`，因为本地会话没有 worktree。
3. **归档时 worktree 已经不在了**：
   - 未登记：结果为 `removed`。
   - 已登记、目录不在（prunable）：结果为 `kept`，记录留给用户清理。
4. **只读会话拒绝写调用时用什么码**：提议 `unavailable`，不带 reason。
5. **已归档的会话收到写调用**（send、set_config 等）：同样回 `unavailable`；`session/get` 照常回答。
6. **`project/remove`**：会话一律保留 worktree；有会话正在准备工作区时，同样回 `conflict/turn_running`。

## 编排者的决定（2026-11-04）

- 拆分选 A：T125 只做 `worktree/check` 和 IT-worktree-check-rules，停止线 350、硬停止线 420。其余归 T240。
- 本地会话调用 `worktree/check` 回 `unsupported`（问题 2）。
- 问题 1、3–6 归 T240：
  - not_found 的会话照方案 B 重建。分支在别的 worktree 里有没有检出，要自己检查，检查时排除这条记录本身。
  - 只读会话和已归档会话的写调用回 `unavailable`，分别带 reason `session_read_only`、`session_archived`。

## 实现要点（T125）

- 新增 `git/cleanup.rs` 的 `check`，由 `GitService` 的 `worktree/check` 调用。`session/archive`（T240）可以直接复用它。
- 第 1、2 条：在快照档的 index 副本上，复用 `status::states`（改为 `pub(super)`）。
  - 脏文件 = 列出的、不是未跟踪的文件。
  - 脏文件都未暂存，而且路径在已跟踪的路径里只差大小写时，记为 `case_only_dirty`，不算第 1 条。
  - 已跟踪的路径取自快照档的 `LsFiles`。
- 被忽略文件的数量：快照档新增 `SnapshotOp::Ignored`，即 `ls-files -z --others --ignored --exclude-standard`。
- 第 3 条：`status::in_progress(git_dir)`。第 7 条：git 目录下的 `modules/` 不为空。两者都在 `blocking` 里读。
- 第 4 条：`listed` 改为返回锁的原因（没有原因的锁记为空串），T236 的调用随之调整；原因不等于 `cupper session <id>` 的锁算外来锁。
- 第 5 条：`rev-list --count HEAD --not --exclude=<本分支> --branches --remotes --tags`。
- 第 6 条：有远程跟踪分支时，执行 `rev-list --count HEAD --not --remotes`。
  - "仓库没有远程"以远程跟踪分支为准：从没 fetch 或 push 过的远程，等同没有远程。
  - 这样不会丢数据：只在这个 worktree 里的提交，第 5 条照样拦住。
- 第 5、6 条在没有任何提交（HEAD 未出生）时都不成立。

## 实测与变异检查

- IT-worktree-check-rules 是表驱动的一条用例：
  - 每行一种情形：干净；修改；暂存；未跟踪；被忽略；进行中的 bisect；外来的锁；只在 HEAD 上的提交；子模块的 git 目录；仅大小写冲突；未推送。
  - 每行做完改动、检查、再撤销，下一行从干净的状态开始。
  - 最后检查本地会话回 `unsupported`。
- 变异检查，都能让用例失败：
  - 不判大小写冲突；
  - 第 5 条不排除本分支。
- 大小写冲突那一行：用例先在 worktree 里建一个文件，看换成另一种大小写的名字能否打开，以此判断文件系统是否区分大小写（编排者的意见）。区分大小写时，这一行改为断言"干净、不报 `case_only_dirty`"，并打印原因。

## 实际规模

`cargo xtask size`：+266 −10，在停止线 350 以内（含探测大小写的 11 行）。

| 文件 | 行 |
|---|---|
| `git/cleanup.rs` | 约 120 |
| `cleanup_tests.rs` | 约 85 |
| 接线（`service.rs`、`cmd.rs`、`worktree.rs`、路由等） | 约 50 |
