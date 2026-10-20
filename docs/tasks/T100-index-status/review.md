# T100 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：净增 961 行（实现约 563、测试约 398，只算 `crates/`），在约 1,100 的上限之内；开工前估约 925，估得准。
- **CI**：执行者的摘要 `pr-c88c08e11c38.json` 通过。合并 main 之后在本 worktree 重跑 `cargo xtask ci pr --record`，`pr-31102fa9a995.json` 通过（rust 1776、protocol 185、ts 353、license、testdata-scan）。第一次重跑碰上已知的锁用例偶发失败（`host_lock_is_held_once_and_names_its_holder`，与本任务无关，已派 T037 修复 1），之后几次因 GitHub 的 SSL 错误 license 作业失败（网络问题），最后一次全部通过。
- **合并冲突**：7 个文件、11 处（main 上新合进了 T109、T115、T116），由执行者解决，编排者提交。
  - git 曾把 T109 的 `rescanned` 自动并进 `impl AgentNotes`，已挪回 `IndexNotes`。`IndexNotes` 现在是 `created`、`note`、`rescanned` 三个方法。
  - 索引邮箱用 `Status` 取代 `Pending`，保留 `Rescanned`。
  - `lifecycle.rs` 里 T100 的检查改用 T116 抽出的共用函数。
  - 执行者报告 clippy 和 `cargo test -p cupper-host --features test-harness` 全过。

## 关键取舍（逐条答复）

1. **`sessions.last_read_seq` 由 `IndexActor` 写**：认可。已读要对没有加载的会话也生效，会话 actor 只读这一列，按列分写者。设计 5.2.3 的 actor 表已补（main 上 23050f9）。
2. **已读位置截在最近回合结束的 seq**：认可。不截的话，一个很大的 `up_to_seq` 会让以后的回合都显示为已读。
3. **意图的原因**：认可。`cancelled`、`interrupted` 不写意图；`needs_action` 只在待处理数从 0 变成大于 0 时写。
4. **"agent 不可用"按回合失败处理**：认可。否则一个 agent 未登录，它名下所有会话都会显示出错。
5. **标题、归档没有接**：认可，交接 T125。
6. **`agents.updated.git` 留空**：认可。T094 已能检测 git 版本，接进来由显示它的任务（设置页 T036）做，记交接。
7. **摘要写出不重试**，下次启动对账补写：认可。

## 其他

- 状态核心只有一个 `Flush` 定时器，100 ms 合并，相同就不写。索引 actor 的等待点仍然只有一个（`Participant::idle` 里同时等邮箱和 `Alarm`），静止屏障不受影响。
- `NoAgentNotes` 换掉后只剩运行时的测试在用，挪成测试里的本地桩。不留没人用的生产代码，好。

## 范围外发现的归属

- **`StreamWriter::append` 遇到空批次会丢掉行写入**：这是一个容易踩的坑。交给 T089（会话 actor 的恢复会走这条提交路径）：补文档，并在 `bodies` 为空、`with` 非空时报错或照常执行行写入。
- **`session/get` 和索引各算一份状态**：规则相同、实现两份。暂不处理，记交接给 T089；它恢复会话时会碰到 `session/get`。
- **执行者在合并时发现的 T109 缺口**：会话 actor 的 `rescan` 用 `tokio::spawn` 另起任务，没有持有静止屏障的 `Hold`；任务里有数据库读取、git 子进程和 `spawn_blocking`，违反 T047 的规则。交给 T068（进程级信任用例会先碰到，缺陷按 1.2 的上限就地修）。
