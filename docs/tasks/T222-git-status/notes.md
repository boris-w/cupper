# T222 notes

## 开工前的规模估计（2026-11-03）

停止线 450，硬停止线 550，取回的代码也算。从 T120 的 `38f85e0` 取回的部分按实测，其余是估计：

| 模块 | 内容 | 行 |
|---|---|---|
| `git/status.rs` | 文件状态（临时 index 上 refresh，再 `diff-index --cached -M`、`diff-files`、`ls-files -o`）、`name_status` 解析、进行中的操作（标记文件）、`branches` | 约 135（实测 220 里去掉移到 `head.rs` 的 HEAD） |
| `git/head.rs` | 在 T120 只取提交的基础上加回分支、上游和领先落后（`upstream:track` 的解析写成纯函数） | +25 |
| `git/service.rs` | `status`、`branches`、按方法分派 | +45 |
| `git/cmd.rs`、路由 | `SnapshotOp::Refresh`、`DiffIndex`、`DiffFiles`；`Routes::git` 加两个方法；UNIT-unsupported-set 去两行 | +18 |
| `cupper diff` | 参数（`--scope unstaged|staged|last-turn|<回合 ID>`）、调用 `git/diff` 再逐个文件调 `git/file_diff`、文字输出 | +52 |
| 单元用例 | `name_status`、`upstream:track`、`in_progress`；CLI 的参数和文字输出（纯函数） | 约 65 |
| IT-git-status | 会话夹具加真实 git：HEAD 的三种状态（分支带上游领先落后、游离、无提交），进行中的操作，文件状态（暂存、修改、未跟踪、重命名、删除、未合并），branches；进程级 IT-cli-commands 加 `cupper diff` 一段（宿主到 CLI 的整条路径） | 约 100 |
| 合计 | | 约 440 |

离停止线只剩约 10 行，余量很小；推算会越过 450 就停下报告。

## 推算越过停止线，暂停（2026-11-03）

宿主侧取回并接好后（status、branches、HEAD 的上游，加两份单元用例），`cargo xtask size` 已是 +352 −42。还没做：`cupper diff`（参数、输出和单元用例约 75）、IT-git-status（约 100）。推算合计约 530，越过停止线 450，低于硬停止线 550。估计偏低在两处：`head.rs` 改写后的差异按整个文件计（+~100，估了 25）；`status.rs` 连单元用例 184 行（估了 135 加用例）。

做法建议：放宽停止线到约 540、一次做完；或者把 `cupper diff` 拆出去（约 75 行），本任务约 455。
