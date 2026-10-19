# T024 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 合并前处理：分支基于 T050 合并之前的 main。编排者把 main 合入分支（`54b613b`）：
  - `crates/cupper-host/Cargo.toml` 两边的依赖都保留，按字母序排列；
  - `Cargo.lock` 取 main 的版本，再由 `cargo metadata` 补上存储层的 6 个包。版本与分支原来的锁定一致；唯一的差别是 `cc` 用了 main 上的 1.9.0（分支上是 1.8.0）。
- 核对：
  - 合并后在任务 worktree 里跑 `cargo xtask ci pr --record`，结果 rust 393、ts 64、license 1，全部通过，摘要为 `ci/pr-54b613b2b52d.json`；
  - 没有 `unsafe`，没有新的 clippy 豁免，没有个人信息；
  - `rusqlite` 锁精确版本 `=0.40.2`，关掉了默认特性。
- 代码审阅：
  - 写线程：
    - 只有 `BEGIN` 之前切换持久性，因为事务内改 `synchronous` 会被 SQLite 拒绝；
    - savepoint 回滚只影响出错的操作；
    - 事务失败时整批作废，没执行到的操作得到 `Batch`；
    - `stop` 会先提交它之前提交的操作。
    - 这些行为都有用例覆盖，执行者还做了变异检查。
  - 读池：借出的连接放在 drop guard 里，读操作 panic 时照样归还；`Lease` 先归还连接，再释放许可，顺序正确。
  - `data_epoch` 是纯函数：只有"异常退出 × 重启过"才加 1；记录缺失按"重启过"处理，代价最多是一次全量快照。
- 接受的取舍：
  - "只在 SystemClock 下等 2 ms"由启动配置判断，不给 `Clock` 加查询方法。判断只在 `WriterConfig::for_startup` 一处。
  - 整批作废后不重试。
  - "只等一次"这一点只能靠代码审阅保证，用例只能以约 96% 的概率测出"去掉等待"的变异。
  - `Batch` 错误的文本是 "no such savepoint"，真正的原因要看出错的那个操作自己的 `Op` 错误。
- 审阅补充（转给 T037）：如果某个操作 panic，写线程会随之结束：事务回滚，在途的回执都得到 `Stopped`，`WriterThread::stop()` 返回 `Err`。宿主应把这种情况当作致命错误，有序退出，不能带着一个死掉的写线程继续运行。已写进 `handoff-notes.md`。
- 规模：新增 1,253 行，在约 1,500 行的上限以内。
- 转交事项：执行者给出的 T025、T026、T027、T032、T037、T047、T069、T121 各条，已写进 `docs/tasks/handoff-notes.md`。
