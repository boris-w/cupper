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
