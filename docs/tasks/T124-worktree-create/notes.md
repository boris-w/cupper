# T124 notes

## 开工前的规模估计（2026-11-04）

停止线 700，硬停止线 850。估计参考了 T123 的实际情况，已经把 rustfmt 的展开（约 +20%）算进去。

| # | 模块 | 内容 | 行 |
|---|---|---|---|
| 1 | 命名与位置 | 纯函数：短名、后缀、`<仓库名>-<短哈希>`、路径；根目录建成 0700；经仓库档读取：`check-ref-format --branch`、分支是否已在、目录是否已在 | 90 |
| 2 | `worktree add` 执行 | 仓库档、`RepoLock`、10 分钟超时；`required` 过滤器失败时回语义码；进度：`run.rs` 增量读 stderr，解析 `Updating files: n%`，经回调交给会话 actor | 110 |
| 3 | 锁与刷新 | `worktree lock --reason "cupper session <id>"`，经 `Clock` 等 1 秒，`update-index --refresh` | 30 |
| 4 | `session/create` 接线 | 见下文"接线"一节 | 55 |
| 5 | 会话 actor 的准备步骤 | 见下文"准备步骤"一节 | 100 |
| 6 | `mixed_changes` 只在本地模式下加（GT-1，handoff） | 按 `sessions.workspace_kind` 判断 | 10 |
| 7 | 崩溃恢复 | 见下文"崩溃恢复"一节 | 70 |
| 8 | 故障点 | 在 `failpoint.rs` 登记 `worktree.adding`（见问题 6） | 8 |
| 9 | 协议 | `PauseReason::workspace_failed`，只做附加（见问题 5） | 5 |
| 10 | 单元用例 | 命名与后缀（表驱动）、进度解析、`worktree list` 解析 | 70 |
| 11 | IT-worktree-create | 见下文"IT-worktree-create"一节 | 220 |
| 12 | IT-NFR9-failpoints-worktree | 照 T229，加在 `crash_concurrency.rs` 里，见下文 | 60 |
| | 合计 | | 约 830 |

### 4. `session/create` 接线

- 在 `accept()` 里处理 worktree 分支：项目不是 git 仓库时回 `unsupported/not_a_repo`。
- 在创建的事务里写：
  - 新增 `Rows::created` 的 worktree 写法：`sessions` 行写进 kind、路径、分支和 `base_ref`；
  - `session.configured.workspace`，带同样的四项；
  - 写 `workspace.preparing{phase: worktree_add}`，代替原来的 `workspace.ready`；
  - 这一批用 FULL 落盘。
- 索引摘要里的 `workspace_kind` 和 `branch` 也随之填上。

### 5. 会话 actor 的准备步骤

- worktree 模式下，actor 先准备工作区，再把第一条消息（`start.first`）交给核心。
- 进度经信箱，由 actor 自己写 `workspace.preparing{checkout, n}`，保持单写者。
- 成功：写 `workspace.ready{path, branch}`，`cwd` 换成 worktree 的 realpath；之后的信任重扫和 Attach 都用这个 realpath。
- 失败：写 `workspace.failed{code, reason}`，第一条消息转成一条暂停中的排队消息（5.2.13），会话状态为 `error`。

### 7. 崩溃恢复

- 载入时，工作区视图停在 Preparing、并且还有 `initial_prompt_json` 的会话，重新准备工作区。
- 判断 worktree 是否已登记：在仓库档新增只读的 `worktree list --porcelain -z`，解析其输出。
- 已登记的 worktree 直接复用：
  - lock 已经存在时视为成功；
  - 补做刷新，然后写 `workspace.ready`。
- 没有登记、但分支已经在了（上次的 `worktree add -b` 建好分支后才中断）：执行 `worktree add <path> <branch>`，不带 `-b`。

### 11. IT-worktree-create

用会话夹具加真实的 git，worktree 根目录用数据目录的默认值，夹具里就是临时目录。覆盖：

- 新建成功：路径、根目录 0700、分支、`base_ref`、`git worktree list` 里的锁和原因。
- `ManualClock` 推进 1 秒之前不会出现 `workspace.ready`，推进后才写，以此检查"等 1 秒后刷新"。
- 第一个回合的 cwd 是 worktree。
- 分支 `cupper/x` 已存在时得到 `cupper/x-2`。
- 非法的短名：`invalid_params`，不建会话。
- `required` 过滤器：得到 `workspace.failed`，第一条消息保留为暂停的排队消息。
- 崩溃后复用：用测试用的 hold 停在"add 之后、lock 之前"，然后 `restart`。
- 本地模式下 `mixed_changes` 不变（已有用例）；worktree 模式下不带 `mixed_changes`。

### 12. IT-NFR9-failpoints-worktree

照 T229，加在 `crash_concurrency.rs` 里：

- worktree 会话在故障点退出；重启后按 5.9.8 检查：完整性、seq 连续、历史前缀。
- 同一 `command_id` 重试 `session/create`，得到原来的 `session_id`。
- 第一条消息恰好一次，第一个回合结束。
- `git worktree list` 里这个 worktree 只有一个，没有孤儿。

## 超过停止线：拆分方案

估计约 830，超过停止线 700，接近硬停止线 850。建议拆成两部分：

- **T124（本任务，约 630）**：表中第 1–6、9 项；第 10 项除去 `worktree list` 解析；第 11 项除去崩溃复用那一条。准备失败的处理也在本任务里：`workspace.failed`、第一条消息转成暂停的排队消息。
- **T124b（约 200）**：
  - 第 7 项崩溃恢复和第 8 项故障点；
  - `worktree list` 解析的单元用例；
  - IT-worktree-create 的崩溃复用一条；
  - IT-NFR9-failpoints-worktree。
  - 依赖 T124；T125 和 T172 改为依赖 T124b。

## 开工前要确认的问题

1. **`pending_external`**：
   - 按 5.2.13，`session/create` 的命令结果和 `workspace.preparing` 在同一个事务里提交，`worktree add` 在这之后才执行，此时账本里已经没有可以标记的在途命令。
   - 提议：把这一批的落盘方式改为 FULL，用 `workspace.preparing` 充当"外部操作在途"的标记。
   - 崩溃后按工作区视图（Preparing）加"是否已登记"来恢复，不经账本；重试 `session/create` 按账本得到原来的 `session_id`。
2. **短名**：
   - 有 `branch_hint` 时，用 `check-ref-format --branch cupper/<hint>` 校验，并且要求它是单段（不含 `/`，目录名和分支名一致）；不合格时 `session/create` 直接回 `invalid_params`，不建会话。
   - 没有 `branch_hint` 时，用 `s-<会话 ID 的末 8 位十六进制>`。
3. **后缀**：
   - 分支和目录用同一个名字：`<短名>`、`<短名>-2`、`<短名>-3` …… 至多到 `-99`，超过就回 `conflict`。
   - 在 `session/create` 里同步决定，这样 `sessions` 行（`writable()` 要用它）和 `session.configured` 从一开始就是最终的路径和分支。
   - 决定之后、`add` 之前，如果别人建了同名分支，`add` 会失败，写 `workspace.failed`。
4. **`<仓库名>-<短哈希>`**：
   - 仓库名取主 worktree 顶层目录的名字。
   - 短哈希取仓库公共 git 目录 realpath 的 sha256 的前 8 位十六进制。
   - 根目录建成 0700 后取 realpath（临时目录在 `/private/var` 下）。
5. **`required` 过滤器**：
   - 回 `workspace.failed{code: git_error, reason: filter_unsupported}`。这个 reason 已经存在，界面可以凭它给出 5.2.10 的那段提示，与其他 `unsafe_repo_config` 区分开。
   - 失败后的暂停原因：提议附加 `PauseReason::workspace_failed`。
   - 工作区失败的会话，`session/queue_resume` 回 `unavailable`；以后怎么恢复留给 T125。
6. **故障点的位置**："`worktree add` 进行中"，真正放在 git 运行中途的话：
   - git 在自己的进程组里，宿主退出后它会继续跑完，或者在写 stdout 时被 SIGPIPE 杀掉，结果不确定。
   - 提议照 T229 登记两个确定的点：
     - `worktree.adding`：`workspace.preparing` 已落盘，`add` 还没执行；
     - `worktree.added`：`add` 已完成，lock、刷新、`ready` 都还没做。
   - 这样分别覆盖"重做"和"复用"两条路径；也可以只登记一个，请定。
7. **进度**：git 的 "Updating files" 进度有 2 秒的延迟才显示（`--progress` 也一样），小仓库在测试里看不到。进度只在单元用例里用假 git 测，IT 不断言进度事件。

## 编排者的决定（2026-11-04）

- 拆分同意：崩溃恢复、两个故障点（`worktree.adding`、`worktree.added`）、IT-NFR9-failpoints-worktree 和崩溃复用那一条用例归 T236。T124 只做 FULL 落盘和在途标记。
- 问题 1–5、7 照提议。会话状态不做 `error`：状态照常计算；界面（T144）凭工作区视图的 `Failed{code, reason}` 和以 `workspace_failed` 暂停的队列显示失败。

## 实现要点

- **进度**：
  - 实测 git 2.50：`worktree add` 在 stderr 是管道时也会写 `Updating files: n% (..)`，以 `\r` 分隔，默认 2 秒后才开始写。
  - `run::hearing` 把读到的 stderr 原样转发到 `Heard` 通道；`run::run` 的签名不变。
  - `worktree::checkout` 只解析完整的行，取最后一个百分比，变化时才写 `workspace.preparing{checkout, n}`。
- **`git/worktree.rs`**：
  - `Worktrees` 带 git、读库、时钟、时长、审计、设置，以及与 Git 服务共用的 `RepoLock`。原来由 `GitService` 自己建的锁，改为由 supervisor 建好后传给两边。
  - `plan`：
    - 短名不含 `/`，并用 `check-ref-format --branch cupper/<短名>` 校验。
    - 用 `for-each-ref refs/heads/cupper/` 一次读出已有的分支，再逐个检查目录，挑出第一个空闲的名字。
    - 根目录按 0700 递归建好，再取 realpath。
  - `add`：
    - 拿锁，执行 `worktree add -b <分支> <路径> <base_ref>`，再执行 `worktree lock --reason "cupper session <id>"`，然后放锁。
    - 写 `refresh` 阶段，经 `Clock` 等 1 秒（时长登记为 `worktree_refresh`）。
    - 再拿锁，在 worktree 里执行 `update-index --refresh`。刷新失败不算失败，它只是省掉后续读取时重新计算哈希。
- **`required` 过滤器**：仓库档照旧报 `RequiredFilter`，在 worktree 这里改为 reason `filter_unsupported`；审计和 notice 仍照仓库档的方式写。
- **`session/create`**：
  - 在 `accept` 里先生成会话 ID 再做 `plan`，错误照旧拒绝命令。
  - `sessions` 行写入 worktree 的路径、`workspace_kind`、`worktree_branch` 和 `base_ref`。
  - 写 `session.configured`（kind 为 worktree），再写 `workspace.preparing{worktree_add}`；这一批用 `Durability::Full`。
- **会话 actor**：
  - 第一条输入先留着，在单独的 task 里 add（计入 quiescence 的 hold 和 `outstanding`）。
  - 期间的进度、notice 由 actor 自己写。
  - 其他会进状态机的调用（send、interrupt、queue_resume 等）先排着，就绪后照原顺序补上。`session/get` 这类只读调用照常回答。
  - 成功：写 `workspace.ready`，`cwd` 换成 worktree。
  - 失败：在同一批里写 `workspace.failed` 和关闸（`workspace_failed`），然后交第一条消息，它进入暂停的队列。
  - 准备期间不清 `initial_prompt_json`，等第一条消息进入回合或队列时再清。
- **`queue_resume`**：工作区视图为 `Failed` 时回 `unavailable`。载入时，队列以 `workspace_failed` 暂停的会话，闸门保持关着。
- **`mixed_changes`**：`git/diff` 的回合范围只在 `workspace_kind = 'local'` 时加。

## 实际规模

`cargo xtask size`：+700 −70，正好在停止线上。

| 文件 | 行 |
|---|---|
| `git/worktree.rs`（含 1 个单元用例） | 约 250 |
| `worktree_tests.rs` | 约 145 |
| `actor.rs` | 101 |
| `manager.rs` | 约 60 |
| `service.rs` | 34 |
| `run.rs`、`cmd.rs`、`mod.rs` | 约 48 |
| 夹具、supervisor、rows、load、协议等 | 其余 |

估计的约 630 加上：
- `Worktrees` 与共用锁的接线：夹具调整了设置的建立顺序；
- 准备期间排队调用的处理（估计里没有）。
