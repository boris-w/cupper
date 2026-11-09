# T240 notes

## 开工前的规模估计（2026-11-04）

停止线 600，硬停止线 700。

| 模块 | 内容 | 行 |
|---|---|---|
| 协议 | `ErrorReason::{session_read_only, session_archived}`，归在 `unavailable` 下 | 6 |
| `Worktrees::remove` | 见下文 | 55 |
| `Worktrees::restore` | 见下文 | 45 |
| 会话 actor | 见下文 | 135 |
| 索引 | `Note::Title`；`Note::Archived` 删掉这一项并写 `session.removed{archived}`；删除项目：`projects.removed_at`、`project.removed` | 45 |
| manager | `list_archived`（游标、每页 50，摘要按索引的算法）、`project/remove`、路由 | 110 |
| IT-session-archive-restore | 见下文 | 180 |
| 合计 | | 约 576 |

### `Worktrees::remove`

- 先看登记：
  - 没登记：结果为 `removed`。
  - 已登记但目录不在：结果为 `kept`。
- 否则：
  - 用 `cleanup::check` 取 blockers，不数被忽略文件（见 T125 的交接）。有 blocker 时结果为 `blocked`。
  - 没有 blocker 时，在 `RepoLock` 内写 `pending_external`，然后 `unlock`、`remove`。只在 `remove_case_only_dirty` 时才加 `--force`。

### `Worktrees::restore`

- 已登记且目录在：直接复用。
- 已登记、目录不在：锁是 cupper 自己的或没有锁，并且分支没在别的 worktree 里检出时，先 `unlock`，再 `add --force`。
- 没登记、分支还在：用 `add`，即 T236 的接管逻辑。
- 都不满足：转为只读。

### 会话 actor

- `archive`、`restore`、`rename` 三个处理。
- 只读和已归档会话的写调用被拒。
- `Step` 加上归档和标题；`Rows` 加上 `archived_at`、标题，以及 `session.configured` 的 `read_only` 列。
- 把摘要的组装从 `session/get` 拆出来，恢复时给索引用。

### IT-session-archive-restore

覆盖以下情形：
- 归档：保留、删除、被 blocker 挡住、`remove_case_only_dirty`、worktree 已不在的两种情形。
- 有运行中的回合时 `conflict`。
- 恢复为 `full`、`read_only`。
- not_found 的重建，以及分支在别处检出时转只读。
- 只读、已归档会话的写调用被拒。
- 重命名、列出已归档（翻页）、删除项目（`conflict` 和成功两种）。

## 拆分（2026-11-04）

实现写完后 +701，超过停止线，已经报给编排者。决定如下：

- `session/restore`、只读会话的拒绝和 `session_read_only` 拆到 T241；用例改名为 IT-session-archive。
- 写好的 restore 代码按 T240、T241 混在一起的原样，存在本机分支 `feat/T241-session-restore` 上，提交 `cc55be2c`，从 ee5be85 开出，不推送。
- T240 分支只留归档这部分。

## 实现要点（T240）

- **会话 actor**：`actor/lifecycle.rs`。
  - `refused`：已归档的会话，对 send、interrupt、queue_resume、confirm_config、set_config、interaction/respond，以及再次归档，回 `unavailable/session_archived`；`session/get` 照常回答。
  - `archive`：
    - 有运行中的回合，或者还在准备工作区时，回 `conflict/turn_running`。
    - 否则先 Detach，关闭 agent 会话；再 `Worktrees::remove`。
    - 最后 `archived_at`、账本、给索引的 `Note::Archived` 在同一次提交里。
    - `project/remove` 经信箱的 `Mail::Archive` 走同一段代码，worktree 一律保留。
  - `rename`：行里写 title 和 `title_source = 'user'`，给索引发 `Note::Title`。
- **`Worktrees::remove`**（`git/cleanup.rs`）：在 `RepoLock` 内先读 `worktree list`。
  - 没登记：`removed`。
  - 已登记但目录不在：`kept`。
  - 否则执行 `cleanup::check`，不数被忽略文件（T125 的交接，避免大目录逼近超时），有 blocker 时 `blocked`。
  - 脏文件只是大小写冲突、参数却不是 `remove_case_only_dirty` 时，也算 `blocked`，blockers 为 `[modified_files]`。
  - 都通过后写 `pending_external`，然后在项目目录下执行 `worktree unlock`、`worktree remove`；只在确认了大小写冲突时才加 `--force`。分支保留。
- **索引**：
  - 状态核心收到 `Note::Archived` 时删掉这一项，下次 flush 时写 `session.removed{archived}`。
  - `Note::Title` 改标题，来源记为 `user`。
  - `IndexNotes::project_removed`：索引 actor 在同一批里写 `projects.removed_at` 和 `project.removed`，并把项目从内存的表里删掉。
- **`list_archived`**：
  - 在 `index::archived` 里与索引共用同一条 SELECT。
  - 状态按 `status_of` 计算。
  - 按 `(archived_at, session_id)` 倒序做键集翻页，每页 50。
- **`project/remove`**：
  - 先看项目下已载入会话的 running 标志，有运行中的回合就回 `conflict`，什么都不改。
  - 否则逐个经信箱归档，然后交给索引移除项目。
  - 正在准备工作区的会话由 actor 拒绝，这时前面的会话已经归档了；之后可以重试。

## 实测

- IT-session-archive 是一条用例，覆盖：
  - 归档：保留；删除，分支还在；被未跟踪文件挡住；大小写冲突时带确认删除；没登记时 `removed`；目录不在时 `kept`；
  - 有运行中的回合时，`archive` 和 `project/remove` 都回 `conflict`；
  - 已归档会话的再次归档和 send 被拒；
  - 重命名后 `session/get` 的标题和来源；
  - `project/remove` 成功后最后一个会话也归档；
  - `list_archived` 共 7 条，用游标取后半页。
- 单元用例 `unit_index_status_takes_a_title_and_lets_an_archived_session_go`。
- 进程内夹具的索引是假的，所以索引 actor 那边的 `projects.removed_at` 和 `project.removed` 没有用例覆盖。

## 实际规模

`cargo xtask size`：+697 −46，超过停止线 600，在硬停止线 700 以内。
