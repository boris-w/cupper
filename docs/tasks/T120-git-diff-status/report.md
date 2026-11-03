# T120 汇报（收窄后）

- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`feat/T120-git-diff-status`，提交 `0e1c0f0`（基于 `38f85e0`，没有 rebase 或 reset）
- 完成日期：2026-11-03
- PR：https://github.com/boris-w/cupper/pull/42
- CI 运行：https://github.com/boris-w/cupper/actions/runs/41582793610

**CI 还没有结果。** 交汇报时 8 个检查都还在 pending。harness 要求我现在交，所以没能按你说的等结果。请看上面的运行链接，有失败用 SendMessage 叫我，我在同一个 worktree 里修。

## 做了什么

按裁定收窄为 `git/diff` 四种范围、`DiffCache` 和路由。移出的代码留在 `38f85e0`，取回清单写在 `notes.md`：

- T221 取回：`git/diff/hunks.rs`，`git/diff.rs` 里的 `patch` 和 `Diffed.place`，`git/service.rs` 里的 `file_diff` 和 `inline`，`SnapshotOp::DiffPatch`。
- T222 取回：`git/status.rs`，`git/service.rs` 里的 `status` 和 `branches`，`SnapshotOp::Refresh`、`DiffIndex`、`DiffFiles`。

留在本任务的：

1. **快照档的拆分**：`snapshot::take` 拆成 `open`/`close` 和拍快照两步。`open` 重写配置并复制 index；`Opened::empty_tree` 按对象格式给出空树。`shoot` 和新的共用函数 `output` 给 diff 复用。
   - 新增两个快照档命令：`ls-files -z --unmerged`，以及 `diff-tree -r -z -M --raw --numstat --no-ext-diff --no-textconv`。
   - 新增 `ShotError::Unmerged`。
   - `Fingerprint` 加了 `Default`：diff 自己拍的快照不参与回合比较。
2. **`git/diff.rs` 的四种范围**：
   - `last_turn`、`turn:<id>`：两次快照对比；回合还在进行时，起始快照对比现拍的一次快照。
   - `unstaged`：把复制来的 index `write-tree`，对比现拍的快照，未跟踪文件包含在内。
   - `staged`：HEAD 对比 index 的 tree；还没有提交时用空树。
   - `unstaged`、`staged` 遇到未合并条目时返回 `unavailable/index_unmerged`。
   - 文件列表解析 raw 和 numstat 的合并输出：重命名带 `old_path`，二进制文件标 `binary`。
   - 配置定义了快照档不执行的过滤器时，用 `check-attr` 标出 `filter_unsupported`。
   - 纯函数 `turn_notes` 和 `add` 从会话视图算出回合的 `snapshot`（各次快照的排除项合并）和 `late_tool_effects`。
3. **`git/head.rs`**：HEAD 的提交经仓库档两次调用取得：先 `branch --show-current`（新增 `RepoOp::BranchCurrent`），再用 `for-each-ref`（在分支上）或 `rev-parse --verify -q HEAD`（游离 HEAD）。
4. **`DiffCache`（纯）**：每个工作区保留最近 32 个，过期时间按 `diff_cache_ttl`（已登记进时长表，10 分钟），时间由调用方经 `Clock` 传入。
5. **`git/service.rs`（`GitService`，实现 `Port`）**：
   - 从 `sessions` 表找到会话的工作区，从 `turns` 表读回合的 tree。
   - `snapshot_missing`：回合缺起始快照，或已结束却没有结束快照。
   - 回合的 notes 先在会话视图里找；不在窗口里的回合读回合快照（`store::log::turn_ops` 改为 `pub`）。
   - `diff_token` 由 `IdGen` 生成，结果放进 `DiffCache`。
   - 仓库档的错误经新增的 `GitError::reported` 转成协议错误，并写 `git.unsafe_repo_config` 审计；`projects.rs` 改用同一个函数。
6. **接线**：
   - `Routes::git` 只登记 `git/diff`；宿主组装和会话测试夹具都登记了 `GitService`。
   - UNIT-unsupported-set 的允许集合只去掉了 `git/diff` 一行。

协议没有改动，所以不需要 `pnpm gen`，`schema-compat` 不受影响。

## 和 r1 不同的地方

- **范围**：`git/file_diff`、hunk 切分、大 hunk 转 blob、`git/status`、`git/branches`、`cupper diff` 都移出了。只是移出，代码没有删，`38f85e0` 里都在。
- **HEAD**：r1 里的 `status::head` 同时取分支、上游和领先落后；收窄后 `git/head.rs` 只取提交，供 `staged` 用，其余由 T222 在它上面加回。
- **`DiffCache` 存的内容**：r1 存 `Diffed` 时带着 `place`（file_diff 生成补丁用），收窄后只存两个 tree、文件列表和运行中回合现拍的快照。`DiffCache::get` 目前只有单元用例调用，第一个真正的调用方是 T221 的 `git/file_diff`。
- **`GitDeps`**：去掉了 `blobs`，T221 再加回。
- **快照合并**：现拍快照与回合快照的排除项合并抽成了 `diff::add`，service 里不再重复这段逻辑。

## 关键取舍

照 r1，你已经接受：`DiffCache` 存 tree 对和文件列表，不存补丁；列表用 `-M`；HEAD 用 `BranchCurrent` 取；`snapshot` 字段只在回合范围给。

另外一点：HEAD 的两次调用都选了在任何 HEAD 状态下都以 0 退出的命令。原因是仓库档在配置了 `required` 过滤器的仓库里，会把任何非零退出判成 `unsafe_repo_config`（T098 的规则 5）。

## 验证方式

- **IT-git-diff-scopes**：用会话夹具加真实 git，在一个真实仓库上跑。
  - 第一次提交前的 `staged`：index 对比空树，5 个新增文件，其中一个是 filter。
  - 回合进行中的 `last_turn`：修改、删除、二进制新增、重命名（`old_path`）、`filter_unsupported`，`snapshot` 为 partial 且带排除项，`stat` 正确，`caveats` 为 `[mixed_changes]`。
  - 回合结束后，`last_turn` 和 `{turn: id}` 结果一致，每次的 `diff_token` 都不同。
  - `unstaged` 与回合结果一致，没有 `caveats` 和 `snapshot`。
  - `git add` 部分文件之后，`staged` 和 `unstaged` 各自分开。
  - 错误：去掉结束快照返回 `snapshot_missing`；不存在的回合返回 `not_found`；构造未合并的 index 后，`unstaged` 和 `staged` 都返回 `index_unmerged`。
- **变异检查**：去掉 `index_unmerged` 检查、去掉 `filter_unsupported` 标记，IT 都会失败。
- **单元用例**：`unit_git_diff_turn_notes_...`（排除项合并，以及 `late_tool_effects` 对晚到条目、仍可能运行的命令、已结束的命令三种情况的判定）、`unit_git_diff_cache_...`、UNIT-unsupported-set。
- **本机**（单份运行）：
  - `cargo test -p cupper-host --all-features --no-fail-fast` 全部通过：lib 458 个，各集成测试二进制都通过。
  - `cargo clippy --workspace --all-targets` 和 `-p cupper-host --all-targets --features test-harness` 都没有警告；`cargo fmt` 已跑。
  - `cargo xtask ci testdata-scan` 通过。
- **规模**：`cargo xtask size` 是 **+998 −54**，没有越过停止线 1000。任务书里仍写着 900，所以工具会提示超出停止线；新的上限由你补进任务书。
- **验收标准**：
  - 四种范围都有 IT 覆盖并通过。
  - `schema-compat`：协议没有改动。
  - CI 8 项：待出结果。

## 提交列表

```
0e1c0f0 feat(git): git/diff of the four ranges, with DiffCache
38f85e0 feat(git): git/diff, file_diff, status and branches (in progress)
```

## 遗留问题

- r1 的 5 条范围外发现照旧，你已收进交接事项。
- 设计 5.2.10 里有两处需要改：`DiffCache` 的写法（存 tree 对，不存补丁），以及仓库档的只读命令清单补上 `branch --show-current`。
