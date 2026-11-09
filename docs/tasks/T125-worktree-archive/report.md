# T125 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`feat/T125-worktree-archive`
- 完成日期：2026-11-04
- PR：https://github.com/boris-w/cupper/pull/73
- CI 运行：https://github.com/boris-w/cupper/actions/runs/41788492714 。`4726705c` 上 8 个检查全部通过。
- test-repeat（`it_worktree_check_rules`，`--lib`，10 次）两次都通过：
  - `c2b5e7eb`：https://github.com/boris-w/cupper/actions/runs/41788194993
  - `4726705c`：https://github.com/boris-w/cupper/actions/runs/41788569120

## 做了什么

范围按拆分方案 A，只做 `worktree/check` 和 IT-worktree-check-rules。archive、restore、rename、list_archived、project/remove 归 T240。

1. **`git/cleanup.rs` 的 `check`**：只读，逐条检查 ADR-10 的 7 条清理规则，返回 `WorktreeCheckResult{removable, blockers, warnings}`。blockers 按规则的顺序排列。T240 的 `session/archive` 可以直接复用它。
   - **第 1、2 条**：在快照档的 index 副本上复用 `status::states`（改为 `pub(super)`）。修改或暂存的文件算第 1 条，未跟踪的文件算第 2 条。
   - **仅大小写冲突**：脏文件都未暂存，并且每个路径在已跟踪路径（快照档 `LsFiles`）里都有另一条只差大小写时，不算第 1 条，记为 `warnings.case_only_dirty`。
   - **被忽略文件**：快照档新增 `SnapshotOp::Ignored`（`ls-files -z --others --ignored --exclude-standard`），只计数，放进 `warnings.ignored_files`。
   - **第 3 条**：复用 `status::in_progress(git_dir)`（T222 的交接）。
   - **第 7 条**：git 目录下的 `modules/` 不为空就算。
   - **第 4 条**：用 T236 的 `listed` 解析 `worktree list --porcelain -z`。`listed` 改为返回锁的原因，没有原因的锁记为空串；原因不等于 `cupper session <id>` 的锁算外来锁。`Worktrees::add` 的调用随之调整。
   - **第 5 条**：`rev-list --count HEAD --not --exclude=<本分支> --branches --remotes --tags`。
   - **第 6 条**：有远程跟踪分支时执行 `rev-list --count HEAD --not --remotes`，没有就跳过。HEAD 还没有提交时，第 5、6 条都不成立。
2. **`GitService` 提供 `worktree/check`**：
   - 本地会话回 `unsupported`（你的决定 2）。
   - 快照档的错误照 `shot` 映射；仓库档的错误照 `reported` 写审计和 notice。
   - 加了路由，删掉了 dispatch 用例里那行 `Later(M1b)`。
3. **用例 IT-worktree-check-rules**：`it_worktree_check_rules_keep_a_worktree_with_work_in_it`，见"验证方式"。

## 关键取舍

- **"仓库没有远程"以远程跟踪分支为准**（你已同意）。从没 fetch 或 push 过的远程等同没有远程。这不会丢数据：只在这个 worktree 里的提交，第 5 条照样拦住。这样也不用为了读远程配置去解析 `config --list`。
- **大小写冲突的判断**：只看"已跟踪路径里只差大小写"，不处理 Unicode 规范化（NFC 和 NFD 写法不同的同名路径）。
- **快照档与仓库档的分工**：读文件状态和 `ls-files` 都走快照档，仓库档只跑 `rev-list` 和 `worktree list`，与 5.2.10 的三档表一致。
- **用例写成一张文本表**：每行一段操作和一段撤销，用小解释器执行。避免 rustfmt 展开，也方便以后加行。

## 验证方式

本机（单份运行）：
- `cargo test --workspace --all-features`：921 个通过，0 失败。
- clippy，带和不带 `--all-features` 两种，都开 `-D warnings`。
- `cargo fmt --all --check`。
- `cargo xtask ci protocol`：通过，协议没有改动。
- `cargo xtask ci testdata-scan`：通过。

对照验收标准：
1. **用例通过，并在 runner 上连跑 10 次**：两次 test-repeat 都通过，链接见开头。
   - 用例在一个 worktree 会话上逐行执行：先做改动，调用 `worktree/check` 核对 blockers、被忽略文件数和 `case_only_dirty`，再撤销。每行都另外核对 `removable` 与"blockers 为空"一致。
   - 11 行依次是：干净；修改；暂存；未跟踪；被忽略（经 `info/exclude`）；进行中的 bisect；外来的锁；只在 HEAD 上的提交；子模块的 git 目录；仅大小写冲突；未推送（本地裸仓库作远程，另建一条分支包住这次提交，所以只触发第 6 条）。
   - 7 条规则各一行，修改和暂存同属第 1 条，另有被忽略、仅大小写冲突两种只提示的情形。
   - 最后断言本地会话回 `unsupported`。
   - 变异检查，以下两处都会让用例失败：不判大小写冲突；第 5 条不排除本分支。
2. **没有 worktree 建在临时目录之外**：worktree 根目录是夹具数据目录下的默认值。远程裸仓库建在 worktree 根目录下，也在临时目录里。
3. **`schema-compat` 0 breaking**：协议没有改动。
4. **8 个检查都通过**：链接见开头。

**大小写的探测**（你的意见）：用例开头在 worktree 里建 `Case-Probe`，再看 `case-probe` 能否打开，以此判断文件系统是否区分大小写。
- 不区分时，那一行断言 `case_only_dirty`。
- 区分时，改为断言 worktree 干净，并在 stderr 打印原因。
- **区分大小写的那条分支没有实测**：本机和 CI 的 macOS runner 都是不区分大小写的 APFS。这条分支的期望是按 git 的行为推的：两个文件各自写出，worktree 是干净的。

## 设计偏差 / 补充

1. **5.2.10 第 6 条**：以有没有远程跟踪分支判断"仓库有没有远程"。
2. **5.2.10 快照档的命令**：新增 `ls-files --others --ignored --exclude-standard`，只用来统计被忽略的文件。
3. **5.2.10 仓库档**：`worktree list` 的解析结果改为带锁的原因。外来锁指原因不是 `cupper session <id>` 的锁，没有原因的锁也算外来锁。
4. **仅大小写冲突的定义**：脏文件都未暂存，并且每个路径在已跟踪路径里都有只差大小写的另一条。
5. **本地会话**：`worktree/check` 回 `unsupported`（你的决定 2）。

## 提交列表

```
4726705c test(host): probe the file system's case in IT-worktree-check-rules
c2b5e7eb feat(host): worktree/check by ADR-10's cleanup rules
```

## 遗留问题 / 范围外发现

- **规模**：+266 −10，在停止线 350 以内。
- **给 T240**：
  - `session/archive` 可以直接调用 `cleanup::check` 得到 blockers。
  - 删除前要先 `worktree unlock`，因为加了锁的 worktree 不加 `-f -f` 删不掉。
  - `remove_case_only_dirty` 只在 `case_only_dirty` 为 true、并且没有其他 blocker 时才加 `--force`。
  - 恢复 not_found 会话时，检查"分支是否在别的 worktree 里检出"可以复用 `listed` 的记录解析。
- **大小写冲突**：没有处理 Unicode 规范化，NFC 和 NFD 写法不同的同名路径不会被认作冲突。

worktree 请保留，T240 接着用。
