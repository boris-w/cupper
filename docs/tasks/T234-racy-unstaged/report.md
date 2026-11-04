# T234 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`fix/T234-racy-unstaged`
- 完成日期：2026-11-04

## 做了什么

1. **漏点查清并修复**（`crates/cupper-host/src/git/mod.rs`）：`SnapshotTier::prepare` 把 git 子进程的工作目录从 cupper 自己的 `<data>/git/` 改成 `place.work_tree`，也就是工作区顶层，并加了一行注释说明原因。
   - 漏的过程：`git/diff`（unstaged）先在 index 的副本上跑 `write-tree`，副本保留了用户 index 的 mtime。git 回写 index 时，会给"racily clean"的条目做 smudge（`ce_smudge_racily_clean_entry`），这一步按相对当前目录的路径 `lstat(ce->name)`。`write-tree` 不需要工作区，也不会 chdir 进工作区。所以在 `<data>/git/` 里，这个 lstat 失败，条目没有被 smudge。
   - 副本回写后时间戳变新，条目不再 racy。随后 `add -A` 看到 stat 一致，就沿用旧 blob，unstaged 视图为空，`git/stage` 因此报 `stale_diff`。
   - 用户自己的 index 没被宿主改写。用户的 `git diff` 能看到改动，是因为他的 index 文件仍带着 racy 的时间戳。
2. **条件 1（只能是顶层）**：
   - `place.work_tree` 来自 `Layout::read`，它要求 `<work_tree>/.git` 存在，所以只能是顶层；linked worktree 用 `.git` 文件，也就是它自己的顶层。
   - 单元用例 UNIT-git-cmd（`unit_git_cmd_tiers_pass_only_their_environment_and_fixed_arguments`）改用真实目录作工作区，断言假 git 记录到的 `pwd -P` 等于 `place.work_tree`。
3. **条件 2（工作区被删或不可访问）**：
   - 调用前就不存在：`open` 做 canonicalize 时就失败，映射为 `NotARepo`，和修复前一样。
   - 在 `open` 之后才消失：spawn 的 chdir 失败，得到 `RunError::Spawn`，再变成 `ShotError::Run`。之后各路径的处理：
     - RPC（diff、status）返回 `git_error`；
     - 回合快照记 `failed/git_failed`；
     - `files/search` 退回遍历目录。
   - 以上都不 panic，也不会变成 internal。
   - 原来没有"工作区被删"的用例。我在 UNIT-git-cmd 里加了一个断言：工作区不存在时，返回 `Err(RunError::Spawn(_))`。
4. **新增 IT-git-racy-unstaged**（`it_git_racy_unstaged_same_second_same_size`，放在 `crates/cupper-host/tests/crash_concurrency.rs`，模块文档同步更新）。
   - 跑的是进程级宿主，配 claude 形态的假 agent。
   - 同一秒不靠 sleep：用 `set_times` 把文件设成 index 里记录的那个 mtime（一个固定的过去整秒），index 文件也设成同一秒，再关掉测试仓库的 `core.trustctime`。
   - 两组：
     - 提交后：`a\n` 改成 `b\n`；
     - 暂存后：`git add` 之后，`b\n` 改成 `c\n`。
   - 两组都断言 unstaged diff 恰好是 `a.txt`，+1 −1。
5. **去掉 T229 的绕法**：`crash_in_git` 改回同大小的改动（`printf 'b\n'`），删掉指向 T234 的注释。
6. **`docs/tasks/T234-racy-unstaged/notes.md`**：写了漏点链路、证据、纯 git 的最小复现脚本（Apple Git 2.50.1 和 Homebrew git 2.54.0 结果一样）、为什么进程内夹具复现不了，以及修复前的输出。

## 关键取舍

- **修法**：按你定的 A，只改快照档的工作目录。另外考虑过两种做法：
  - `write-tree` 后把副本的 mtime 改回去：只治这一处，以后别的回写 index 的命令照样会漏；
  - 在快照档里先跑一次 `update-index --refresh`：多一次调用，也绕开了根因。

  顶层目录本来就是 `GIT_WORK_TREE`，pathspec 不变。
- **为什么关 ctime**：写文件和设 mtime 都会改 ctime。git 默认比较 ctime（只比到秒），ctime 一变，它不走 racy 检查就能直接看出改动，用例就测不到漏点。我在修复前、不关 ctime 的情况下实测：
  - 提交后那组照样通过：回合跑了一秒多，ctime 已经不在 index 记录的那一秒；
  - 暂存后那组失败，但失败只是因为写入恰好落在 `git add` 记下 ctime 的那一秒。

  也就是结果取决于时机。关掉以后，两组都只能靠 racy 检查发现改动，结果确定。快照档会按白名单从仓库配置里带上 `core.trustctime`，宿主看到的设置和用户的一致。

## 验证方式

- **修复前的失败输出**（本机，修复的提交撤掉、用例保留）：
  ```
  assertion `left == right` failed: after the commit
    left: []
   right: [[String("a.txt"), Number(1), Number(1)]]
  ```
  去掉第一个断言后再跑，暂存后那组的输出：
  ```
  assertion `left == right` failed: after staging
    left: []
   right: [[String("a.txt"), Number(1), Number(1)]]
  ```
- **修复后本机**（单份）：
  - `cargo test -p cupper-host --features test-harness --lib`：505 通过、3 忽略；
  - `--test crash_concurrency -- it_git_racy_unstaged it_nfr9_failpoints_git`：3 个用例通过；
  - `cargo fmt --check`、`cargo clippy -p cupper-host --features test-harness --all-targets -D warnings` 通过；
  - `node tools/trace-ids/src/main.ts check` 通过：447 个编号。新用例按最长编号匹配，归到 IT-git-racy-unstaged，不会算进 IT-git-racy。
- **对照验收标准**：
  - [x] IT-git-racy-unstaged 通过，修复前失败（输出见上）。
  - [x] runner 上 10 次全过（10 × 4 份并行，macos-15）：
    - `it_git_racy_unstaged`：40 passed, 0 failed，https://github.com/boris-w/cupper/actions/runs/41765125024
  - [x] T229 的绕法已去掉，IT-NFR9-failpoints-git 照常通过：
    - `it_nfr9_failpoints_git`：40 passed, 0 failed，https://github.com/boris-w/cupper/actions/runs/41765128680
  - [x] PR #64 https://github.com/boris-w/cupper/pull/64 的 8 个检查全部通过，ci 运行 https://github.com/boris-w/cupper/actions/runs/41765222613：rust、e2e、ui、compat-framework（macos-15），protocol、ts、license、testdata-scan（ubuntu-24.04）。
- **规模**：`cargo xtask size` 为 +93 −11（净 +82），停止线 250。

## 提交列表

```
2a722f6 test(git): IT-git-racy-unstaged, and T229's case back to a same-size change
4f2cc03 fix(git): run the snapshot tier in the work tree's top
```

## 设计偏差 / 补充

- **5.2.10 第 450 行"工作目录"**，以及 **08-security 5.8.5 表里 git 那一行"工作目录是 cupper 自己的目录"**：快照档的工作目录改为工作区顶层，仓库档仍用 `-C <repo>`。
  - 原因：`write-tree` 这类不需要工作区的命令在回写 index 时，按相对当前目录的路径 lstat，来决定是否 smudge racy 条目；当前目录不在工作区时，lstat 失败，racy 条目漏掉 smudge。回写后副本的时间戳变新，条目不再 racy，之后的 `add -A` 就漏掉同一秒内同大小的改动。
  - 约束：必须正好是顶层（`Layout` 所在的目录）。用子目录会给 pathspec 加上前缀。
  - 按你说的，这两处在合并时由你改。
- **工作区在调用前一刻消失时**，spawn 的 chdir 失败，走 `ShotError::Run`，对外是 `git_error` 或 `failed/git_failed`。现有设计没写这种情况，可以在 5.2.10 补一句。

## 遗留问题 / 范围外发现

- 没有新的产品缺陷。
- 观察：`Layout::read` 要求 `.git` 就在工作区目录下，所以会话的工作区如果是某个仓库的子目录，快照会记为 `skipped/not_a_repo`。这是现有行为，本任务没改。如果产品允许把项目加在仓库的子目录上，需要另行确认。
- 顺带：我停掉了 T227 时遗留在后台、还在等 CI 的一个轮询脚本。它只做只读查询，没有影响。
