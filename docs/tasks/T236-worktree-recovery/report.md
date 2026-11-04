# T236 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务与会话编排）
- 分支：`feat/T236-worktree-recovery`
- 完成日期：2026-11-04
- PR：https://github.com/boris-w/cupper/pull/71 （已合并，main 317ca1e）
- CI 运行：https://github.com/boris-w/cupper/actions/runs/41775168854 ，`25218168` 上 8 个检查全部通过
- test-repeat，各连跑 10 次，都通过：
  - `it_nfr9_failpoints_worktree`（`--features test-harness --test crash_concurrency`，在 `9981b8e4` 上）：https://github.com/boris-w/cupper/actions/runs/41774637237
  - `it_worktree_create`（`--lib`，在 `25218168` 上）：https://github.com/boris-w/cupper/actions/runs/41775162667

## 做了什么

1. **载入时恢复**：会话视图的工作区停在 `Preparing`、`initial_prompt_json` 不为空、`sessions` 行有分支和 `base_ref` 时，`load` 组出 `Plan`（项目路径、`workspace_path`、分支、`base_ref`），交给 actor 重新准备。宿主启动时的 `reopen` 会载入这类会话，不必等有人调用。
2. **`Worktrees::add` 接管崩溃留下的状态**：先在锁内经仓库档执行只读的 `worktree list --porcelain -z`（新增 `RepoOp::WorktreeList`），按记录找这条路径。
   - **已登记**：
     - 目录还在：不再 add，已锁上的也不再 lock，接着照常 refresh，然后写 `workspace.ready`。
     - 目录不在（prunable）：回 `not_found`，写 `workspace.failed`。不执行 `worktree prune`，也不加 `--force`。
   - **未登记**：
     - 分支已在（用 `for-each-ref refs/heads/<分支>` 判断）：执行 `worktree add <路径> <分支>`，不带 `-b`。
     - 分支不在：照常 `worktree add -b`。
   - 原来的 add 那一段拆成了 `checkout` 方法，检出进度照旧上报。
3. **恢复时失败**：恢复时第一条输入是 `Input::Restored`，它会按自己的 `gate` 重设闸门，另发 Gate 会被覆盖。所以失败时把 `workspace_failed` 放进 `restored.gate`；新建的会话照 T124 另发 Gate。两种情况下，第一条消息都转成以 `workspace_failed` 暂停的排队消息。
4. **故障点**：在 `failpoint.rs` 登记两个。
   - `worktree.adding`：`workspace.preparing` 已落盘、已有记录也查过之后，add 命令执行之前。
   - `worktree.added`：add 成功之后，lock 和 `workspace.ready` 之前。
5. **用例**：
   - IT-NFR9-failpoints-worktree：在 `crash_concurrency.rs` 里新增 `crash_in_worktree`，照 T229 的 `crash_in_git` 写，两个点各一条（`before_the_add`、`after_the_add`）。
   - IT-worktree-create 新增三条，用进程内夹具构造状态：
     - 接管已登记的 worktree；
     - 只剩分支时不带 `-b` 再 add；
     - 目录被删掉时失败。
   - 测试辅助：`ManualClock` 新增 `pending()`，返回未到期的计时器个数。

## 关键取舍

- **在 `add` 里判断，不按崩溃点分支**：add 本身先看已有状态，新建会话和恢复走同一段代码，代价是新建时多一次只读调用。这样不必知道上次停在哪一步：只要状态是 git 能表达的，都能接上。
- **prunable 不清理**：`worktree prune` 会顺带清掉用户自己其他失效的 worktree 记录，`--force` 会越过用户的锁，所以只报 `not_found`，把记录留给用户（你的意见）。
- **没登记、但目录已存在且非空**：add 失败，写 `workspace.failed`，第一条消息转成暂停的排队消息。不删用户的目录（你的意见）。
- **进程内构造的办法**：
  - 构造：让会话停在"已 add、已 lock、正等 1 秒"，再 restart。
  - 难点：上一轮的 add task 仍在等它的计时器，两轮的截止时间相同，`next_deadline` 分不出新一轮。
  - 办法：用 `pending()` 等到两个计时器都在再推进时钟。上一轮 task 随后做的 refresh 失败也不影响结果。
- **故障点位置**：两个点都放在确定的位置，避开 git 运行中途（T124 的问题 6）。
  - "add 之前"重启后走 `-b` 新建；
  - "add 之后"重启后走"已登记、未锁"，补上 lock。

## 验证方式

本机（单份运行）：
- `cargo test --workspace --all-features`：第一次提交时 918 个通过，0 失败；补上 prunable 后，`cargo test -p cupper-host --all-features` 617 个通过，0 失败。
- IT-NFR9-failpoints-worktree 单独跑过：2 个通过。
- clippy，带和不带 `--all-features` 两种，都开 `-D warnings`。
- `cargo fmt --all --check`。
- `cargo xtask ci protocol` 和 `cargo xtask ci testdata-scan` 都通过。本任务没有改协议。
- 变异检查，每项都让对应用例失败：
  - 不查已登记、总是 add；
  - 不查分支、总是 `-b`；
  - 去掉目录检查。

对照验收标准：
1. **IT-NFR9-failpoints-worktree 通过，并在 runner 上连跑 10 次**：是，链接见开头。两个点重启后都断言了以下各项：
   - `integrity_check` 通过，seq 连续，崩溃前客户端看到的历史是之后历史的前缀；
   - 用同一 `command_id` 重试 `session/create`，得到库里唯一的那个 `session_id`；
   - 第一个回合结束；
   - `workspace.ready` 恰好一条；
   - `worktree list` 里只有两条（主 worktree 加这一条），分支只有 `cupper/w`，没有重复创建，也没有孤儿。
2. **崩溃复用的两种情形有用例**：是，`it_worktree_create_takes_over_the_worktree_a_crash_left` 和 `it_worktree_create_takes_over_the_branch_a_crash_left`。两条都断言：
   - Attach 的 cwd 就是这个 worktree，第一条消息照常发出；
   - `worktree list` 只有两条，锁的原因是 `cupper session <id>`；
   - 分支只有 `cupper/x`。
   另有 `it_worktree_create_fails_once_the_worktree_a_crash_left_is_gone`，断言 `workspace.failed{not_found}`、队列以 `workspace_failed` 暂停且第一条消息在里面、git 里的记录还在。
3. **没有 worktree 建在临时目录之外**：进程内夹具的数据目录在临时目录里；进程级测试的数据目录和项目都在 `TempDir` 下，worktree 根目录用的是数据目录下的默认值。
4. **8 个检查都通过**：是，链接见开头。

## 设计偏差 / 补充

1. **5.2.13 第 6 条**：判断"已登记"用仓库档的只读命令 `worktree list --porcelain -z`，在 `RepoLock` 内执行。接管规则如下：
   - 已登记且目录在：不再 add；已锁上的不再 lock；refresh 照常做。
   - 已登记但目录不在（prunable）：回 `workspace.failed{not_found}`，不 prune，不 `--force`。
   - 未登记但分支已在：`worktree add <路径> <分支>`，不带 `-b`。
   - 未登记、目录已存在且非空：add 失败，写 `workspace.failed`。
   - 失败时第一条消息都转成以 `workspace_failed` 暂停的排队消息。
2. **5.2.10 仓库档的命令表**：只读命令里加上 `worktree list`。
3. **5.9.8 故障点**："`worktree add` 进行中"落实为两个确定的点：`worktree.adding`（add 之前）和 `worktree.added`（add 之后、lock 之前）。
4. **恢复时的闸门**：第一条输入是 `Restored` 时，`workspace_failed` 放进 `restored.gate`，不另发 Gate，因为恢复会按 `restored.gate` 重设闸门。这是实现细节，供 T125 参考。
5. **测试辅助**：`ManualClock::pending()`。

## 提交列表

```
25218168 fix(host): fail a worktree whose directory is gone
9981b8e4 feat(host): take over the worktree a crash left
```

## 遗留问题 / 范围外发现

- **规模**：+271 −38。补上 prunable 后超过停止线 250，在你指定的硬停止线 300 以内。
- **给 T125**：
  - 归档、恢复时可以复用 `worktree list` 的解析，也就是 `listed`：判断有没有登记、有没有锁。
  - `not_found` 的会话怎样恢复（重建 worktree，或者转为只读），可以在 T125 一并定。
- **给 T144**：`workspace.failed` 新增了 `not_found` 这种情况（worktree 目录被删）。界面要给出提示，并告诉用户可以自己 `git worktree prune`。

worktree 是否保留由你决定，T236 已经结束。
