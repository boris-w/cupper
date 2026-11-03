# T221 notes

## 开工前的规模估计（2026-11-03）

停止线 450，硬停止线 550，取回的代码也算。取回部分是实测行数（T120 的 `0e1c0f0`、`38f85e0`），其余是估计：

| 模块 | 内容 | 行 |
|---|---|---|
| `git/diff/cache.rs` | `DiffCache`（纯）：每个工作区 32 个、`diff_cache_ttl` 过期、`insert` 与 `get` | 75（实测，不含单元用例） |
| `git/diff/hunks.rs` | 补丁切成 `hunk`、`mode`、`whole_file`，按内容寻址的 `hunk_id` | 132（实测，不含单元用例） |
| `git/diff.rs` | `Diffed.old`/`new`/`place`、`patch`（单文件、`--no-renames`、重命名带上旧路径） | 22 |
| `git/service.rs` | 按方法分派、`file_diff`、大 hunk 转 blob、缓存的放入与取出、`GitDeps.blobs` | 76 |
| `git/cmd.rs`、`durations.rs`、路由与接线 | `SnapshotOp::DiffPatch`、`diff_cache_ttl`、`Routes::git` 加 `git/file_diff`、UNIT-unsupported-set 去一行、宿主与会话夹具的 `blobs` | 28 |
| 测试 | IT-git-file-diff：三种单位，新增、删除、二进制、符号链接、子模块、LFS 指针，重命名，`hunk_id` 跨 diff 不变，大 hunk 转 blob，缓存命中、32 个上限、10 分钟过期与 `stale_diff`，路径不在 diff 里 | 约 130 |
| 合计 | | 约 460 |

T120 里 hunks（75 行）和 cache（36 行）各有单元用例；连同它们合计约 570，会越过硬停止线。我原想不取回、改由 IT 覆盖，编排者裁定不要这样（2026-11-03）：两份单元用例都取回，纯核心的边界（各种整文件、32 个上限、10 分钟过期）由单元用例守住，IT 只覆盖真实 git 上的整条路径、不与单元用例重复；规模放宽为停止线 580、硬停止线 620。按此重估：取回的单元用例 111，IT 约 100，合计约 540。

## 完成时

- `cargo xtask size`：+556 −29，低于放宽后的停止线 580（分支上的 brief 还写着 450/550，编排者在 main 上改）。
- 取回：`cache.rs`、`hunks.rs` 原样取回，连同各自的单元用例；`Diffed.place`/`old`/`new`、`patch`、`SnapshotOp::DiffPatch`、`diff_cache_ttl`、service 的缓存、`file_diff`、`inline`。新写的部分只有 service 按方法分派和 IT。
- IT-git-file-diff 不再重复单元用例覆盖的单位种类、32 个上限和 `hunk_id` 的稳定性。它只测真实 git 上的整条路径：token 取出缓存里的 tree 对、按 pathspec 重新生成单个文件的补丁、模式加两个 hunk、重命名的两个整文件、长文件转 blob、过滤器文件没有单位、路径不在 diff 里、未知 token、经宿主 `Clock` 过期。
- 变异检查：`patch` 不带重命名的旧路径、不转 blob，IT 都失败。
