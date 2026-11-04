# T237 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`fix/T237-lfs-repo-config`
- 完成日期：2026-11-04

## 做了什么

1. **审查**：用锁定的 git-lfs 3.8.0 逐项实测，配置键从 release 自带的 `git-lfs-config(5)` 里取，结果表写在 `docs/tasks/T237-lfs-repo-config/notes.md`。结论：
   - **会执行程序的键**：只在 git 配置里生效，写进 `.lfsconfig` 的都被 git-lfs 当作不安全的键忽略。
     - `lfs.extension.<名>.clean`：每次 clean 都执行。`.smudge`：smudge 写着该扩展的指针时执行。
     - `lfs.customtransfer.<名>.path` 配合 `lfs.standalonetransferagent`：下载时直接执行，不经过服务器；只有 customtransfer 时，要服务器在 batch 响应里选中才执行。
     - ssh 端点：core.sshCommand 被置空后，git-lfs 用 PATH 上的 ssh 连出去（带 ControlMaster）。
     - `credential.helper`、`core.askpass`：仓库档本来就置空，实测确实不执行。
     - `-c lfs.extension.x.clean=` 置空没用，git-lfs 会直接失败。
   - **新发现：git-lfs 往 cupper 的 empty-hooks 里装钩子**。作为过滤器运行时，它把 pre-push、post-checkout、post-commit、post-merge 四个钩子装到 core.hooksPath 指向的目录，也就是 `<数据目录>/git/empty-hooks/`。快照档和仓库档都会，实测确认。之后仓库档的 worktree add 会执行 post-checkout：PATH 上有 git-lfs 就运行 `git lfs post-checkout`，没有就退出码 2，worktree add 失败，worktree 留在磁盘上。
   - **联网**：只在 smudge 一个本地没有的对象时发生。对象在本地时，不管端点、改写还是代理怎么配，都没有请求；clean 从不联网。
     - 端点可以来自 `.lfsconfig`（lfs.url、remote.*.lfsurl、lfs.gitprotocol）和 git 配置（lfs.url、remote url、url.insteadOf、http.proxy、ssh）。
     - 改写端点堵不住：insteadOf 能改写我们设的任何 URL；没有 scheme 的值会被当成 ssh 主机，实测经本机代理真的连出去了。
     - `GIT_LFS_SKIP_SMUDGE` 和 `lfs.fetchexclude=*` 也不是"离线 smudge"：对象在本地时同样写指针。
   - **悄悄写错内容**：`.lfsconfig` 里的 `fetchexclude`、`fetchinclude`、`skipdownloaderrors` 会让 smudge 把指针文本写进工作区，退出码还是 0。
   - **快照档**：用白名单配置，读不到 extension、customtransfer 类的键；只做 clean，不下载。受影响的只有钩子那一条。

2. **修复**（全部按你定的方案）：
   - 钩子：empty-hooks 建成只读（0500）。宿主每次启动先恢复写权限，清空目录，再设回只读（`git/mod.rs`）。
   - 方案二：配置里出现 `lfs.extension.*`、`lfs.customtransfer.*`、`lfs.standalonetransferagent` 时，这个仓库的 LFS 按非标准处理，两档都不运行宿主的 git-lfs，复用 filter_unsupported 和 RequiredFilter 路径。判定是 `rules.rs` 的 `lfs_runs_programs`，附了原因；快照档的 `runs_lfs` 也调用它。
   - 离线覆盖：仓库档每次调用都加 `-c lfs.fetchinclude= -c lfs.fetchexclude= -c lfs.skipdownloaderrors=false`。
   - 宿主从不 smudge 本地没有的对象：
     - revert：在 T235 的复制之后，补丁 `-` 侧的对象两边存储都没有，就在 git 运行前拒绝，报 `git_error`/`lfs_object_missing`。
     - worktree add：检出之前，用 `ls-tree -rz --format='%(objectsize) %(objectname)'` 加 `cat-file --batch` 找出基准树（或已有分支）里的指针。识别方式和 git-lfs 一样：小于 1024 字节、能解析成指针的 blob。最低支持的 git 2.39 没有 `check-attr --source`，所以不按属性筛。缺对象就写 `workspace.failed{git_error, lfs_object_missing}`，不建一半。
     - 两处都只在"宿主在这个仓库运行 LFS"时检查（`Stores::runs`）。
   - 新增协议原因码 `ErrorReason::LfsObjectMissing`（`lfs_object_missing`，归 git_error），git/revert 的 errors 里加了它，已 gen，catalog 单元测试一起改了。
   - 错误映射：git apply 退出码 1（补丁应用不上）报 stale_diff；其他（128：过滤器失败、坏输入）报 git_error，带脱敏后的 stderr。index.lock 那条照旧。
   - **顺带发现并修复**：经仓库档 `config --list` 读到的配置里，混着仓库档自己加的 `-c` 覆盖，其中标准 LFS 过滤器的值已经被改写成宿主的 git-lfs，导致"宿主是否运行 LFS"判成否，预检被跳过。`Stores::of` 现在滤掉来源是 command line 的条目。

3. **用例**：
   - **IT-git-lfs-repo-config**（`lfs_tests.rs`，一个用例，联网和程序两段的断言消息分别带 `network:` 和 `programs:` 前缀，对照的带 `control:`）：
     - 联网段：提交的 `.lfsconfig` 指向只绑 127.0.0.1、随机端口的本地端点，并让 `b.bin` 不参与 smudge。
       - 对照：用户自己的 git 把 `b.bin` 写成指针，并向端点请求缺失的对象。
       - 宿主：revert `b.bin` 得到正确内容；revert `a.bin`（对象缺失）和 worktree add 都报 `lfs_object_missing`；没有留下 worktree；端点没有收到宿主的请求。端点在用例结束时关闭。
     - 程序段：配上 extension 和 standalone transfer agent。
       - 对照：用户的 git 两个都执行了。
       - 宿主：快照档的 diff 显示 filter_unsupported，git/status 正常，worktree add 报 filter_unsupported，两个程序都没执行，empty-hooks 仍然是空的。
   - **UNIT-git-lfs-store**（`git/lfs.rs`）：
     - `lfs.storage`：默认值、相对路径、绝对路径、`~` 不展开、空值、多个值时取最后一个；
     - `core.sharedRepository`：裸键、group、true、1、all、world、everybody、2、0640、0777、0440、umask、false、0。
   - 已有用例的调整：rules 单元测试（离线覆盖、程序类键）、快照配置单元测试；`Case` 加了 `try_git`；用户 git 的 PATH 补上 `/usr/bin` 和 `/bin`（git-lfs 经 sh 启动 transfer agent，需要它们）。

## 关键取舍

- 联网问题靠预检解决，不改写端点：端点改写挡不住 insteadOf 和代理。预检之后宿主只 smudge 本地已有的对象，这种情况下实测不联网，也不运行 transfer agent。
- worktree 预检按内容识别指针，不按属性：2.39 没有 `check-attr --source`；而且 `git lfs fetch` 也是按内容找指针的，和拒绝时给出的提示一致。
- 规模：钩子和 worktree 预检是审查后新增的范围。停止线由你放宽到 350，硬停止线放到 500。为了压行数：
  - 两个 IT 合成一个；
  - `ls-tree` 改用 `--format`；
  - supply 保持为自由函数，避免整段缩进变动。

## 验证方式

- **修复前的失败**：新用例跑在 main（317ca1e）的产品代码上，共五条：
  1. revert `b.bin` 把指针文本写进了工作区：`left: "version https://git-lfs.github.com/spec/v1\noid sha256:27dd8e…\nsize 4\n"`，right 应为 `"two\n"`（.lfsconfig 的 fetchexclude 生效）。
  2. 对象缺失时 revert 报 `conflict/stale_diff "the patch does not apply"`，真实原因是 smudge 去下载失败。
  3. worktree add 报 `workspace.failed{git_error}`，没有原因码，假端点多收到 3 次请求（`the host went to the endpoint: left 5, right 2`）。
  4. worktree add 执行了仓库配置的 transfer agent 16 次（`left: "agent\n" ×16, right: ""`）。
  5. 快照档的 diff 是指针 diff，不是 filter_unsupported（`left: Null, right: true`）。
- **command line 过滤的守护**：删掉 `Stores::of` 里的 `retain` 那一行，IT 在 `network: revert` 失败（left `"git_error "`，right `"git_error lfs_object_missing"`）。
- **本机检查**（单份）：
  - host lib 测试 516 个通过、3 个忽略；
  - cupper-protocol 测试 100 个通过；
  - packages/protocol 的 vitest 168 个通过；
  - crash_concurrency 里 git 相关的 3 个用例通过；
  - fmt、clippy（host 带 test-harness，加 protocol，all-targets，`-D warnings`）、trace-ids（449 个编号）都通过。
- **验收标准**：
  - [x] IT-git-lfs-repo-config、UNIT-git-lfs-store 通过。runner 上 10 次 × 4 份，macos-15：
    - `it_git_lfs_repo_config`：https://github.com/boris-w/cupper/actions/runs/41788316700 ，40 passed, 0 failed。Build 打印了 `target/git-lfs/git-lfs`；Run 用时 2 分 01 秒，每份每次约 12 秒，是真跑。
    - `it_git_lfs_stage_objects`（T235）：https://github.com/boris-w/cupper/actions/runs/41788320393 ，40 passed, 0 failed。同样打印了 git-lfs；Run 用时 39 秒，每份每次约 4 秒。
  - [x] 汇报里写明了哪些键会执行程序、哪些会联网、各自怎么处理（见"做了什么"第 1、2 条）。
  - [x] PR #74 https://github.com/boris-w/cupper/pull/74 的 8 个检查都通过，运行在 https://github.com/boris-w/cupper/actions/runs/41788238829 。rust 作业里 `git-lfs ok (1.8s)` 排在 `test ok (447.9s)` 之前，所以 LFS 用例是真跑的。
- **规模**：`cargo xtask size` 为 +500 −82，正好在硬停止线 500。

## 提交列表

```
ad6e101a test(git): IT-git-lfs-repo-config and UNIT-git-lfs-store
6fde03e4 fix(git): check a worktree's LFS objects before its checkout
71c1d8ce fix(git): stale_diff only when git apply refuses the patch
223538ec fix(git): refuse a revert that would download an LFS object
7b2732d1 Merge main into fix/T237-lfs-repo-config
c354fb18 docs(T237): what git-lfs runs and where it downloads from
85d03980 feat(protocol): the lfs_object_missing error reason
e937ec74 fix(git): run no git-lfs that would run programs, and smudge content
9048ce5d fix(git): keep git-lfs out of the empty hooks directory
```

## 设计偏差 / 补充

5.2.10，以及 5.8.5 里 git 那几行：
- **空钩子目录**：只读（0500），宿主每次启动先清空。原因是 git-lfs 会往 core.hooksPath 里装自己的钩子。
- **仓库档规则加一条**：配置里有 `lfs.extension.*`、`lfs.customtransfer.*`、`lfs.standalonetransferagent` 时，宿主不运行 git-lfs，LFS 按不支持的过滤器处理，两档都是。这些键 git-lfs 不会从 `.lfsconfig` 读。
- **离线覆盖**：仓库档每次调用都加 `-c lfs.fetchinclude= -c lfs.fetchexclude= -c lfs.skipdownloaderrors=false`。
- **宿主从不 smudge 本地没有的对象**：revert 和 worktree add 之前先预检，缺对象就报 `git_error`/`lfs_object_missing`；worktree add 写 `workspace.failed`，不建一半。worktree 的指针按 git-lfs 的方式按内容识别。
- **apply 的错误映射**：退出码 1 报 `stale_diff`，128 及其他报 `git_error` 加脱敏后的 stderr。
- **Stores 滤掉 command line 来源**：判断仓库的 LFS 配置时（是否运行 LFS、`lfs.storage`、`core.sharedRepository`），不算仓库档自己加的 `-c` 覆盖。这一点由 IT 的 `network: revert` 断言守着。
- **03 协议文档**：新增 `lfs_object_missing`（git_error），git/revert 的 errors 里加上它。你说过合并时改。

## 遗留问题 / 范围外发现

- **界面提示**：界面要按 `lfs_object_missing` 提示"在终端里运行 `git lfs fetch`"，这是桌面端的事，本任务没做。
- **alternates**：预检只看用户存储和 cupper 的私有存储，没看 alternates 对应的 lfs 目录（git-lfs 的 LocalReferenceDirs）。用 alternates 共享 LFS 对象的仓库可能被误拒，比较少见。
- **超大仓库**：worktree 预检要列整棵树。超大仓库的 `ls-tree` 输出可能超过 64 MB 的 stdout 上限，这时 worktree add 会失败，报 git_error。
- **.lfsconfig 导致 clean 失败**：`.lfsconfig` 里写 `lfs.extension.*` 时 git-lfs 不会执行，但会让 clean 失败，报 "executable file not found"。agent 提交这样一个文件，就能让快照档的 clean 失败（快照记为 git_failed）。只是让功能失效，不会执行程序；没有处理。
- **退出码 128 的 revert**：git 可能已经写了一部分工作区文件；现在按"没有改动"处理，ledger 会 reject，这是原有行为。有了预检，这种情况少了很多。
- **用户档**：commit、push 照设计使用用户自己的配置和钩子，git-lfs 在那里联网是用户自己的操作，不在本任务范围。
