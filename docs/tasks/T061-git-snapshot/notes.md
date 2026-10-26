# T061 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）
- 日期：2026-10-26

## 读到的现状（开工前）

- `GitCmd<Snapshot>` 只有 `LsFiles` 一个命令；`SnapshotTier::prepare` 只设了 `GIT_DIR` 和 `GIT_WORK_TREE`。
- 会话 actor 的快照端口是 `skipped()`：回合机器发 `Snapshot { turn_id, phase }`，actor 立即回一个 `skipped`。
- 回合机器已经处理了这些：起始快照 3 s 超时（`failed/timeout`）；结束快照到来之前不写 `turn.completed`。
- `turns` 表有 `snapshot_start`、`snapshot_end`（tree id）和 `snapshot_status` 三列，目前只写 `snapshot_status`。
- 仓库档的恶意配置夹具（`git/repo/tests.rs` 的 `SETUP`）可以借来做快照档的逐键确认。
- 本机 git 2.50.1，没有 git-lfs，LFS 用例要靠 xtask 下载的锁定版本。
- `perf` 作业还没有实现（`run: None`），PERF-snapshot 先写成每晚运行的 `#[ignore]` 用例，基准数据记在这里。

## 设计上要定的几点（开工时定，写进汇报）

- **快照档带 `HOME`**：
  - git 默认的全局忽略文件 `~/.config/git/ignore` 和默认属性文件 `~/.config/git/attributes` 要靠 `HOME` 才能找到；不带的话，快照与用户的 git 结果不同（D02 实验在有 `HOME` 时验证了全局忽略文件生效），属性里的过滤器也会漏检。
  - 全局配置由 `GIT_CONFIG_GLOBAL` 指向 cupper 的空文件代替，git 这时不读 `~/.gitconfig`，也不读 XDG 的 `git/config`，所以带 `HOME` 不会引入用户配置。
- **超大未跟踪目录**：设计没有给阈值，定为"整个未跟踪的目录里文件超过 10,000 个"。
- **快照档不发 `git.unsafe_repo_config`**：快照档不执行用户配置里的任何程序，也就没有拒绝。读不出配置时快照记 `failed/git_failed`。

## 开工前的规模估计

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 快照档的目录与环境 | `repo_key`、`workspace_key`、0700 目录、`HEAD`、alternates；`prepare` 补齐 `GIT_INDEX_FILE` 等六个变量与 `HOME` | 90 | — |
| 白名单配置 | 从有效配置挑白名单键；LFS 标准取值换成绝对路径；`objectFormat`；临时文件改名 | 70 | 40（单元） |
| 内容一致 | 复制 index（保留 mtime）、sharedindex、`info/exclude`、`info/attributes`、`info/sparse-checkout` | 60 | — |
| 快照命令 | `SnapshotOp` 加 `add -A`、`write-tree`、两种 `ls-files -o`、`check-attr`、`hash-object`、`update-index --cacheinfo`、`diff-tree --numstat` | 50 | — |
| 快照过程 | 预检（超大文件、嵌套仓库、超大目录）、过滤器检测与占位、`add -A --pathspec-from-file`、`write-tree`、结束时统计、错误归类 | 200 | — |
| 接入回合 | actor 另起快照任务（持 `Hold`、计入在途）、结果回到机器、tree id 写行、去掉 `skipped()` | 70 | 50（会话夹具） |
| LFS 的测试工具 | `cargo xtask git-lfs`：按锁定版本下载、核对哈希、解压；CI 的 rust 作业先取它 | 60 | — |
| IT-git-snapshot-parity | 与 SP-5 方案对比 tree：基本场景、split index、sparse index、untracked cache、linked worktree、sha256、autocrlf；三类排除 | — | 190 |
| IT-git-racy | 显式设 mtime 构造"同一秒、大小不变"；临时 index 的 mtime 等于原文件 | — | 50 |
| IT-git-filter-placeholder | 只在 `info/attributes` 里声明的过滤器；占位 blob；对象库里没有明文 | — | 50 |
| IT-git-snapshot-malicious | 借仓库档的恶意配置；拍起始和结束快照后逐键断言没有标记 | — | 70 |
| IT-git-lfs-snapshot | 有 git-lfs 时 tree 与用户的一致；没有时跳过并说明 | — | 50 |
| PERF-snapshot | 1 万文件合成仓库，`#[ignore]`，每晚运行 | — | 40 |
| **合计** | | **约 600** | **约 540** |

- 直接相加约 **1,140**，低于 1,300 的停止线。
- 但最近四个任务的实际行数都比估计多 30% 以上（T175 用例多 73%，T177 实现多 68%）。按 1.2 推算约 **1,370**，越过停止线 1,300，接近硬停止线 1,450。按任务书，开工前先停下汇报，给出拆分建议。

## 拆分建议

| 拆分 | 内容 | 估计（直接相加） | 按 1.2 |
|---|---|---|---|
| **T061a 快照档与回合快照** | 隔离与白名单配置、内容一致、排除、过滤器占位、快照过程、接入回合；IT-git-snapshot-parity、IT-git-racy、IT-git-filter-placeholder、IT-git-snapshot-malicious（Q32 逐键）；会话夹具的接入用例 | 约 990 | 约 1,190 |
| **T061b LFS 与性能** | 白名单里 LFS 标准取值的实测；`cargo xtask git-lfs` 与 CI 步骤；IT-git-lfs-snapshot；PERF-snapshot 与基准数据 | 约 150 | 约 180 |

- b 依赖 a。LFS 的配置生成本身很短，留在 a 里（有单元测试），b 只负责用真的 git-lfs 实测和下载工具。
- 另一种做法：不拆，停止线提到约 1,450。

## 裁定后（计划 v1.3.51）

范围收窄为快照档与回合快照。LFS 的实测、`cargo xtask git-lfs` 和 PERF-snapshot 归 T186。开工时定的四点都已获认可。

### 实现时的取舍

- **过滤器检测的条件**：设计写的是"属性来源里没有任何 `filter=` 时跳过"。实现改为看用户的有效配置：配置里定义了 `clean` 或 `process`、而快照档不执行的过滤器（git-lfs 由宿主执行时除外），才执行 `ls-files` 加 `check-attr`。理由：
  - 配置里没有定义的过滤器，用户自己的 git 也按原样处理，快照与用户的 git 一致，明文本来就在用户自己的对象库里；
  - 这样不必逐个找工作区里的 `.gitattributes`；
  - `check-attr` 仍然覆盖全部属性来源（工作区、`info/attributes`、`core.attributesFile`、默认属性文件）。
- **占位 blob 的内容**：用固定的 ASCII 格式 `cupper: not stored, filter <名称> not run; sha-256 <哈希>`，没有用设计里的中文例句。宿主不产生界面文案，界面可以按这个格式识别占位。
- **`add -A` 没加进全部文件**（带 `--ignore-errors` 时退出码不为 0）：快照照常写出，状态记 `partial`。
- **快照的状态**：
  - 没有 git：`skipped`，不带原因；
  - 工作区不是仓库：`skipped/not_a_repo`；
  - git 失败或超时：`failed/git_failed`，详情写日志；
  - 有排除或 `add` 不完整：`partial`。
- **`mark_external()`**：快照档不改用户仓库（对象写进私有库，index 是临时复制的），所以不用。

### Q32 快照档的逐键结果

用仓库档的恶意配置样例（`git/repo/tests.rs` 的 `SETUP`）：每个程序被执行时都会留下标记文件。
- 在 `evil`、`include`、`required`、`lfs` 四个仓库上各拍一次起始快照，改一个文件，再拍一次结束快照（带 `diff-tree --numstat`）。
- 涉及的命令：`config --list`、两种 `ls-files -o`、`ls-files`、`check-attr`、`hash-object`、`update-index --index-info`、`add -A`、`write-tree`、`diff-tree`。
- 跑完后没有任何标记。

| 键或来源 | 快照档里 | 挡住它的 |
|---|---|---|
| `core.fsmonitor`、`hooks/` 与 `core.hooksPath` 下的 hooks（post-checkout、post-index-change、reference-transaction、pre-auto-gc、fsmonitor-watchman） | 没有执行 | 快照档的 `config` 没有这些键，GIT_DIR 里没有 `hooks/`；命令行另加 `-c core.fsmonitor=false -c core.hooksPath=<空目录>` |
| `filter.*.clean`、`filter.*.smudge`、`filter.*.process`（含 `required=true` 的过滤器和 include 进来的定义） | 没有执行 | 配置里不定义这些过滤器；命中的文件写占位 blob，不经 `add` |
| `diff.*.textconv`、`diff.*.command`、`diff.external` | 没有执行 | 不定义 diff 驱动；`diff-tree` 本身默认不用 textconv 和外部 diff，另加 `--no-ext-diff --no-textconv` |
| `merge.*.driver`、`core.sshCommand`、`core.pager`、`core.editor`、`core.askPass`、`core.gitProxy`、`core.alternateRefsCommand`、`sequence.editor`、`gpg.program`、`gpg.ssh.program`、`gpg.ssh.defaultKeyCommand`、`credential.helper`、`credential.<url>.helper`、`uploadpack.packObjectsHook`、`remote.*.receivepack`、`remote.*.uploadpack`、`pager.log`、`submodule.*.update` | 没有执行 | 不在白名单里，快照档读不到；快照的命令也用不到它们 |
| `include.path` 指向工作区里的文件 | 没有执行 | 读配置只为挑白名单键和过滤器名，include 进来的过滤器同样不定义 |
| `info/attributes` 里的过滤器 | 没有执行 | 属性照常读取（复制了 `info/attributes`），引用的过滤器不定义，命中的文件写占位 |

**防护是分层的**：
- 单独把 GIT_DIR 换成用户自己的 `.git`，测试仍然通过：命令行覆盖、占位和 `--no-textconv` 也都挡得住。
- 同时去掉占位，恶意用例和占位用例都会失败。
- 所以恶意用例证明的是"这几层合在一起，什么都不执行"，不单独证明某一层。

### 变异检查

| 变异 | 结果 |
|---|---|
| 复制 index 时给新的 mtime | IT-git-racy 与 parity 失败 |
| 不复制 `info/exclude` | parity 失败 |
| 不写占位 | IT-git-filter-placeholder 失败（对象库里出现明文） |
| 用户的 GIT_DIR 加上不写占位 | IT-git-snapshot-malicious 与 IT-git-filter-placeholder 失败 |
| 只去掉"复制时设回 mtime"那一步 | 用例仍通过。macOS 上 `std::fs::copy` 本身就保留 mtime（`fcopyfile` 的 COPYFILE_STAT）；显式设回是为了不依赖这一点 |

### 实际行数（`cargo xtask size`）

| 提交 | 新增 |
|---|---|
| 快照档与回合接入（实现约 836，其中白名单配置的单元测试约 70；四个 IT 约 250） | 1,087 |
| 会话层用例 | 42 |
| **合计** | **1,129**（停止线 1,300 以内） |

实现比估计多约 40%，主要在快照过程，rustfmt 展开较多；用例比估计少：恶意样例借用了仓库档的夹具。
