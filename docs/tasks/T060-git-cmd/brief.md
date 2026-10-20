# T060 Git 服务 I：GitCmd 三档与仓库档防护

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主安全）。T054 的执行者接手，因为可信 PATH 和 git 的绝对路径都是它做的
- 分支：`feat/T060-git-cmd`
- 依赖：T054（已合并）。T061（快照档）、T044（信任重扫要 git 根目录）、T034 依赖本任务。**本任务离剩余关键路径只有 1 个单位的松弛**，请优先保证进度

## 背景

宿主运行在所有 agent 沙箱之外，而仓库里的配置（fsmonitor、hooks、filter、diff 驱动、include）可能被 agent 写过，所以**宿主自己发起的 git 调用不执行仓库配置指定的任何程序**（设计 5.2.10，安全 #1，纪要 D8，ADR-10）。为此按"在哪里执行"把调用分成快照档、仓库档、用户档三档。

本任务做三档共用的 `GitCmd` 构造器，以及仓库档的全部防护。快照档归 T061，用户档归 T123。

## 范围

**做**（计划 T060 一行；设计 5.2.10 的"三档调用""仓库档：每次调用前读配置""仓库可执行配置指纹""调用约定"）：
1. **`GitCmd` 构造器**（调用约定，三档共用）：
   - 每类命令一组固定参数，统一 `-z`；只读命令加 `--no-optional-locks`；固定加 `-c core.quotepath=false`；
   - 只用宿主启动时解析出的 git、git-lfs 绝对路径（`CliLocator::tool`，交接，T054），不按 PATH 查找；
   - 环境：PATH 只保留可信条目（`CliLocator::trusted_path()`），`LC_ALL=C`，`GIT_TERMINAL_PROMPT=0`，`GIT_PAGER=cat`，继承 `SSH_AUTH_SOCK`；其余一律不带；
   - 工作目录设为 `<数据目录>/git/`，目标仓库经 `-C <路径>`（仓库档、用户档）或 `GIT_DIR`、`GIT_WORK_TREE`（快照档）指定；
   - 经 `cupper_sys::spawn` 拉起（T094 的 `driver/probe` 里有带 `Clock` 超时的一次性拉起辅助，能复用就复用，需要 stdin 或更大输出时在 `cupper-host` 里另写）；
   - 超时按 5.2.10：只读命令 30 s；快照 60 s；`worktree add` 10 分钟；`push` 5 分钟。登记进 `durations.rs`。
2. **三档类型从第一次调用起分开**：三档是三个不同的类型，仓库档的命令只能经仓库档的入口构造，编译期就分开，不能用一个参数切换。
3. **仓库档的防护**（逐条按 5.2.10 的第 1–6 条）：
   - 每次调用前 `git config --list --show-origin --includes -z`，得到有效配置和每个键的来源文件；
   - 任何配置来源位于工作区内或其他 agent 可写的位置（由调用方传入的项目、工作区根目录）时拒绝，返回 `git_error`（`reason: unsafe_repo_config`）。仓库自己的 git 目录（`.git/config`、`config.worktree`）除外，但 `include.path` 指向 git 目录之外的工作区文件时照样拒绝；
   - 无条件覆盖 `core.fsmonitor=false`、`core.hooksPath=<数据目录>/git/empty-hooks`，`--no-pager`；生成补丁的命令加 `--no-ext-diff --no-textconv`；
   - 可执行类键（第 4 条的清单）取值不在白名单里的，一律用 `-c <键>=` 置空；白名单只有 git-lfs 的标准取值，程序写成 git-lfs 的绝对路径（Q32，未实测，用例里核实能用）；
   - 覆盖后 git 报错：这次调用失败，返回 `unsafe_repo_config`，**不去掉覆盖重试**；
   - 调用结束后再算一次指纹：变了时只读命令丢弃结果、返回 `repo_config_changed`；写操作照常返回结果；两种情况都给出要写 `notice` 的信号。
4. **仓库可执行配置指纹**（UNIT-git-fingerprint）：仓库的 `config` 及其 include 的文件、worktree 的 `config.worktree`、`hooks/` 和 `core.hooksPath` 指向的目录下的文件（名称和哈希）、`info/attributes`、工作区里的全部 `.gitattributes`。提供"前后比较并给出变化路径"的函数，供回合前后（T061）和每次仓库档调用使用。
5. **结果与 notice**：本任务返回结构化的错误和"需要写 notice"的信号（`unsafe_repo_config`、`repo_config_changed`，带 `paths`）；写进会话流由调用方（会话 actor、git 方法的执行者）做。

**不做**：
- 快照档的私有对象库、临时 index、回合快照（T061）；
- 用户档 `commit`、`push`（T123）；`git/*` 方法、diff 与 hunk（M1b 的任务）；
- `RepoLock`（第一个改用户 index 的任务来做）；
- 写会话流的 notice。

## 验收标准

- [ ] 新增 IT-git-repo-tier-malicious：在临时目录里造出恶意仓库，覆盖 5.2.10 列出的每一类可执行配置——fsmonitor、hooks（含 `core.hooksPath` 指向工作区）、`filter.*.clean/smudge/process`（含 `required=true`）、`diff.*.command`、`diff.*.textconv`、`include.path` 指向工作区文件、`core.sshCommand`、`core.pager` 等——每个"程序"一运行就留下标记文件；断言经仓库档调用之后**没有任何标记文件**，并且该拒绝的拒绝（`unsafe_repo_config`）、该置空的置空、`required=true` 的过滤器报错后不重试；调用中途改了配置时得到 `repo_config_changed`。
- [ ] 新增 UNIT-git-fingerprint：指纹对每一类内容都敏感（config、include 的文件、`config.worktree`、hooks 的名称和内容、`info/attributes`、任意层级的 `.gitattributes`），对无关文件不敏感；变化时给出路径。
- [ ] 三档在类型上分开：有一个编译期的证明（例如 doctest 的 `compile_fail`），仓库档的入口构造不出快照档的命令，反之亦然。
- [ ] 环境只含上面列出的变量（用例断言 git 子进程收到的环境）。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T060 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T060-git-cmd/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T054：git 子进程的 PATH 用 `CliLocator::trusted_path()`；git 的绝对路径用 `CliLocator::tool("git")`。
- T094：`/usr/bin/git` 的占位程序在检测阶段已经处理，本任务拿到的 git 路径都是检测通过的。

## 注意

- **用例只在本任务自己建的临时仓库里运行 git**，绝不对本机真实的仓库（包括本仓库）运行仓库档的命令；用例里的 `HOME`、`XDG_CONFIG_HOME`、`GIT_CONFIG_GLOBAL`、`GIT_CONFIG_SYSTEM` 指向临时目录，免得读到本机的全局配置。
- 恶意配置里的"程序"只是写标记文件的小脚本，不做任何其他事。
- 代码约束：子进程一律经 `cupper_sys::spawn`；时间只经 `Clock`；`cupper-host` 不用 `unsafe`；错误用 thiserror。
- SP-5 的实验代码（`spikes/SP-5-git-snapshot/`）可以参考，产品代码不依赖它。

## 规模

计划估为 M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**；计划 1.1 的经验是实现按草估的 1.7 倍计，T054 和 T094 的实际偏差主要来自 rustfmt 展开结构体字面量和文档注释。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 1,000 行时，开工前就停下来汇报，并给出拆分建议**（例如把"指纹"单独拿出来）；推算贴线时也要停下来，不要指望后面能压回去。

## 参考资料

- 设计：`02-host.md` 5.2.10；`08-security.md` 5.8.5；`13-open-issues.md` Q32。
- 安全 #1、安全核对 2、纪要 D8；ADR-10。
- `docs/tasks/SP-5-git-snapshot/`、`spikes/SP-5-git-snapshot/`；`docs/tasks/T054-cli-discovery/`、`docs/tasks/T094-login-shell-version/`。
- `crates/cupper-host/src/driver/locate/`、`driver/probe/`。

## 工作方式

在原来的 T054 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T060-git-cmd main`。T054 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
