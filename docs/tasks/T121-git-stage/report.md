# T121 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`feat/T121-git-stage`（从 main `05ce7d9` 开出）
- 完成日期：2026-11-03
- PR 和 CI 运行链接：还没有。`0ecefa2` 已请你推送，交汇报时还没有 PR 和 CI 结果；CI 失败就叫我回来修。

## 做了什么

范围按拆分后的 (a)，另加会话流的 notice 通路。

1. **`git/stage`、`git/unstage`、`git/revert`**（`git/service.rs`）共用一条写路径：
   - 按 `diff_token` 取出缓存里的 diff，取不到返回 `conflict/stale_diff`。
   - 核对操作与范围：stage 只认 `unstaged`，unstage 只认 `staged`，revert 认 `unstaged` 和回合范围；对不上也返回 `stale_diff`。
   - 解析 target：
     - `{all}`：对整对 tree 生成补丁，始终排除 `filter_unsupported` 的文件；
     - `{paths}`：只生成这些文件的补丁，含重命名的源路径；路径不在 diff 里返回 `stale_diff`，文件在过滤器下返回 `git_error/filter_unsupported`；
     - `{hunk_ids}`：对整对 tree 跑一次 `diff-tree -p`，用新增的 `hunks::pick` 选出单位，有 ID 找不到就返回 `stale_diff`。缓存里没有加 hunk_id 到文件的索引，按你认可的做法每次重新生成。
   - 补丁为空（target 指向的文件全在过滤器下，或者 diff 本身为空）时，返回 `stale_diff`（"nothing to apply"），不拿空输入去跑 `git apply`。
   - 暂存前，只要配置里有快照档不执行的过滤器，就在用户仓库里对目标文件跑一次 `check-attr -z --stdin filter`（新增的 `RepoOp::CheckAttr`，走仓库档的防护）。命中 diff 里记下的那组过滤器，就返回 `filter_unsupported`。
   - 然后拿 `RepoLock`，调 `Ticket::mark_external()` 等记录以 FULL 加 fullfsync 落盘，再在仓库档执行 `apply --whitespace=nowarn [--cached] [-R]`。
   - window 角色来的 revert 不带真实输入证据时，由网关已有的 `policy::git_revert` 拒绝。
2. **`hunks::pick`**：按 ID 选单位，同一文件的合成一节：
   - 整文件单位原样输出；
   - `mode` 单位只带 `diff --git` 行和两行模式行；
   - `hunk` 单位先带该文件头部的 `index`、`---`、`+++` 行，再接选中的 hunk。头部只在 `@@` 之前找，删掉的行即使以 `--- ` 开头也不会被误认成头部。
   - `diff::patch` 改为可以一次生成多个文件或整个 diff。`Diffed` 另记下范围和快照档不执行的过滤器。
3. **`RepoLock`**：按仓库的 common dir 各一把异步锁，只有写操作拿；快照档和只读操作都不拿。
4. **账本**：
   - 成功：记下 `{}`，重试回放。
   - `apply` 失败（git 原子地拒绝，什么都没改）：返回 `stale_diff`，index.lock 被占时返回 `git_error/lock_held`；ticket 被 reject，重试会再执行一次。
   - git 被杀或没能启动、调用后的指纹算不出：结果未知，ticket 保持 `pending_external`，重试得到 `conflict/outcome_unknown`。
5. **会话流的 notice 通路**：
   - 新增 `SessionNotes` trait，由 `SessionManager` 实现：notice 放进会话 actor 的信箱（新增 `Mail::Notice`），由 actor 这个会话流唯一的写者写成 notice 条目。
   - 仓库档调用中途配置变了、或者调用被拒，都经这条路写 notice，包括读操作里取 HEAD 和暂存前的 `check-attr`；没有会话的 `git/branches` 不写。
6. **接线**：`Routes::git` 加上三个写方法；UNIT-unsupported-set 去掉三行；宿主组装和会话夹具都接上了 `notes`。

协议没有改动，所以 `schema-compat` 不受影响。

## 关键取舍

- **从 hunk_id 找文件**：每次对整对 tree 重新生成补丁再挑，缓存里不加索引。只有写操作付这个代价，实现最简单，是你认可的做法。
- **补丁为空时返回 `stale_diff`**，没有当成功处理：不拿空输入去跑 git，客户端刷新后会看到确实没有可做的。如果你更希望返回 `{}`，改动约 5 行。
- **"结果未知"的判定**：只有 git 被杀、没能启动、或调用后的指纹算不出时才保留 `pending_external`。其余失败都发生在 git 生效之前，或者 git 原子地拒绝了补丁，所以 reject，重试可以再执行。

## 验证方式

- **IT-git-stage**（会话夹具加真实 git，`it_git_stage_writes_what_a_token_names`），覆盖：
  - 按 hunk_id 暂存 `a.txt` 的第二个 hunk；同一个 command_id 再发得到 `{"replayed": true}`；staged diff 只有这一处改动。
  - 同一个 hunk 再暂存一次，补丁打不上：`stale_diff`。
  - 拿 unstaged 的 token 做 unstage，范围不对：`stale_diff`。
  - 按路径暂存新文件成功；对过滤器下的文件：`git_error filter_unsupported`。
  - 拿 staged 的 token 全部取消暂存。
  - window 角色不带 input 做 revert：`forbidden user_activation_required`；harness 全部还原后，只剩过滤器下的文件。
  - 在工作区放一个被 include 的配置文件，再暂存：`git_error unsafe_repo_config`，会话流里多出一条 `unsafe_repo_config` notice，路径是那个文件。
- **单元用例**：`unit_git_hunks_pick_makes_a_patch_of_the_units_chosen`，覆盖单个 hunk、只选 mode、全选后与原补丁逐字节相同、什么都不选四种情况。原有的 hunk、cache 单元用例和 IT-git-diff-scopes、IT-git-file-diff、IT-git-status 照常通过。
- **本机**（单份运行）：
  - `cargo test --workspace --all-features --no-fail-fast` 全部通过。
  - `cargo clippy --workspace --all-targets`，带和不带 `--all-features`，都没有警告。
  - `cargo xtask ci testdata-scan` 通过。
- **规模**：`cargo xtask size` 为 **+517 −50**，在停止线 550 以内（开工前估约 450）。
- **验收标准**：
  - IT-git-stage 通过。IT-git-hunk 归 T228，IT-NFR9-failpoints-git 归 T229。
  - `schema-compat`：协议没有改动。
  - CI 的 8 项：等推送后的结果。

## 提交列表

```
0ecefa2 feat(git): git/stage, git/unstage and git/revert
```

（编排者另在分支上合并了 main：e9d9377，带进 T126 和 T160，`routes.rs` 和会话测试夹具取两边的并集。）

## 遗留问题 / 交给后续任务

- **T229**：
  - `RepoLock` 的并发互斥、`mark_external` 之后崩溃的重试，这两项都没有测，适合放进故障点扫描：在 `mark_external` 之后、`apply` 前后各登记一个故障点。
  - Q32：在 T098 的恶意夹具上跑三个写操作，确认什么程序都没执行。
- **T228**：82 个用例可以直接走 `git/stage`、`git/unstage`、`git/revert` 加 `git/file_diff` 这条产品路径。`hunks::pick` 合成的补丁格式（mode 单位只带模式行，hunk 单位带文件头）对应 SP-5 的"mode lines stripped"做法。
- **范围核对的覆盖**：revert 拿 `staged` token 这类组合在 IT 里没有专门的用例。git 本身会拒掉大部分不对的组合，但"revert 用 staged token"在工作区与 index 相同时 git 会照做，只靠范围核对拦下。T228 可以补一条。
