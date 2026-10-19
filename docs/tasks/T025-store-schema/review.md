# T025 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 合并前处理：分支开出后，main 合入了 T014 和 T051。编排者把 main 合入分支（`375e97a`），`Cargo.lock` 自动合并，`cargo metadata --locked` 通过。之后在任务 worktree 里跑 `cargo xtask ci pr --record`，结果 rust 492、ts 64、license 1，全部通过，摘要为 `ci/pr-375e97abf8b3.json`。
- 核对：
  - 没有 `unsafe`，没有 clippy 豁免，没有个人信息；
  - 时间只经 `Clock`，备份文件名里的时间也从 `Clock` 来；
  - 新增依赖 `cupper-protocol`，在设计 5.1.3 的依赖表里。
- 代码审阅：
  - 每个脚本在自己的 `BEGIN IMMEDIATE` 事务里执行，同一事务写 `schema_version`；新库的 meta 初值放在第一个脚本的事务里。这样在任何时刻崩溃，都不会留下"有版本、没有 `host_id`"的库。
  - 备份先写 `host.partial`，然后 `sync_all`、改名、fsync 目录，中途崩溃留下的半截文件不会被当成备份；备份按版本前缀查找，`host-v1-` 不会匹配到 `host-v10-`。
  - 表结构测试直接执行设计文档 5.4.2 的 SQL 块，用 pragma 逐项比对，以后设计和迁移脚本不一致时会立刻发现。做法可取。
  - raw.db 按 `user_version` 判断，版本不同就在事务里删表重建，不删正在打开的文件。
  - 执行者对表结构做了 8 种变异、对迁移框架做了 5 种变异，都被测出来了。
- 规模：1,348 行，在约 1,500 行的上限以内。
- 编排者裁定：
  - `VIEW_VERSION` 挪到 `cupper-protocol::view`，和视图类型放在一起，视图的 JSON 格式变了就一起改版本。由 T026 在第一次用到检查点时挪过去。
  - `turn_snapshots.v` 的含义由 T026 定。执行者建议每个 op 带自己的 `v`，T026 可以照此做。
  - `backups/` 只保留最近 3 份（5.4.5），归 T027 的保留清理。
  - `Backup`、`Unreadable`、`BadVersion` 这三种错误的退出码由 T037 定，建议用 5。
  - T050 的 `driver/tap/mod.rs` 里有一处文档链接指向私有的 `redact`，`cargo doc -D warnings` 会失败。CI 不跑 rustdoc，所以没被发现。交给 T055，它下一次改 `driver/` 时顺手修。
  - 以后的迁移脚本如果要按 SQLite 的 12 步法重建表，需要在事务外关闭外键。记给 T160。
- 转交事项已写进 `docs/tasks/handoff-notes.md`。
