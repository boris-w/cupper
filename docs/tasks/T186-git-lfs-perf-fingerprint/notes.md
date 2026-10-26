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
