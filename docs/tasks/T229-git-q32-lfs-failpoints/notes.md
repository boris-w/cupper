# T229 工作笔记

## 开工前的估计

停止线 250、硬停止线 300。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 故障点 | `failpoint.rs` 登记 `git.before_apply`（`mark_external` 之后、`apply` 之前）和 `git.after_apply`（`apply` 之后、结果记账之前），写路径上各标一处 | 12 | — |
| IT-NFR9-failpoints-git | T069 夹具上的场景：git 仓库做项目，会话一回合，`git/diff` 取 token，`git/stage` 撞上故障点；重启后完整性、seq 连续、历史前缀，按原 `command_id` 重试得到 `outcome_unknown`，仓库的 index 与故障点相符 | — | 90 |
| Q32 | T098 的恶意夹具上跑 stage、unstage、revert 和暂存前的 `check-attr`，标记目录为空 | — | 35 |
| LFS 写操作 | 锁定版本的 git-lfs，LFS 文件的 stage、unstage、revert；没有 git-lfs 时跳过 | — | 45 |
| `RepoLock` | 同一仓库（含它的 worktree）互斥，不同仓库不互斥 | — | 30 |
| 小计 | | 12 | 200 |

## 结论

- **IT-NFR9-failpoints-git**：`failpoint.rs` 登记了 `git.before_apply`（`mark_external` 之后、`apply` 之前）和 `git.after_apply`（`apply` 之后、结果记账之前），写路径上各标一处。T069 夹具上的场景：git 仓库做项目，会话跑一回合，`git/diff` 取 unstaged 的 token，`git/stage` 撞上故障点、宿主以 99 退出；重启后 `consistent()`（`integrity_check`、seq 连续、实时看到的是日志前缀），按原 `command_id` 重试得到 `conflict/outcome_unknown`，index 里有没有这处改动与故障点相符（apply 之前没有，之后有）。场景里只有一次写，所以每个故障点只在第 1 次命中时触发。
- **Q32（仓库档写操作）**：T098 的恶意夹具上，暂存前的 `check-attr`、`apply --cached`、`apply --cached -R`、`apply -R` 都照常完成，`markers` 为空，`.gitattributes` 把每个文件都归到 `filter=evil diff=evil merge=evil`。对照：同样的补丁用用户自己的 git 和配置执行，会跑 clean、smudge 过滤器和 `post-index-change` 钩子（本机实测三个标记都出现），所以用例确实证明了覆盖有效。
- **LFS 写操作**（锁定的 git-lfs 3.8.0，`cargo xtask git-lfs`；没有时跳过）：unstaged 的补丁是指针对指针的补丁，`apply --cached` 写进 index 的 blob 与用户 `git hash-object --path=a.bin` 得到的指针相同；取消暂存后 index 恢复；还原时仓库档运行宿主的 git-lfs 做 smudge，工作区回到 "one"，`status` 干净。另外看到：暂存后，新内容的 LFS 对象只在 cupper 私有的 GIT_DIR 里（`<数据目录>/git/<仓库>/ws/<工作区>/lfs/objects/`），用户仓库的 `.git/lfs/objects` 里还没有；用户随后 `git commit` 时 git-lfs 的 clean 把它写进用户仓库，`git lfs fsck --objects` 通过。不算缺陷，记在这里。
- **`RepoLock`**：同一仓库的工作区和它的 worktree 共用一把锁（按 common dir），另一个仓库不等。用例 `repo_lock_one_write_at_a_time_per_repository`。

## 范围外发现：同一秒内同大小的改动在 unstaged 视图里看不到（T234）

现象：用户仓库 commit 之后，同一秒内把文件改成同样大小的内容（a.txt 从 "a\n" 改为 "b\n"），进程级宿主的 `git/diff`（unstaged）返回空，`git/stage` 报 "nothing to apply"。用户自己的 `git diff --name-only` 列出 a.txt。

三组对照（进程级宿主，`login_env.PATH` 为 `/usr/bin:/bin`）：同大小、同一秒，漏掉；改成不同大小（"bb\n"），看得到；同大小但 commit 后先 `sleep 2` 再改，看得到。另外：宿主的 git 换成 Homebrew 的 2.54.0，照样漏掉；会话夹具（进程内，`diff_tests.rs` 的 `git_project`）里同样的步骤却看得到，所以漏点可能与完整宿主里另外跑的东西（例如索引对用户仓库的刷新）或经过的时间有关。

复现草稿（放进 `tests/crash_concurrency.rs`，用 T229 的 `git_sh`；断言目前失败）：

```rust
#[tokio::test(flavor = "multi_thread")]
async fn t234_a_same_size_change_within_the_commits_second() {
    let dir = TempDir::new("t234");
    let (data_dir, app) = (dir.path().join("data"), dir.path().join("app"));
    git_sh(
        dir.path(),
        "git init -q -b main app; cd app; printf 'a\\n' > a.txt; git add a.txt
        git -c user.name=t -c user.email=t@example.invalid commit -qm init; printf 'b\\n' > a.txt",
    );
    let config = json!({"login_env": {"PATH": "/usr/bin:/bin"}});
    let host = Running(spawn_with(&data_dir, fake_agent_config(config)));
    let client = connect(&data_dir).await;
    let add = json!({"command_id": client::command_id(), "path": app});
    let project = request(&client, ProjectAdd::NAME, add).await.unwrap();
    let session = start(&client, &project["project_id"], FIRST).await;
    let diff = json!({"session_id": session, "scope": "unstaged"});
    let diff = request(&client, GitDiff::NAME, diff).await.unwrap();
    assert_eq!(git_sh(&app, "git diff --name-only"), "a.txt");
    assert_eq!(diff["files"][0]["path"], "a.txt", "{diff}");
    terminate(&host.0);
}
```
