# T120 notes

## 开工前的规模估计（2026-11-03）

停止线 900，硬停止线 1100。按模块（不含生成物和 lock；协议类型 M1a 已全部冻结在 `methods/git.rs`，不改协议）：

| 模块 | 内容 | 估计 |
|---|---|---|
| `git/cmd.rs`、`git/snapshot.rs` | 快照档新增的命令（`ls-files -u`、`diff-tree --raw --numstat`、`diff-tree -p`、`update-index --refresh`、`diff-index`、`diff-files`），仓库档 `symbolic-ref` 等沿用；把 `take` 拆成"打开 GIT_DIR"和"拍快照"两步，diff 与 status 复用 | 55 |
| `git/diff.rs` | 四种范围求 tree 对、`index_unmerged`、列表解析（raw 与 numstat 合并输出）、`filter_unsupported`（`check-attr`）、`stat`、`file_diff` 生成补丁 | 160 |
| `git/diff/hunks.rs`（纯） | 补丁切成 `hunk`、`mode`、`whole_file` 单位，按内容寻址的 `hunk_id` | 100 |
| `git/diff/cache.rs`（纯） | `DiffCache`：每个工作区最近 32 个、10 分钟过期，时间由调用方经 `Clock` 传入 | 45 |
| `git/status.rs` | `git/status`（分支、上游、领先落后走仓库档；进行中的操作看标记文件；文件状态在快照档）、`git/branches` | 100 |
| `git/service.rs` | `Port`：会话到工作区、回合到 tree（`turns` 表）、`caveats` 与 `snapshot`（会话视图，窗口外的回合读回合快照）、错误与 `git.unsafe_repo_config` 审计、大 hunk 转 blob、登记路由 | 150 |
| 接线 | `Routes::git`、宿主组装、`projects.rs` 的审计转换改用共用函数 | 25 |
| `cupper diff` | 参数与输出 | 45 |
| 测试 | hunks、cache、caveats 的单元用例约 90；IT-git-diff-scopes（会话夹具 + 真实 git：四种范围、三种单位、`stale_diff`、`index_unmerged`、`snapshot_missing`、status、branches）约 150；UNIT-unsupported-set 与 IT-cli-commands 的改动约 15 | 255 |
| 合计 | | 约 935 |

估计越过停止线（900）约 4%，在硬停止线（1100）以内，已报编排者。

## 设计上的取舍（实现前定下的）

- **DiffCache 存什么**：设计写"diff_token → 这次 diff 的全部补丁"。这里存 tree 对、文件列表和工作区；`git/file_diff` 每次按 tree 对重新生成该文件的补丁。tree 按内容寻址、不可变，同一对 tree 生成的补丁逐字节相同，`hunk_id` 也按内容寻址，所以与存补丁等价，而且内存只跟文件列表成正比（一次 diff 的补丁可达 64 MB，32 个就是 2 GB）。T121 暂存时同样按 tree 对重新生成、按 `hunk_id` 取单位。
- **重命名**：列表用 `-M`，`old_path` 照给；补丁用 `--no-renames`（ADR-10），重命名的 `file_diff` 是旧路径的删除与新路径的新增，两个都是 `whole_file`。
- **HEAD（`staged` 与 `git/status` 共用）**：快照档的 GIT_DIR 没有用户的引用，HEAD 经仓库档两次调用取得：`branch --show-current`（新增的 `RepoOp::BranchCurrent`，还没有提交时也给出分支名），再按有无分支 `for-each-ref`（提交、上游、领先落后）或 `rev-parse --verify -q HEAD`。不用 `symbolic-ref -q` 和 `rev-parse HEAD^{tree}`：它们在游离 HEAD、还没有提交时以非零退出，而仓库档在配置了 `required` 过滤器的仓库里会把任何非零退出判成 `unsafe_repo_config`（T098 的规则 5）。还没有提交时比较对象用空树（sha1、sha256 各一个固定值）。
- **`git/status` 的文件状态**：在快照档的临时 index 上 `update-index --refresh` 后，`diff-index --cached -M <HEAD>` 得到 index 一栏，`diff-files` 得到工作区一栏，`ls-files -o` 得到未跟踪。不在快照档里跑 `git status`：它要读 HEAD，而 GIT_DIR 是同一工作区的快照共用的。

## 越过硬停止线，暂停（2026-11-03）

写完宿主侧的实现、还没写 IT-git-diff-scopes 和 `cupper diff` 时，`cargo xtask size` 已是 **+1358 −60（合计 1418）**，超过硬停止线 1100。按守则第 8 条停下，由编排者决定是否拆分。估计严重偏低：实现约 1,140 行，估的是约 675 行（status 估 100 实际 220，Port 估 150 实际 350）。

已完成（编译、clippy 干净，单元用例通过；未跑 IT）：

| 部分 | 文件 | 行 |
|---|---|---|
| 快照档：`take` 拆成 `open`/`close` 与拍快照，新增 6 个只读或写临时 index 的命令，`output` 共用 | `git/cmd.rs`、`git/snapshot.rs`、`git/stamp.rs` | 约 165 |
| diff：四种范围、列表解析、`filter_unsupported`、补丁、回合的 `snapshot` 与 `late_tool_effects`（纯函数） | `git/diff.rs` | 251 |
| hunk 切分（纯，含 2 个单元用例） | `git/diff/hunks.rs` | 208 |
| `DiffCache`（纯，含 1 个单元用例），`diff_cache_ttl` 登记进时长表 | `git/diff/cache.rs`、`durations.rs` | 114 |
| status、branches、HEAD（仓库档两次调用） | `git/status.rs` | 220 |
| Port：会话到工作区、回合到 tree、视图里的 notes（窗口外读回合快照）、大 hunk 转 blob、错误 | `git/service.rs` | 351 |
| 接线：`Routes::git`、宿主组装、会话夹具、UNIT-unsupported-set 去掉 4 行、`GitError::reported`（`projects.rs` 改用它） | 其余 | 约 110 |

还没做：IT-git-diff-scopes（约 130）、列表解析与 notes 的单元用例（约 40）、`cupper diff`（约 50）、IT-cli-commands 加一段（约 6）。全做完预计约 1,650。

拆分建议（每一步合并后 main 都可用、没有无人调用的代码）：

1. **T120（本分支）**：快照档拆分、`git/diff` 四种范围、`DiffCache`、`git/diff` 的路由、IT-git-diff-scopes 的范围部分。`staged` 要 HEAD，所以带上 `status::head`。约 950–1,000。
2. **T120b**：`git/file_diff`、hunk 切分、大 hunk 转 blob、IT-git-diff-scopes 的三种单位部分（或新编号）。约 350。
3. **T120c**：`git/status`、`git/branches`、`cupper diff`、相应用例（需新编号，如 IT-git-status）。约 330。

另一种做法是提高本任务的上限到约 1,700，一次做完。
