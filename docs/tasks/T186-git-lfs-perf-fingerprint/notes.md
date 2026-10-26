# T186 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）
- 日期：2026-10-26

## 开工前的规模估计

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `cargo xtask git-lfs` | 锁定 3.8.0（按机器架构选 arm64 或 amd64 的 zip），官方 release 地址进白名单，复用 `runtime-pack` 的下载与核对（占位 UA），核对后再解压到 `target/git-lfs/git-lfs`；CI 的 `rust` 作业先取它 | 60 | 15（哈希不符时不解压） |
| 回合前后的指纹 | `snapshot::take` 随快照算 T097 的指纹；actor 记下起始指纹，结束时比对，不同就写 `notice{repo_config_changed, paths}` | 35 | — |
| IT-git-lfs-snapshot | 真实 git-lfs；快照档的 PATH 里没有它，只能靠配置里的绝对路径；tree 与用户的 git 一致，LFS 文件是指针 | — | 40 |
| IT-git-turn-fingerprint | 会话夹具：回合里改 `.gitattributes` 和仓库配置，回合里有提示，路径正确；下一个回合没有改动就没有提示 | — | 50 |
| PERF-snapshot | 1 万文件合成仓库，无改动与改 10 个文件各取 5 次中位数；`#[ignore]`，每晚运行 | — | 40 |
| **合计** | | **约 95** | **约 145** |

合计约 **240**，按最近的偏差乘 1.2 约 **290**，在停止线 320 以内。

## 下载的物料

- `git-lfs-darwin-arm64-v3.8.0.zip`（2026-08-28 发布），哈希取自同一 release 的 `sha256sums.asc`：`caff76a7d070d8160c89bc39b6e85d98f24135b6fed038a3b4de2590d25102d8`。下载时用 curl 的 `-q` 和项目占位 UA，不带个人信息。amd64 版的哈希取自同一文件。
- 没有核对 `sha256sums.asc` 的 PGP 签名（需要 git-lfs 的签名公钥），与 SP-5 的做法相同。

## 结果

### Q32 的 LFS 部分（IT-git-lfs-snapshot）

- 测试仓库用 `git lfs install --local` 写入 git-lfs 的标准取值：`git-lfs clean -- %f`、`git-lfs smudge -- %f`、`git-lfs filter-process`、`required = true`。
- 快照档把程序换成宿主解析出的 git-lfs 绝对路径。快照档的 PATH 里没有 git-lfs，所以只有绝对路径生效时快照才能成功。
- 结果：
  - 快照的 tree 与用户的 git（PATH 里有 git-lfs）写出的相同；
  - `a.bin` 在快照里是 LFS 指针（`version https://git-lfs.github.com/spec/v1` 开头），没有排除，状态 `ok`。
- 变异：宿主不带 git-lfs 时，`a.bin` 变成 `filter_unsupported` 的占位，用例失败。
- 本机没有 git-lfs 时用例跳过，并打印原因（"`cargo xtask git-lfs` downloads the pinned one"）。

### PERF-snapshot 基准

- 环境：Apple Silicon 笔记本，macOS，git 2.50，热缓存。
- 合成仓库：1 万个小文件，已提交；5 次取中位数。
- 数字包括读配置、算指纹（遍历工作区找 `.gitattributes`）、准备 GIT_DIR、两次 `ls-files -o`、`add -A`、`write-tree`。

| 情况 | 中位数 |
|---|---|
| 没有改动 | 约 112 ms |
| 改了 10 个文件 | 约 115 ms |

- 设计 5.9.6 的预算是"9.6 万文件 ≤ 0.5 s"。用例里的上限设得宽（2 s），只防严重退化，趋势由 `perf` 作业（T117）按历史判断。
- 运行方式：`cargo test -p cupper-host --lib -- --ignored perf_snapshot`。

### 回合前后的配置指纹（IT-git-turn-fingerprint）

- 回合里改了 `.git/config` 和新增 `.gitattributes`：回合结束时写出 `notice{repo_config_changed}`，`params.paths` 是这两个文件，位置在 `turn.completed` 之前。
- 下一个回合没有改动，不再提示。
- 指纹就是仓库档用的 T097 纯函数，随每次快照计算：起始的那份留在 actor 里，结束时比对。

### 实际行数（`cargo xtask size`）

| 提交 | 累计新增 |
|---|---|
| 回合前后的指纹与用例 | 97 |
| `cargo xtask git-lfs` 与 CI 步骤 | 219 |
| IT-git-lfs-snapshot 与 PERF-snapshot | 287 |

- 合计 **287**，在停止线 320 以内。
- xtask 部分多于估计：两种架构的锁定、解压和用法说明。
