# T124 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务与会话编排）
- 分支：`feat/T124-worktree-create`
- 完成日期：2026-11-04
- PR：https://github.com/boris-w/cupper/pull/66
- CI 运行：https://github.com/boris-w/cupper/actions/runs/41770272208 ，`afbb9c71` 上 8 个检查全部通过
- test-repeat（`it_worktree_create`，`--lib`，10 次，macos-15）：https://github.com/boris-w/cupper/actions/runs/41770271928 ，全部通过

## 做了什么

范围按拆分后的 T124。崩溃恢复、两个故障点和 IT-NFR9-failpoints-worktree 归 T236。

1. **规划 worktree**（`git/worktree.rs` 的 `Worktrees::plan`，在 `session/create` 里同步执行）：
   - 短名取 `branch_hint`，没有 hint 时用 `s-<会话 ID 末 8 位十六进制>`。
   - 短名不能含 `/`，并且要能通过 `check-ref-format --branch cupper/<短名>`；不合格时回 `invalid_params`，不建会话。
   - 用 `for-each-ref refs/heads/cupper/` 一次读出已有的分支，分支和目录都没被占用时用原名，否则依次试 `-2` 到 `-99`，都被占用时回 `conflict`。
   - 位置是 `<worktrees.root>/<仓库名>-<短哈希>/<短名>`：
     - 仓库名取主 worktree 顶层目录的名字（裸仓库去掉 `.git`）；
     - 短哈希取公共 git 目录 realpath 的 SHA-256 的前 8 位十六进制；
     - 目录按 0700 递归建好后再取 realpath。
   - 项目不是 git 仓库，或者没有 git 时，回 `unsupported/not_a_repo`。
2. **`session/create` 的接线**：
   - `sessions` 行写入 worktree 的路径、`workspace_kind`、`worktree_branch` 和 `base_ref`。
   - `session.configured` 带上 kind、路径、分支和 `base_ref`，接着写 `workspace.preparing{worktree_add}`。
   - 这一批与命令结果在同一个事务里，用 `Durability::Full` 落盘，作为 T236 恢复时的在途标记。
   - 索引摘要里带上 `workspace_kind` 和分支。
3. **添加 worktree**（`Worktrees::add`，由会话 actor 在单独的 task 里执行，计入 quiescence 的 hold 和 `outstanding`）：
   - 拿 `RepoLock`，经仓库档执行 `worktree add -b <分支> <路径> <base_ref>`。仓库档的覆盖照常生效，过滤器和 hook 都不运行；超时用 `git_worktree_add`（10 分钟）。
   - 接着执行 `worktree lock --reason "cupper session <id>"`，然后放锁。
   - 写 `workspace.preparing{refresh}`，经 `Clock` 等 1 秒（新登记的时长 `worktree_refresh`）。
   - 再拿锁，在 worktree 里执行一次 `update-index --refresh`；没有加 `--relative-paths`。
   - 检出进度：`run::hearing` 把 stderr 一边读一边转发出去；`worktree::checkout` 只解析完整的行，百分比变化时写 `workspace.preparing{checkout, n}`。
   - 仓库档的 notice 交给 actor 写。
4. **`required` 过滤器**：仓库档照旧报 `RequiredFilter`，并照旧写审计和 notice；到了 worktree 这里，回 `workspace.failed{git_error, filter_unsupported}`。
5. **会话 actor 的准备步骤**：
   - 第一条输入先留着。
   - 期间会进状态机的调用（send、interrupt、queue_resume 等）先排着，就绪后按原顺序补上；`session/get` 这类只读调用照常回答。
   - 成功：写 `workspace.ready{path, branch}`，`cwd` 换成 worktree 的 realpath。之后的信任重扫、快照和 Attach 都用它。
   - 失败：同一批里写 `workspace.failed{code, reason}`，并以 `workspace_failed` 关闸；然后交第一条消息，它进入暂停的队列。
   - 准备期间不清 `initial_prompt_json`。
   - 工作区视图为 `Failed` 时，`queue_resume` 回 `unavailable`。
   - 载入时，队列以 `workspace_failed` 暂停的会话，闸门保持关着。
6. **`mixed_changes`**（handoff，GT-1）：`git/diff` 的回合范围只在 `workspace_kind = 'local'` 时加。
7. **协议**：只附加了 `PauseReason::workspace_failed`，并重新生成了 TS 类型和 schema。
8. **共用的锁**：`RepoLock` 改由 supervisor 建好，传给 Git 服务（`GitDeps.locks`）和 `Worktrees` 两边；`writable()` 改成两边共用的函数。

## 关键取舍

- **在途标记**：没有用账本的 `pending_external`。`session/create` 的结果已经和 `workspace.preparing` 一起提交，账本里没有可以标记的在途命令，所以改为 FULL 落盘的 `workspace.preparing`（你的决定 1）。
- **名字在 `session/create` 里同步决定**：这样 `sessions` 行（`writable()` 要用）和 `session.configured` 从一开始就是最终的路径和分支。代价是，决定之后、add 之前如果别人建了同名分支，add 会失败，写 `workspace.failed`。
- **两段加锁**：锁只覆盖 add 和 lock 两步；等 1 秒期间不占锁，刷新前再拿一次，避免在等待时挡住其他会话的暂存和提交。
- **刷新失败不算失败**：刷新只是省掉后续读取时重新计算哈希。
- **会话状态不进 `error`**（你的决定）：状态照常计算，结果是 `idle`。
- **准备期间的调用先排着**：否则，第一条消息还没进回合，后发的消息就可能先开始回合，并在还不存在的目录里 Attach。
- **进度**：实测 git 2.50 的 `worktree add` 在 stderr 是管道时也会写进度，所以做了增量读取。`run::run` 的签名保持不变，新增 `run::hearing`。

## 验证方式

本机（单份运行）：
- `cargo test --workspace --all-features`：914 个通过，0 失败。
- clippy，带和不带 `--all-features` 两种，都开 `-D warnings`。
- `cargo fmt --all --check`。
- `cargo xtask ci protocol`：通过，schema-compat 0 breaking、4 additive。本任务新增的只有 `PauseReason "workspace_failed"`，其余三项来自 main。
- `cargo xtask ci testdata-scan`：通过。

GitHub：PR #66 上 8 个检查全部通过；test-repeat 连跑 10 次全部通过。

对照验收标准：
1. **IT-worktree-create 通过**，并在 runner 上连跑了 10 次。IT-NFR9-failpoints-worktree 已移到 T236。
   - `it_worktree_create_adds_locks_and_refreshes_then_attaches_there`：
     - 预先建好 `cupper/x`，新建的会话得到 `cupper/x-2`；路径为 `<数据目录>/worktrees/app-<8 位>/x-2`；根目录和仓库目录都是 0700。
     - 准备阶段依次是 `worktree_add`、`refresh`；推进时钟 1 秒之前不写 `workspace.ready`，推进之后才写。
     - 准备期间发来的消息排在第一条之后。
     - `workspace.ready` 的路径和分支正确；`worktree list --porcelain` 里有这个 worktree、它的 HEAD、分支，以及 `locked cupper session <id>`。
     - agent 在 worktree 里 Attach；回合结束后，diff 只有这个会话的改动，不带 `mixed_changes`。
   - `it_worktree_create_refuses_a_bad_name_and_a_required_filter_fails_it`：
     - `a/b` 和 `x..y` 两个短名回 `invalid_params`，没有建会话。
     - `required` 过滤器导致 `workspace.failed{git_error, filter_unsupported}`。
     - 重启前后都一样：会话状态是 `idle`；队列以 `workspace_failed` 暂停，里面是第一条消息；`queue_resume` 回 `unavailable`；`initial_prompt_json` 已经清空。
   - 单元用例 `unit_git_worktree_progress_is_the_last_whole_line_of_the_checkout`：按块拆开的 `\r` 进度行能解析，不完整的行留到下一块。
   - 变异检查：总是加 `mixed_changes`、不把 reason 改成 `filter_unsupported`，这两处改动都会让对应的用例失败。
2. **没有 worktree 建在临时目录之外**：夹具的数据目录就是临时目录，`worktrees.root` 的默认值因此是 `<临时目录>/worktrees`，测试不碰本机的 `~/.cupper`。
3. **`schema-compat` 0 breaking**：是。
4. **PR 上 8 个检查全过**：是，链接见开头。

## 设计偏差 / 补充

1. **5.2.10 / 5.2.13 的在途标记**：`session/create` 那一批用 FULL 落盘，`workspace.preparing` 充当 `worktree add` 的在途标记，不经账本（决定 1，你在合并时写进设计）。
2. **短名和后缀**：
   - `branch_hint` 只能是单段，并且要通过 `check-ref-format --branch`，否则回 `invalid_params`；没有 hint 时用 `s-<会话 ID 末 8 位十六进制>`。
   - 分支和目录用同一个名字，后缀 `-2` 到 `-99`，都被占用时回 `conflict`，在 `session/create` 里同步决定。
3. **`<仓库名>-<短哈希>`**：仓库名取主 worktree 顶层目录的名字（裸仓库去掉 `.git`）；短哈希取公共 git 目录 realpath 的 SHA-256 前 8 位十六进制；目录按 0700 递归创建。
4. **`required` 过滤器**：回 `workspace.failed{git_error, filter_unsupported}`。
5. **失败后的处理**：
   - 新增 `PauseReason::workspace_failed`，第一条消息转成暂停的排队消息。
   - `queue_resume` 回 `unavailable`；会话状态照常计算，结果是 `idle`。
   - 界面（T144）要凭工作区视图的 `Failed{code, reason}` 和这个暂停的队列来显示失败，请转进交接事项。
6. **新增时长 `worktree_refresh`（1 秒）**。等待期间不占 `RepoLock`，刷新前再拿；刷新失败不算失败。
7. **进度**：`worktree add` 的 stderr 一边读一边转发（`run::hearing`），按 `Updating files: n%` 写 `workspace.preparing{checkout, n}`；git 默认要过 2 秒才开始写进度。
8. **`RepoLock` 由 supervisor 建好**，Git 服务和 `Worktrees` 共用；`GitDeps` 新增 `locks` 字段。
9. **准备期间的调用**：会进状态机的调用先排着，就绪或失败后按原顺序补上。设计里没有写到这一条，属于补充。
10. **不是 git 仓库的项目**：建 worktree 时回 `unsupported/not_a_repo`。原来 M1a 有一条用例断言这里回不带 reason 的 `unsupported`，已经随之改了。
11. **项目须是工作树的顶层目录**：项目是仓库的子目录时，仓库档读不到布局，同样回 `not_a_repo`。
12. **`base_ref` 不在 `session/create` 时校验**：写错的 `base_ref` 要等 add 失败，才得到 `workspace.failed{git_error}`。

## 提交列表

```
afbb9c71 feat(host): new worktrees for sessions
```

## 遗留问题 / 范围外发现

- **T236**：
  - 载入时停在 Preparing、并且还有 `initial_prompt_json` 的会话，要重新准备：复用已登记的 worktree；分支已经在时不带 `-b` 再 add。
  - 两个故障点 `worktree.adding`、`worktree.added`，IT-NFR9-failpoints-worktree，以及崩溃复用那一条用例。
  - 在 T236 合并之前，宿主在 add 途中崩溃的话，重启后的会话不会恢复。
- **界面（T144）**：
  - 需要按第 5 条显示失败；
  - `filter_unsupported` 的提示文案在 5.2.10；
  - 选择 `base_ref` 时最好从 `git/branches` 里挑，避免 add 才失败。
- **规模**：+700 −70，正好在停止线上，你已经接受。

worktree 请保留，T236 接着用。
