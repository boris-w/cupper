# T235 汇报

T235 已完成，PR #67 的 8 个 CI 检查全部通过。这份汇报已用 SendMessage 发给 main 存档，这里再交一次。

- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`fix/T235-lfs-stage-objects`
- 完成日期：2026-11-04

## 做了什么

1. **复现**：在 T229 的 LFS 写操作用例上做了三步：仓库档暂存 a.bin（`one` 改成 `two`），工作区再改成 `three`，然后用户自己提交。用户仓库跑 `git lfs fsck --objects` 报 `a.bin (27dd8ed4…) could not be checked: no such file or directory`，退出码 1。这个对象只存在于 cupper 的私有存储 `<数据目录>/git/<仓库>/ws/<工作区>/lfs/objects/`。

2. **修复**：改动在 `crates/cupper-host/src/git/lfs.rs`（新文件）、`service.rs`、`snapshot.rs`。
   - **时机**：`git/stage`、`git/unstage`、`git/revert` 在 `RepoLock` 之后、`mark_external` 之前调用 `objects()`。
   - **取哪些 oid**：从补丁里取这次要写入的那一侧的指针 oid。stage 取 `+oid sha256:` 行；unstage 和 revert 是反向应用，取 `-` 行。
   - **复制**：经仓库档 `config --list` 读配置，来源检查照旧。用户存储里没有、私有存储里有的对象，先复制到 `<storage>/tmp/<oid>.<唯一名>`，sha256 与 oid 对上后 sync，再改名到 `objects/xx/yy/<oid>`。私有存储里也没有的，留给用户自己的 git-lfs。
   - **存储位置**：按 git-lfs 的规则取。`lfs.storage` 只认 git 配置，相对路径相对公共目录；默认是 `<公共目录>/lfs`，worktree 共用。
   - **权限**：有 `core.sharedRepository` 时按 git 的规则：
     - `group`、`true`、`1` 为 660；
     - `all`、`world`、`everybody`、`2` 为 664；
     - 八进制值取 `& 0666`，且必须包含 0600；
     - `umask`、`false`、`0` 不设。
     
     目录在每个 r 位上加 x。没有这个配置时按 umask。
   - **失败**：复制失败或 sha256 对不上，返回 git_error，不执行 apply，ledger 照常 reject。
   - **辅助函数**：新增 `snapshot::private()` 算私有 GIT_DIR；`unique()` 改成 `pub(super)`。

3. **用例 IT-git-lfs-stage-objects**（`orchestrator/session/lfs_tests.rs`）：走会话夹具，用锁定的 git-lfs，没有 git-lfs 时跳过。夹具加了 `Case::with_lfs` 和 `git_project_with`。
   - **暂存**：仓库设 `core.sharedRepository=group`。暂存后用户存储里有对象，内容正确，文件 100660，目录 40770。改文件后用户提交，`git lfs fsck --objects` 通过。
   - **还原**：回合 2 开始时的内容只在私有存储里，用例先断言用户存储里没有它。revert `last_turn` 成功，工作区回到 `start`，对象也进了用户存储。

4. **notes.md**：写了复现过程、存储位置实测表、修法和取舍、修复前的输出，以及"For T237"一节。

## 关键取舍

- **选 A**：从私有存储复制，不选 B（在仓库档用宿主的 git-lfs 跑 `hash-object -w`）。B 每个对象要多跑三四个进程，git 会把整个文件读进内存，还会执行 `lfs.extension.*` 的程序。A 不运行任何程序，大文件流式复制，并按 oid 校验。
- **读配置**：经 `RepoTier::run(ConfigList)` 读，不另抽函数，避免挪动代码让行数翻倍。
- **校验对象**：先 `fs::copy` 到临时文件，再对临时文件算哈希，校验的就是最终放进去的那份字节。

## 验证方式

- **修复前的失败**（跳过复制后跑；后两条各自去掉了前面的断言）：
  1. 暂存后：`left: None, right: Some(("two\n", "100660", "40770"))`。
  2. 用户提交后，`git lfs fsck --objects` 退出码非零。
  3. revert 被报成 `conflict stale_diff "the patch does not apply"`。git 的 stderr 是 `Downloading a.bin (6 B)` / `Error downloading object: a.bin (46210dd): Smudge error: … batch request: missing protocol: ""` / `fatal: a.bin: smudge filter lfs failed`，说明确实去下载了，只是测试仓库没有远端才失败。
- **本机**（单份）：host lib 测试 508 个通过、3 个忽略；fmt、clippy（`-p cupper-host --features test-harness --all-targets -D warnings`）、trace-ids（447 个编号）都通过。
- **对照验收标准**：
  - [x] IT-git-lfs-stage-objects 通过，修复前失败（输出见上）。
  - [x] runner 上 10 次（10 × 4 份，macos-15）：
    - 第一次 https://github.com/boris-w/cupper/actions/runs/41771238835 作废。downloads 缓存虽然命中，但 Run 步骤 1 秒跑完 40 次，用例实际被跳过了。
    - 重跑 https://github.com/boris-w/cupper/actions/runs/41772123987 ，在临时分支 `test/T235-repeat` 上（9f2cfb71 加上 PR #69 的 `cargo xtask git-lfs`），40 passed, 0 failed。Build 里执行了 `cargo xtask git-lfs`，打印了 `target/git-lfs/git-lfs`；Run 步骤约 51 秒，每份每次约 5 秒，用例是真跑的。
  - [x] PR #67 https://github.com/boris-w/cupper/pull/67 的 8 个检查全部通过，运行 https://github.com/boris-w/cupper/actions/runs/41771240738 。rust 作业里 `git-lfs ok (1.2s)` 在 `test ok (379.1s)` 之前，所以 LFS 用例是真跑的，不是跳过。这一点是按步骤顺序推断的：日志不打印通过的测试名。
- **规模**：+294 −8，在硬停止线 300 以内，你已按 (a) 接受。产品代码 194 行，用例和夹具 108 行。

## 提交列表

```
9f2cfb71 test(git): IT-git-lfs-stage-objects
e5911fe9 fix(git): put the LFS objects of written pointers in the user's store
```

## 设计偏差 / 补充

- **5.2.10 仓库档写操作**：补一条规则：写之前，把新指针引用的 LFS 对象从私有存储放进用户的 LFS 存储。要写清存储位置、临时文件加改名、oid 校验、`core.sharedRepository` 权限，以及失败时返回 git_error、不执行 apply。
- **08-security 可能要补一句**：这是宿主不经 git、直接写用户仓库 `.git` 下文件的一条新路径。

## 遗留问题 / 范围外发现

- **交给 T237**（notes 里有一节）：
  - `lfs.extension.*` 的程序会被宿主的 git-lfs 执行；
  - smudge 找不到对象时会联网：T235 只补了 cupper 自己写入的指针，其他缺失的对象照旧会联网；
  - smudge 失败被报成 `stale_diff`，掩盖了真实原因。
- **CI 缓存**：其他用到 setup 的作业里，`target/git-lfs` 可能是空的。test-repeat 已由你在 PR #69 修复，这条你也记进了交接事项。
- **用例覆盖**：IT 只覆盖了默认存储位置和 `group`。`lfs.storage` 的相对、绝对路径，以及 `all`、八进制这些取值，只用 `git lfs env` 实测核对过规则，没有单独的用例；要补的话可以另开 UNIT 编号，约 30 行。
- **非法取值**：`core.sharedRepository` 是非法八进制值时，git 自己会拒绝运行；cupper 这时按 umask 处理。
