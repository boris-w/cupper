# T060 汇报

- 执行者：子 agent（Rust 工程师，宿主安全）
- 分支：`feat/T060-git-cmd`
- 完成日期：2026-10-20

收窄后的 T060（GitCmd 与三档类型）做完了。UNIT-git-cmd 和两个 `compile_fail` 证明都通过，`cargo xtask ci pr` 全部通过。规模 713 行，超出约 650 的上限，在 750 的硬停止线以内。

## 做了什么

1. **git 用的拉起辅助**（`git/run.rs`）
   - 经 `cupper_sys::spawn` 拉起。有输入就写进 stdin（`apply` 从 stdin 读补丁），git 没读完就退出不算错，看退出码；没有输入时 stdin 是 `/dev/null`。
   - stdout 上限 64 MiB，超出即失败；stderr 只保留前 64 KiB，其余读掉丢弃，避免管道写满卡住 git。
   - 超时由 `Clock` 驱动。到时先记下子进程树的全部进程组再发 SIGKILL，然后回收。
2. **时长登记**（`durations.rs`）：`git_command` 30 s（只读命令和仓库档的写操作）、`git_snapshot` 60 s、`git_worktree_add` 10 分钟、`git_push` 5 分钟。
3. **`GitCmd<档位>`**（`git/cmd.rs`）
   - 档位是类型参数，标记类型是 `Repo` 和 `Snapshot`。只有本档的入口能构造本档的命令：`GitCmd::repo(RepoOp)` 和 `GitCmd::snapshot(SnapshotOp)`。
   - 仓库档命令表 `RepoOp` 按 5.2.10 列全：
     - 只读的 `rev-parse`、`for-each-ref`、`rev-list`、`symbolic-ref`、`merge-base`、`check-ref-format`、`branch --list`；
     - 写操作 `apply --cached` / `--cached -R` / `-R`（都带 `--whitespace=nowarn`）、`update-index --refresh`、worktree 的 add、lock、unlock、remove、prune、repair；
     - 外加 T098 读配置用的 `config --list --show-origin --includes -z`。
   - 快照档的最小入口是 `SnapshotOp::LsFiles`（`ls-files -z --cached --others --exclude-standard`，5.2.11）。
   - 命令行的顺序：
     1. `--no-pager`；
     2. 只读命令加 `--no-optional-locks`；
     3. `-c core.quotepath=false`；
     4. 调用方加的 `-c`；
     5. 档位固定的 `-c`；
     6. `-C <仓库>`；
     7. 子命令和它的固定参数（`-z` 只在命令支持时加）；
     8. 调用方的参数。

     档位固定的覆盖放在调用方的 `-c` 之后，调用方覆盖不了。
   - 编译期证明：模块文档里有两个 `compile_fail` 文档测试——仓库档入口构造不出 `GitCmd<Snapshot>`，快照档入口也构造不出 `GitCmd<Repo>`；另有一个能编译的对照。
4. **`GitTool` 与两档**（`git/mod.rs`）
   - `GitTool::new` 只用 `CliLocator::tool` 给的 git、git-lfs 绝对路径和 `trusted_path()`，并创建 `<数据目录>/git/` 和 `empty-hooks`（权限 0700）。git 的工作目录固定为 `<数据目录>/git/`。
   - 环境只有 `PATH`（可信条目）、`LC_ALL=C`、`GIT_TERMINAL_PROMPT=0`、`GIT_PAGER=cat`、`SSH_AUTH_SOCK`。仓库档另带 `HOME`（按补充裁定 3）；快照档另带 `GIT_DIR`、`GIT_WORK_TREE`，其余环境由 T061 补。
   - 仓库档和快照档都固定加 `-c core.fsmonitor=false` 和 `-c core.hooksPath=<数据目录>/git/empty-hooks`。
   - `RepoTier::prepare` 和 `SnapshotTier::prepare` 产出 `Prepared`（命令、输入、超时键、是否只读），`Prepared::run` 时从 `Durations` 取超时。`RepoTier::prepare` 的文档写明只给 T098 带防护的 `run` 用。

## 关键取舍

- **用类型参数分档，不做三套重复的构造器。** 共用的拼装逻辑写一次，两个入口只接受本档的操作枚举。
- **执行与防护分开。** 仓库档的执行入口叫 `prepare`，把 `run` 这个名字留给 T098 带防护的版本，避免出现名字相同、却不做检查的入口。
- **与 T097、T098 的接口写在 `notes.md` 里**：
  - 无条件的覆盖由 T060 固定加上；
  - T097 只算取决于配置的覆盖，结果是 `(键, 值)` 列表，由 T098 经 `GitCmd::config` 加进命令；
  - git-lfs 的绝对路径从 `GitTool::git_lfs()` 取；
  - 生成补丁的命令要加的 `--no-ext-diff --no-textconv` 由加这类命令的 T061 写进固定参数。

## 验证方式

- **UNIT-git-cmd**，3 个用例，都通过：
  - `unit_git_cmd_tiers_pass_only_their_environment_and_fixed_arguments`：假 git 脚本记下收到的环境、参数、stdin 和工作目录。断言两档的环境只有规定的变量（过滤掉 `/bin/sh` 自己设的 `PWD` 等）；参数逐字相符；工作目录是 `<数据目录>/git`；写操作的输入到了 stdin；`worktree add` 用长超时；只读命令带 `-z`。
  - `unit_git_cmd_run_feeds_stdin_and_keeps_stdout_and_stderr_apart`；
  - `unit_git_cmd_run_kills_git_at_the_timeout`：`ManualClock` 推进，不真的等。
- **编译期分档**：两个 `compile_fail` 文档测试和一个对照都通过。rustdoc 生成的测试名（如 `git::cmd (line 7) - compile fail`）带不上编号，按裁定归在 UNIT-git-cmd 名下。
- **顺带的发现**：假 git 脚本里的 `cat` 必须写成 `/bin/cat`，因为 git 子进程的 `PATH` 只有可信条目。这说明 git 只能从可信条目里找到它要再拉起的程序。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T060-git-cmd/ci/`，全部通过（rust 1273、protocol 185、ts 323、license、testdata-scan），摘要 `pr-afa90583929b.json` 已提交。
  - 分支上的 `tests/ids.toml` 早于 v1.3.16，没有登记 UNIT-git-cmd，所以摘要里没有这个编号，合并后以 main 为准。
  - testdata-scan 里新加的 `sensitive-words` 步骤在 worktree 里被跳过，原因是本地敏感词表只在主工作区。
- 没有对本机任何真实仓库运行 git；用例里的 git 都是假脚本。分支 diff 里没有本机路径和用户名。

## 规模

713 行：实现约 535，测试约 178（拉起辅助 53、UNIT-git-cmd 104、文档测试 21）。比裁定时推算的约 685 多 28 行，多在假 git 的记录与读取上。逐模块数字在 `notes.md` 里。

## 提交列表

```
c18a6e1 docs(task): T060 pr CI summary
afa9058 docs(task): T060 final line counts and notes on the tests
3657ade test(host): UNIT-git-cmd, the environment and arguments each tier passes
3666723 docs(task): T060 line counts so far, and why work stopped
791a97c feat(host): GitCmd and the repo and snapshot tier types
d2307aa feat(host): run a git command with stdin, separate stderr and a Clock timeout
44a43ea docs(task): T060 estimate after the split, and the interface with T097 and T098
41e2dd1 docs(task): T060 size estimate and split proposal
```

## 遗留问题 / 范围外发现

- **交接给 T098**：
  - 带防护的 `RepoTier::run` 放在 `git/` 下；
  - 读配置用 `GitCmd::repo(RepoOp::ConfigList)`；
  - T097 算出的覆盖经 `.config(键, 值)` 加进命令；
  - 运行用 `tier.prepare(cmd).run(clock, durations)`；
  - 指纹变化时按 `Prepared::read_only` 决定丢弃结果还是照常返回。
- **交接给 T061**：
  - 在 `SnapshotOp` 里加快照档命令，生成补丁的要带 `--no-ext-diff --no-textconv`；
  - 在 `SnapshotTier::prepare` 里补 `GIT_INDEX_FILE`、`GIT_OBJECT_DIRECTORY`、`GIT_ALTERNATE_OBJECT_DIRECTORIES`、`GIT_CONFIG_NOSYSTEM`、`GIT_CONFIG_GLOBAL`，并决定是否带 `HOME`；
  - 快照用 `GIT_SNAPSHOT` 超时。
- **交接给 T123**：用户档加一个 `User` 标记和 `UserTier`。用户档不加 hooks 的覆盖，带 `HOME`；`push` 用 `GIT_PUSH` 超时。
- **可能的合并冲突**：T097 也在 `git/` 下加文件，合并时 `git/mod.rs` 的模块声明可能有相邻行冲突。

worktree 和分支都保留着。