# T229 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`test/T229-git-q32-lfs-failpoints`（头提交 `1ec66ec`，基于 main `a294024`）
- 完成日期：2026-11-04
- PR：https://github.com/boris-w/cupper/pull/61 。`1ec66ec` 上 8 个检查全部通过：https://github.com/boris-w/cupper/actions/runs/41760003286 。rust 作业先跑了 `cargo xtask git-lfs`，日志里 LFS 用例显示 ok、没有 "skipped"，说明它在 CI 上真正跑了。
- 连跑（计划 1.2，新增的 IT 连跑 10 次）：https://github.com/boris-w/cupper/actions/runs/41760007751 。参数 kind=cargo、package=cupper-host、args=`--features test-harness --test crash_concurrency`、filter=`it_nfr9_failpoints_git`、times=10、parallel=4、runner=macos-15，40 次全部通过。

## 做了什么

1. **IT-NFR9-failpoints-git**（0f069d5）
   - 在 `failpoint.rs` 登记两个故障点：`git.before_apply` 在 `mark_external` 之后、`apply` 之前；`git.after_apply` 在 `apply` 之后、结果记账之前。`git/service.rs` 的写路径上各标一处，共 2 行。
   - 用例放在 T069 的夹具上（`tests/crash_concurrency.rs`，`crash_in_git`）。场景：git 仓库做项目，会话跑一回合，`git/diff` 取 unstaged 的 token，`git/stage` 撞上故障点，宿主以 99 退出。
   - 重启后检查：`consistent()`（`integrity_check`、各流 seq 连续、崩溃前客户端实时看到的事件是日志的前缀）；按原 `command_id` 重试得到 `conflict/outcome_unknown`；用户仓库 index 里有没有这处改动，与故障点相符（apply 之前没有，之后有）。
   - 场景里只有一次写，所以每个故障点在第 1 次命中时触发，没有做倍增扫描。
2. **Q32（仓库档写操作）**：在 T098 的恶意夹具上补了 `it_git_repo_tier_malicious_writes_run_no_program_of_the_repository`。暂存前的 `check-attr` 正确报出 `filter evil`，`apply --cached`、`apply --cached -R`、`apply -R` 都照常完成，工作区内容正确，`markers` 目录为空。
3. **LFS 写操作**：`it_git_lfs_writes_stage_unstage_and_revert`，用锁定的 git-lfs 3.8.0（`cargo xtask git-lfs`），没有 git-lfs 时跳过。unstaged 视图的补丁是 LFS 指针对指针的补丁：
   - 暂存后，index 里的 blob 与用户 `git hash-object --path=a.bin` 得到的指针相同；
   - 取消暂存后 index 恢复；
   - 还原时，仓库档运行宿主的 git-lfs 做 smudge，工作区回到原内容，`status` 干净。
4. **`RepoLock`**（T121 交接）：`repo_lock_one_write_at_a_time_per_repository`。同一仓库的工作区和它的 worktree 按 common dir 共用一把锁；另一个仓库的写不等。为此 `RepoLock` 和 `hold` 改成 `pub(super)`。

## 结论

- **Q32 仓库档写操作：确认有效。** 置空覆盖对 stage、unstage、revert 和暂存前的 `check-attr` 都有效，没有任何程序被执行。对照：同样的补丁用用户自己的 git 和配置执行，clean、smudge 过滤器和 `post-index-change` 钩子都会跑，本机三个标记都出现了，所以这个用例确实能证明覆盖有效。
- **LFS 写操作：工作正常。** 附带看到：暂存后，新内容的 LFS 对象只在 cupper 私有的 GIT_DIR 里（`<数据目录>/git/<仓库>/ws/<工作区>/lfs/objects/`），用户仓库的 `.git/lfs/objects` 还没有；用户随后 `git commit` 时 git-lfs 的 clean 会把它写进用户仓库，`git lfs fsck --objects` 通过。不算缺陷。

## 关键取舍

- 故障点用例不复用 `check()`：`check()` 期望重发的命令都成功，而这里的重试应当得到 `outcome_unknown`。项目和会话的重发已由 core 的故障点覆盖，git 用例只断言写命令本身。
- Q32、LFS、`RepoLock` 用 git 服务现成的仓库档和快照档夹具，在库内测试里跑，不起宿主进程：要证明的是仓库档的调用本身。

## 验证方式

- 本机单份运行：`crash_concurrency` 的两个 git 故障点用例；`git::` 下的库内测试 31 个全部通过（含新增 3 个，LFS 用例用本机下载的 3.8.0 真正跑了）；clippy 带和不带 test-harness 都没有警告。
- 连跑和 PR 的 CI 结果见文首。

## 规模

增 199、删 5，停止线 250。产品代码只有登记故障点（5 行）、标记（2 行）和 `RepoLock` 的可见性（2 行），其余是用例。

## 提交列表

```
1ec66ec test(git): Q32 and LFS for the writes, and the repository lock
0f069d5 test(host): crash a git write before and after its apply
```

## 遗留问题 / 范围外发现

1. **unstaged 视图漏掉同一秒内同大小的改动（已开 T234）**
   - 现象：用户仓库 commit 之后，同一秒内把文件改成大小相同的新内容（a.txt 从 "a\n" 改为 "b\n"），进程级宿主的 `git/diff`（unstaged）返回空列表，`git/stage` 报 "nothing to apply"（`stale_diff`）。用户自己的 `git diff --name-only` 能列出 a.txt。
   - 复现步骤：`git init`，`commit` a.txt，紧接着做同大小的改动，然后起宿主（`login_env.PATH` 为 `/usr/bin:/bin`），加项目，建会话，取 `git/diff` unstaged。
   - 三组对照：
     - 同大小、同一秒：漏掉；
     - 改成不同大小（"bb\n"）：能看到；
     - 同大小，但 commit 后先 `sleep 2` 再改：能看到。
   - 另外两点：宿主的 git 换成 Homebrew 的 2.54.0，照样漏掉；进程内的会话夹具（`diff_tests.rs` 的 `git_project`）里同样的步骤却能看到。所以漏点可能与完整宿主里另外跑的东西（例如索引对用户仓库的刷新）或经过的时间有关。
   - 本任务的故障点用例改用不同大小的改动绕开它，用例旁的注释指向 T234。复现草稿在本任务的 `notes.md` 里。
2. worktree 保留着，等合并后再清理。

（说明：子任务的报告交接通道已在第一个任务时用掉，所以经 SendMessage 送达，请按此存档为 T229 的 report.md。）
