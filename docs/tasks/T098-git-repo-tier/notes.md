# T098 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）
- 日期：2026-10-20

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行。实现按 T054、T060 的经验乘 1.3（文档注释和 rustfmt 展开），测试按写出来的估。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `git/cmd.rs` | `GitCmd::global_args`：T097 的 `Overrides::global` 是现成的 `OsString` 参数（键可能不是 UTF-8），不能经 `.config(String, String)` 转一道，否则有损转换会让置空对不上原来的键 | 8 | — |
| `git/mod.rs` | 声明 `repo` 模块；测试构建里给 `GitTool` 加额外环境变量的口子（用例要 `GIT_CONFIG_NOSYSTEM`，不读本机的系统配置） | 10 | — |
| `git/repo.rs` | `RepoTier::run`：`Layout::read` → `ConfigList` → `parse` → `check_sources` → `overrides` → `global_args` → 调用前指纹 → 执行 → 调用后指纹 → 判定；`~/` 开头的 `core.hooksPath` 展开后再交给 `Fingerprint::take`；`GitError`（thiserror，`code`/`reason` 按协议，notice 信号）；`RepoNotice`；失败时 stderr 末尾经 `diag::redact::credentials` 脱敏 | 140 | — |
| IT-git-repo-tier-malicious | 临时仓库与隔离环境（`HOME`、`GIT_CONFIG_NOSYSTEM`）；写标记文件的"程序"；恶意的 `.git/config`（5.2.10 第 4 条全部的键加新补的 5 类、fsmonitor、`core.hooksPath` 指向工作区）、全部常用 hooks、`.gitattributes`；读写各几条命令后没有标记；include 到工作区、子段带 `=` 拒绝；`required=true` 报错不重试；普通失败不误判；中途改配置（包一层假 git，命令后改 `.git/config`）；git-lfs 假脚本核实 `clean`、`smudge` | — | 200 |
| **合计** | | **约 158，乘 1.3 约 205** | **约 200** |

合计约 **405 行**，在约 500 的停止线以内。合成大仓库上指纹的耗时用临时用例量一次、写进本文件，不提交那段代码。
