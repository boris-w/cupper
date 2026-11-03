# T121 notes

## 开工前的规模估计（2026-11-03）

按开工前报给编排者的拆分，本任务只做 (a)，另加会话流的 notice 通路。停止线 550，硬停止线 650。

| 模块 | 内容 | 行 |
|---|---|---|
| `git/diff/hunks.rs` | 切分时记下每个单位在补丁里的行；`pick`：按 `hunk_id` 选出单位，同一文件的合成一节（整文件原样；`mode` 只带模式行；`hunk` 带 `index`、`---`、`+++` 再接选中的 hunk） | +45 |
| `git/diff.rs` | `patch` 改为一次生成多个文件或整个 diff，`filter_unsupported` 的文件总是排除；`Diffed` 记下范围和快照档不执行的过滤器 | +15 |
| `git/service.rs` | stage/unstage/revert 共用一条路径：取缓存、核对范围、解析 target、用户仓库里 `check-attr filter`、`RepoLock`、`mark_external`、仓库档 `apply`、错误映射、账本 | +130 |
| `RepoLock` | 按仓库（common dir）的异步锁 | 25 |
| notice 通路 | `SessionNotes` trait；会话 actor 的 `Mail::Notice`，以及管理器上的入口；仓库档调用的 notice 和拒绝都经它写进会话流 | +40 |
| 接线 | `RepoOp::CheckAttr`、路由、UNIT-unsupported-set、宿主组装、会话夹具 | +15 |
| 单元用例 | `pick` 的各种组合 | 40 |
| IT-git-stage | 会话夹具加真实 git：三种 target 的 stage/unstage/revert，范围不对、过期 token、补丁打不上时的 `stale_diff`，账本重放，`filter_unsupported`，window 角色不带 input 被拒，配置变化时的 notice | 140 |
| 合计 | | 约 450 |

## 完成时

- `cargo xtask size`：+517 −50，在停止线 550 以内。
- 写操作的路径：
  1. 按 `diff_token` 取出缓存的 `Diffed`，先核对操作与范围对得上：stage 只认 `unstaged`，unstage 只认 `staged`，revert 认 `unstaged` 和回合范围；对不上返回 `stale_diff`。
  2. 解析 target：
     - `{all}`：对整对 tree 生成补丁，排除 `filter_unsupported` 的文件；
     - `{paths}`：只生成这些文件的补丁，含重命名的源路径；路径不在 diff 里返回 `stale_diff`，文件在过滤器下返回 `filter_unsupported`；
     - `{hunk_ids}`：对整对 tree 跑一次 `diff-tree -p`，用 `hunks::pick` 选出单位；有 ID 找不到就返回 `stale_diff`。
  3. 补丁为空时返回 `stale_diff`（"nothing to apply"）。
  4. 只有暂存，并且配置里有快照档不执行的过滤器时，才在用户仓库里对目标文件跑一次 `check-attr filter`（仓库档，受防护），命中就返回 `filter_unsupported`。
  5. 拿 `RepoLock`（按仓库的 common dir），`mark_external` 落盘，再在仓库档执行 `apply`。
- 错误与账本：
  - `apply` 失败时什么都没改：返回 `stale_diff`，index.lock 被占时返回 `lock_held`；ticket 被 reject，重试会再执行一次。
  - git 被杀或没能启动、调用后的指纹算不出：结果未知，ticket 保持 `pending_external`，重试得到 `outcome_unknown`。
  - 成功时记下 `{}`，重试回放。
- notice 通路：新增 `SessionNotes` trait，`SessionManager` 实现，经 `Mail::Notice` 交给会话 actor 写进会话流。仓库档调用的 notice（配置在调用中途变了、拒绝调用）都走这条路，包括读操作的 HEAD 和暂存前的 `check-attr`。没有会话的 `git/branches` 不写。
- 没有测到的：`RepoLock` 的并发互斥；`mark_external` 之后崩溃的行为。这两项归 T229 的故障点用例。范围核对里，revert 拿 `staged` token 这类组合在 IT 里没有专门的用例，因为 git 本身也会拒掉大部分不对的组合。
