# T221 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`feat/T221-git-file-diff`（从 main `c97522c` 开出），提交 `973a51f`，已请你推送
- 完成日期：2026-11-03

**PR 和 CI 都还没有结果。** harness 要求我现在就交汇报。交的时候查不到 PR，连 GitHub 也超时了，所以不确定分支是否已推送。CI 有失败就用 SendMessage 叫我，我在同一个 worktree 里修。

## 做了什么

从 T120 的 `38f85e0`、`0e1c0f0` 取回代码，按你的裁定把两份单元用例都取回了。新写的只有 service 里的方法分派和 IT。

1. **`DiffCache`**（`git/diff/cache.rs`，纯核心，原样取回）：每个工作区保留最近 32 个，存活时间是 `diff_cache_ttl`（10 分钟，已登记进时长表），时间由调用方经 `Clock` 传入。`git/diff` 把结果放进缓存，四种范围共用同一套 token。
2. **`git/file_diff`**：
   - 按 token 从缓存取出 tree 对，在快照档用 `diff-tree -p --no-renames --no-color --no-ext-diff --no-textconv --full-index --binary -U3` 重新生成这一个文件的补丁（新增 `SnapshotOp::DiffPatch`）。
   - pathspec 用 `:(literal)P` 加 `:(exclude,literal)P/`，只取这个文件，不带同名目录下的内容。
   - 重命名会同时带上源路径，结果是"旧路径删除"和"新路径新增"两个 `whole_file` 单位。
3. **hunk 切分**（`git/diff/hunks.rs`，纯核心，原样取回）：
   - 三种单位：`hunk`、`mode`（权限位单独一个单位）、`whole_file`。
   - `whole_file` 覆盖新增、删除、二进制、符号链接（120000）、子模块（160000）和 LFS 指针文件。
   - `hunk_id` 是 `hk_` 加 24 位十六进制，由文件的 `diff --git` 行、单位种类和单位内容的 sha-256 截取而来，同一处改动在不同的 diff 里 ID 不变。
4. **大 hunk 转 blob**：一个 hunk 的行转义后超过 `max_inline_bytes`，或者整个结果快要超过一帧（留出 8,000 字节给信封和路径）时，这些行改用 `lines_blob`，媒体类型 `text/x-diff`。blob 不挂引用，过了孤儿 blob 的宽限期就会被回收。
5. **接线**：`Routes::git` 加上 `git/file_diff`；`GitDeps` 加上 `blobs`，宿主和会话夹具都已接上；UNIT-unsupported-set 去掉 `git/file_diff` 一行。

协议没有改动，`schema-compat` 不受影响。

## 实际语义（供你改写设计 5.2.10）

- **缓存的键和值**：键是 `diff_token`（`dt_` 加 `IdGen` 生成的 UUIDv7，十六进制），值是工作区路径、tree 对、文件列表、打开 GIT_DIR 时的 `Place`，不存补丁。
- **淘汰**：每次放入或查询前，先清掉超过 `diff_cache_ttl` 的项（从生成时起算，不按最近使用续期）。同一工作区第 33 个放入时，挤掉这个工作区最旧的一个。
- **查询**：要求 token 存在、没有过期、并且属于请求里那个会话的工作区。三条有一条不满足，`git/file_diff` 都返回 `conflict`（`reason: stale_diff`）。宿主重启后缓存清空，旧 token 也一样返回 `stale_diff`。
- **按需重生成**：tree 不可变，所以每次生成的补丁逐字节相同。IT 里连续两次请求同一个文件，结果完全一致。
- **其他错误**：
  - `path` 不在这次 diff 的文件列表里：`not_found`。
  - 文件标了 `filter_unsupported`：返回空的 `hunks`，不运行 git。
  - 生成补丁时 git 失败：`git_error`；超时：`timeout`。

## 关键取舍

- 两份单元用例都取回了，守住纯核心的边界：各种整文件单位、`mode`、`hunk_id` 的稳定性、32 个上限、10 分钟过期。IT-git-file-diff 不和它们重复，只测真实 git 上从 token 到单位的整条路径。
- `file_diff` 的工作区取会话行里的 `workspace_path`，和 `git/diff` 放入缓存时用的是同一个值。

## 验证方式

- **IT-git-file-diff**（会话夹具加真实 git，`it_git_file_diff_units_of_a_diff_by_its_token`），覆盖：
  - 一个文件改了权限位又有两处改动：单位依次是 `mode`、两个 `hunk`，带表头和行；两次请求结果相同。
  - 重命名：两个 `whole_file` 单位。
  - 3,000 行的新文件：没有 `lines`，blob 的内容正确。
  - 过滤器文件：没有单位。
  - 路径不在 diff 里：`not_found`；未知 token：`conflict stale_diff`；推进会话夹具的 `ManualClock` 10 分钟后：`conflict stale_diff`。
- **变异检查**：`patch` 不带重命名的源路径、关掉转 blob，IT 都失败。
- **单元用例**：`unit_git_hunks_*` 两个、`unit_git_diff_cache_*` 一个，IT-git-diff-scopes 和 UNIT-unsupported-set 也照常通过。
- **本机**（单份运行）：
  - `cargo test -p cupper-host --all-features --no-fail-fast` 全部通过（lib 466 个，各集成测试二进制都通过）。
  - `cargo clippy -p cupper-host --all-targets --all-features` 和 `cargo clippy --workspace --all-targets` 没有警告。
  - `cargo xtask ci testdata-scan` 通过。
- **规模**：`cargo xtask size` 为 **+556 −29**，在放宽后的停止线 580 以内。分支上的 brief 还写着 450/550，工具会按旧数字提示越线，可以忽略。
- **验收标准**：IT-git-file-diff 通过；协议没有改动，所以 `schema-compat` 是 0 breaking；GitHub CI 的 8 项结果还没出来。

## 提交列表

```
973a51f feat(git): git/file_diff, its hunk units and DiffCache
```

## 遗留问题 / 范围外发现

- 给 T121：暂存时按 `diff_token` 取出 `Diffed`，用 `diff::patch` 重新生成文件的补丁，再用 `diff::units` 按 `hunk_id` 找到单位。缓存目前不存 `hunk_id` 到路径的索引：目标是 `{hunk_ids}` 时，要么由 T121 加这个索引，要么逐个文件重新生成补丁去找。
- T120 汇报里列的范围外发现照旧，其中第 3 条（`git/diff` 的文件列表没有 blob 兜底）不在本任务处理。
