# T240 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务与会话编排）
- 分支：`feat/T240-session-archive`
- 完成日期：2026-11-04
- PR：https://github.com/boris-w/cupper/pull/78
- CI 运行：https://github.com/boris-w/cupper/actions/runs/41795395782 ，`7413a067` 上 8 个检查全部通过
- test-repeat（`it_session_archive`，`--lib`，10 次）：https://github.com/boris-w/cupper/actions/runs/41795396007 ，全部通过（`7413a067`）

## 做了什么

范围按拆分后的 T240：archive、rename、list_archived、project/remove，加上已归档会话的拒绝。restore 和只读会话的拒绝归 T241。

1. **`session/archive`**（会话 actor，`actor/lifecycle.rs`）
   - 有运行中的回合，或者还在准备工作区时，回 `conflict/turn_running`。
   - 否则先 Detach，关闭 agent 会话；再按参数保留或删除 worktree。
   - 本地会话一律 `kept`。
   - `archived_at`、账本、给索引的 `Note::Archived` 在同一次提交里写入。
2. **`Worktrees::remove`**（`git/cleanup.rs`），在 `RepoLock` 内先读 `worktree list`：
   - 没登记：`removed`。
   - 已登记、目录不在：`kept`，记录留给用户。
   - 否则用 `cleanup::check` 取 blockers，不数被忽略文件（T125 交接的超时风险）。有 blocker 时为 `blocked`，带上 blockers。
   - 脏文件只因大小写冲突、而参数不是 `remove_case_only_dirty` 时，也是 `blocked`，blockers 为 `[modified_files]`。
   - 通过检查后：先写 `pending_external`，再在项目目录下 `worktree unlock`、`worktree remove`。只有确认过的大小写冲突才加 `--force`。默认保留分支。
3. **已归档会话的写调用**：send、interrupt、queue_resume、confirm_config、set_config、interaction/respond，以及再次归档，都回 `unavailable/session_archived`（新增的附加 reason）。`session/get` 照常回答。会话载入时从行里读出是否已归档。
4. **`session/rename`**：行里写 `title`，`title_source = 'user'`，并给索引发 `Note::Title`（T100 的交接）。
5. **索引**
   - 状态核心收到 `Note::Archived` 时删掉这一项，下次 flush 写 `session.removed{archived}`。
   - `Note::Title` 改标题和来源。
   - 新增 `IndexNotes::project_removed`：索引 actor 在同一批里写 `projects.removed_at` 和 `project.removed`，并从内存的表里删掉这个项目。
6. **`session/list_archived`**
   - `index::archived` 与索引共用同一条 SELECT，摘要的状态按 `status_of` 计算。
   - 按 `(archived_at, session_id)` 倒序做键集翻页，每页 50；还有下一页时才带 `next`。
7. **`project/remove`**
   - 项目不存在：`not_found`。
   - 已载入的会话有运行中的回合：`conflict/turn_running`，什么都不改。
   - 否则经信箱 `Mail::Archive`，逐个以"保留 worktree"归档会话，再由索引移除项目。账本在最后完成。
8. **接线**：路由；dispatch 用例删去 4 行 Later；actor 的 `Reply::Config` 改名为 `Reply::Result`，供生命周期的命令共用。
9. **T241 的代码**：写在一起的原样存在本机分支 `feat/T241-session-restore`（`cc55be2c`，从 ee5be85 开出），没有推送。

## 关键取舍

- **归档放在会话 actor 里，按顺序执行**：actor 是会话流和会话行唯一的写者，归档和账本可以在一次提交里完成。git 删除期间 actor 不处理别的调用；会话这时是空闲的，所以可以接受。
- **未确认的大小写冲突算 `blocked [modified_files]`**：不加 `--force` 的话，git 本身就会拒绝删除；用已有的枚举值告诉界面原因，界面可以再按 `worktree/check` 的 `case_only_dirty` 询问用户。
- **删除前不数被忽略文件**：归档只关心 blockers；数被忽略文件可能在大目录上逼近超时，不能因此让归档失败。
- **`project/remove` 逐个经各自的 actor 归档**：保持单写者，worktree 一律保留（决定 6）。

## 验证方式

本机（单份运行）：
- `cargo test --workspace --all-features`：925 个通过，0 失败。
- clippy（带和不带 `--all-features` 两种，`-D warnings`）、`cargo fmt --all --check`。
- `cargo xtask ci protocol`：通过，schema-compat 的附加项是 `ErrorReason "session_archived"`。
- `cargo xtask ci testdata-scan`：通过。

对照验收标准：
1. **用例通过，并在 runner 上连跑 10 次**：IT-session-archive（`it_session_archive_keeps_removes_and_is_kept_from_work`）通过，连跑见开头。它覆盖：
   - 归档的 6 种情形：
     - 保留：worktree 还在；
     - 删除：目录已删、分支还在；
     - 被未跟踪文件挡住：`blocked [untracked_files]`，目录还在；
     - 大小写冲突时带确认删除；
     - worktree 已经没登记：`removed`；
     - 已登记、目录不在：`kept`。
   - 6 条 `cupper/*` 分支全部保留。
   - 有运行中的回合时，`archive` 和 `project/remove` 都回 `conflict turn_running`。
   - 已归档的会话再次归档、send，都回 `unavailable session_archived`。
   - 重命名后，`session/get` 的标题为 `mine`、来源为 `user`，`archived_at` 存在。
   - 回合结束后 `project/remove` 成功，项目里最后一个会话也已归档。
   - `list_archived` 共 7 条；用第 3 条作游标，得到后 4 条，且没有 `next`。归档时刻相同，所以这一步同时验证了按 `session_id` 打破并列。
   - 单元用例 `unit_index_status_takes_a_title_and_lets_an_archived_session_go`：标题和来源进入摘要；`Note::Archived` 变成 `Action::Remove`，之后这个会话的 Note 不再产生输出。
2. **没有 worktree 建在临时目录之外**：worktree 根目录是夹具数据目录下的默认值。
3. **`schema-compat` 0 breaking**：是。
4. **8 个检查都通过**：是，链接见开头。

## 设计偏差 / 补充

1. **5.2.6 / 5.5.9**：
   - 已归档会话的写调用回 `unavailable`，带附加的 `ErrorReason::session_archived`（决定 4、5）。
   - 再次归档也同样拒绝。
2. **5.5.9**：
   - `worktree remove` 在项目目录下执行，用的是 worktree 的绝对路径。
   - 未确认的大小写冲突算 `blocked [modified_files]`。
   - 归档时不数被忽略文件。
3. **5.3.4 `session/list_archived`**：每页 50，键集游标 `(archived_at, session_id)` 倒序。
4. **`project/remove`**：先查已载入会话的 running 标志，再逐个经 actor 归档（worktree 保留），最后索引写 `removed_at` 和 `project.removed`。
5. **`IndexNotes` 新增 `project_removed`，状态核心新增 `Action::Remove`**。

## 提交列表

```
7413a067 feat(host): archive sessions, rename them, and remove projects
```

## 遗留问题 / 范围外发现

- **规模**：+697 −46，超过停止线 600，在硬停止线 700 以内，已获接受。
- **交给 T241 的三条**（你已写进 T241 的任务书）：
  1. `project/remove` 现在可能部分归档：预检只看已载入会话的 running 标志，正在准备工作区的会话由 actor 拒绝，这时排在前面的会话已经归档了。要改成先问遍所有会话，全部可以归档再动手。
  2. 未确认的大小写冲突（`remove` → `blocked [modified_files]`）补一行用例。
  3. 索引 actor 写 `projects.removed_at` 和 `project.removed` 那一段，在进程级宿主上补断言。进程内夹具用的是假索引，所以这一段现在没有用例覆盖。
- **T241**：restore 的代码在本机分支 `feat/T241-session-restore`（`cc55be2c`）上，派发后 merge main，把 restore 部分移过去。

worktree 请保留，T241 接着用。
