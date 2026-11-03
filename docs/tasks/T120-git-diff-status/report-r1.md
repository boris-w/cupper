# T120 汇报（暂停：越过硬停止线，等编排者决定拆分）

- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`feat/T120-git-diff-status`（提交 `38f85e0`，未推送）
- 日期：2026-11-03（未完成）

## 结论

宿主侧的实现写完时，`cargo xtask size` 已是 **+1358 −60，合计 1418 行**，超过硬停止线 1100。IT-git-diff-scopes、`cupper diff` 和几个单元用例都还没写，全部做完预计约 **1,650 行**。按守则第 8 条在这里停下，请你决定怎么拆，或者提高上限。

开工前我估约 935 行，也给你发过消息，但估计严重偏低：实现部分估约 675 行，实际约 1,140 行。偏差主要在两处：status 估 100 实际 220，Port 估 150 实际 350。

## 做了什么（编译通过、clippy 无警告、单元用例通过；IT 还没写，没跑 CI）

| 部分 | 行数 |
|---|---|
| 快照档：`snapshot::take` 拆成 `open`/`close` 和拍快照两步，diff 和 status 复用；新增 6 个快照档命令（`ls-files -u`、`diff-tree --raw --numstat -M`、`diff-tree -p --no-renames …`、`update-index -q --refresh`、`diff-index --cached -M`、`diff-files`），抽出共用的 `output` | 约 165 |
| `git/diff.rs`：四种范围各自得到一对 tree（含 `index_unmerged` 和空树），解析 raw 加 numstat 的合并输出，用 `check-attr` 标 `filter_unsupported`，`stat`，单文件补丁的生成；纯函数 `turn_notes` 算回合的 `snapshot` 和 `late_tool_effects` | 251 |
| `git/diff/hunks.rs`（纯）：切出 `hunk`、`mode`、`whole_file` 三种单位（新增、删除、二进制、符号链接、子模块、LFS 指针）；`hunk_id` 按内容寻址 | 208（含 2 个单元用例） |
| `git/diff/cache.rs`（纯）：`DiffCache`，每个工作区最多 32 个，10 分钟过期，时间由调用方传入；`diff_cache_ttl` 登记进时长表 | 114（含 1 个单元用例） |
| `git/status.rs`：HEAD（仓库档两次调用）、进行中的操作（标记文件）、文件状态（快照档）、`git/branches` | 220 |
| `git/service.rs`：`Port` 实现，负责会话到工作区、回合到 tree（`turns` 表）、视图里的 notes（窗口外的回合读回合快照）、大 hunk 转 blob、错误映射 | 351 |
| 接线：`Routes::git`、宿主组装、会话测试夹具；UNIT-unsupported-set 去掉 4 行（已通过）；新增 `GitError::reported`，`projects.rs` 改用它 | 约 110 |

协议没有改动：M1a 冻结的类型已经齐全，所以 `pnpm gen` 和 `schema-compat` 都不受影响。

## 拆分建议

每一步合并后 main 都能用，也不留没人调用的代码：

1. **T120（本分支）**：快照档拆分、`git/diff` 四种范围、`DiffCache`、路由、IT-git-diff-scopes 的范围部分。`staged` 需要 HEAD，所以 `status::head` 也在这一步。约 950–1,000 行。
2. **T120b**：`git/file_diff`、hunk 切分、大 hunk 转 blob、三种单位的用例（沿用 IT-git-diff-scopes 或新编号）。约 350 行。
3. **T120c**：`git/status`、`git/branches`、`cupper diff` 和它们的用例（需要新编号，例如 IT-git-status）。约 330 行。

另一种做法是把本任务上限提到约 1,700 行，一次做完。决定后用 SendMessage 告诉我，我在同一个 worktree 继续，请先别删 worktree。

## 关键取舍（请确认）

- **DiffCache 存 tree 对、文件列表和打开的 GIT_DIR，不存补丁**：`git/file_diff` 每次按 tree 对重新生成那一个文件的补丁。tree 不可变，生成的补丁逐字节相同，`hunk_id` 也按内容寻址，所以和存"全部补丁"等价，内存却只跟文件列表成正比。T121 也按同样的方式重新生成，再按 `hunk_id` 取单位。建议把 5.2.10 的写法改过来。
- **重命名**：文件列表用 `-M`，照给 `old_path`；补丁用 `--no-renames`（ADR-10），所以重命名在 `file_diff` 里是旧路径的删除加新路径的新增，两个都是 `whole_file`。为了只取那一个文件、不带同名目录下的内容，pathspec 用 `:(literal)P` 加 `:(exclude,literal)P/`。
- **HEAD 的取法**：先 `branch --show-current`（新增的 `RepoOp::BranchCurrent`，这条命令不在设计的仓库档清单里，请补进去），再取 `for-each-ref` 或 `rev-parse --verify -q HEAD`。没有用 `symbolic-ref -q` 或 `rev-parse HEAD^{tree}`：它们在游离 HEAD 或还没有提交时会非零退出，而仓库档在配置了 `required` 过滤器的仓库里，会把任何非零退出判成 `unsafe_repo_config`（T098 的规则 5）。
- **status 的文件状态**：在临时 index 上先 refresh，再用 `diff-index --cached -M <HEAD>`、`diff-files` 和 `ls-files -o` 三条命令得出。没有在快照档里直接跑 `git status`，因为它要读 HEAD，而同一工作区的快照共用一个 GIT_DIR。
- **late_tool_effects 的判定**：回合里有 `late_tool_output` 条目；或者有 `may_still_run` 的命令，它在回合结束前结束，而它的晚到条目在回合开始前还没有结束。
- **`snapshot` 字段**：只在回合范围里给。运行中的回合，把现拍快照的排除项并进去。

## 验证

- `cargo check`、`cargo clippy -p cupper-host --tests` 无警告，`--features test-harness` 也能编译。
- 单元用例通过：hunk 切分 2 个、DiffCache 1 个，以及 UNIT-unsupported-set 的 2 个。
- IT-git-diff-scopes、CLI 和 GitHub CI 都还没做。本机只单份跑了单元用例。

## 提交列表

```
38f85e0 feat(git): git/diff, file_diff, status and branches (in progress)
```

## 范围外发现

1. **项目是仓库的子目录时**：`Layout::read` 要求工作区目录里就有 `.git`，所以回合快照会记成 skipped/not_a_repo，diff 和 status 也拿不到结果。这是 T061 就有的限制，会影响 monorepo 的子目录项目。
2. **仓库档拒绝或配置变化时，会话流里没有 notice**：Git 服务不是会话流的写者，要写 `notice{unsafe_repo_config / repo_config_changed}` 得经会话 actor 转一道。现在只返回错误，并写 `git.unsafe_repo_config` 审计。T121 的写操作也需要这条通路。
3. **`git/diff` 的文件列表没有 blob 兜底**：几百个文件以上会超过 64,000 字节的帧，宿主只能回 `too_large`。要改需要在协议里附加一个可选字段。
4. **`unstaged` 里 git-crypt 这类过滤器的文件**：没有改动的这类文件也会显示为"已变化"，因为快照里是占位 blob，而 index 里是密文。可以改成工作区内容与 index 的 stat 一致时保留 index 条目。
5. **指纹的开销**：每次仓库档调用都要遍历整个工作区两次（handoff 里已经提过）。status 和 `staged` 各要两次仓库档调用，带大 `node_modules` 的仓库会慢到秒级。

## 编排者的决定（2026-11-03）

按建议拆成三份：T120（收窄）、T221（`git/file_diff` 与 hunk 切分，IT-git-file-diff）、T222（`git/status`、`git/branches`、`cupper diff`，IT-git-status）。T121 改为依赖 T221，T123、T125 改为依赖 T222。关键取舍全部接受，T120 合并时改设计 5.2.10。计划 v1.3.86。
