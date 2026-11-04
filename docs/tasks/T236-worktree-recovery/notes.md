# T236 notes

## 开工前的规模估计（2026-11-04）

停止线 250，硬停止线 300。

| 模块 | 内容 | 行 |
|---|---|---|
| 载入时恢复（`load.rs`） | 视图停在 Preparing、仍有第一条消息的 worktree 会话：取项目路径，组成 `Plan` 交给 actor | 15 |
| actor | 恢复时第一条输入是 `Input::Restored`，它会重设闸门：失败时把 `workspace_failed` 放进 `restored.gate`，不另发 Gate | 10 |
| `Worktrees::add` 的复用 | 在仓库档新增只读的 `worktree list --porcelain -z`：已登记就跳过 add，已锁就跳过 lock；分支已在时不带 `-b` | 35 |
| 故障点 | `worktree.adding`、`worktree.added` | 8 |
| IT-NFR9-failpoints-worktree（`crash_concurrency.rs`） | 照 `crash_in_git`：5.9.8 的检查、重试 `session/create` 得到原来的 `session_id`、`workspace.ready`、第一个回合结束、只有一个 worktree 和一条分支 | 65 |
| IT-worktree-create 的崩溃复用两条 | 进程内夹具：add 与 lock 已完成、正等 1 秒时 `restart`（复用）；restart 前把 worktree 移除、只留分支（不带 `-b`） | 65 |
| 合计 | | 约 200 |

## 实现要点

- **载入**：
  - 视图的工作区停在 `Preparing`、`initial_prompt_json` 不为空、`sessions` 行有分支和 `base_ref` 时，`load` 用项目路径、行里的 `workspace_path`、分支和 `base_ref` 组成 `Plan`，交给 actor 重新准备。
  - 宿主启动时 `reopen` 会载入这些会话，所以不等有人调用就开始恢复。
- **`Worktrees::add` 先看已有的**：在锁内执行仓库档只读的 `worktree list --porcelain -z`（新增 `RepoOp::WorktreeList`），按记录找这条路径：
  - 已登记：跳过 add；已经锁上的，也跳过 lock。
  - 没登记：用 `for-each-ref refs/heads/<分支>` 看分支在不在。在，就 `worktree add <路径> <分支>`；不在，就照常 `-b`。
  - lock 和 1 秒后的 refresh 照 T124。
  - 新建的会话也走同一段检查，多一次只读调用。
- **故障点**：
  - `worktree.adding`：在 add 命令执行之前；这时 `workspace.preparing` 已经落盘，已有记录也查过了。
  - `worktree.added`：add 成功之后、lock 之前。
  - 已登记的 worktree 不会走到这两个点。
- **恢复时失败**：第一条输入是 `Input::Restored`，它会按自己的 `gate` 重设闸门。所以失败时把 `workspace_failed` 放进 `restored.gate`，不另发 Gate；新会话照旧另发 Gate。
- **测试辅助**：给 `ManualClock` 加了 `pending()`（未到期的计时器个数）。进程内夹具 restart 后，上一轮 add 的 task 还在等它那 1 秒，两轮的截止时间相同，只能靠计时器个数判断新一轮已经开始等待。

## 实测与变异检查

- IT-NFR9-failpoints-worktree 的两个点都通过（`--features test-harness --test crash_concurrency`）。
- 进程内的两条接管用例都通过。各做了一次变异，都会失败：
  - 不查已登记、总是 add；
  - 不查分支、总是 `-b`。

## 实际规模

`cargo xtask size`：+234 −37，在停止线 250 以内。

| 文件 | 行 |
|---|---|
| `git/worktree.rs` | +76 −28（add 拆出 `checkout`） |
| `crash_concurrency.rs` | +66 |
| `worktree_tests.rs` | +49 |
| `load.rs` | +22 −6 |
| `actor.rs` | +8 |
| `failpoint.rs`、`clock/manual.rs`、`cmd.rs` | +13 |

## 边界情形（编排者 2026-11-04 的意见）

- **已登记、但目录被删掉了（prunable）**：
  - `add` 回 `not_found`；actor 写 `workspace.failed{not_found}`，第一条消息转成以 `workspace_failed` 暂停的排队消息。
  - 不执行 `worktree prune`（它会顺带清掉用户其他失效的 worktree 记录），也不加 `--force`；这条记录留给用户自己清理。
  - 用例 `it_worktree_create_fails_once_the_worktree_a_crash_left_is_gone`。去掉这段检查做变异，用例会失败。
- **没登记、但目录已经存在且非空**：add 失败，写 `workspace.failed`，按这样处理。
- 加上这一条后，规模为 +271 −38，在硬停止线 300 以内。
