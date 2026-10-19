# T026 汇报

- 执行者：子 agent（Rust 工程师，宿主存储）
- 分支：`feat/T026-store-streams`
- 完成日期：2026-10-19

## 做了什么

- **`VIEW_VERSION` 挪到 `cupper-protocol::view`**（编排者裁定）。`store::upcast` 保留 `checkpoint_fate`，常量改从协议 crate 引用。
- **`store::streams`**，由 SP-4a 的 `hub` 重构而来。
  - **门面**：`sync.rs` 在 `cfg(all(test, shuttle))` 时换成 shuttle 的锁，其余构建一律用 std 的锁。
  - **流状态**（`state.rs`）：head、first_seq、检查点位置、视图、订阅者、单写者标记，都在流锁里。锁内只做内存操作。
  - **订阅**：在锁内调 `subscribe_mode`，并在登记订阅者之前，于锁内调用 `start` 回调。T033 在这里把响应、快照页或补发标记放进发送队列。
  - **扇出接口**：`Sink::push` 返回 `Queued`、`Full` 或 `Closed`。返回 `Full` 时摘掉这个订阅者并调用 `Sink::reset`（`slow_consumer`）；返回 `Closed` 时直接摘掉。事件只序列化一次（`Published`），日志和所有订阅者共用。
  - **追加三步**（`append.rs`，`StreamWriter`）：
    - 第一步不持锁：分配 seq，`v` 取 `Upcasters::host().current`。批里有 `turn.completed` 时，在视图副本上依次应用，回合结束时先 `turn_ops` 再 `trim`。
    - 第二步：写线程用一个事务写入事件、不可变的回合快照、检查点（批末尾，带 `view_v`）、裁剪，以及 `Batch::with` 给的行（账本可以把命令结果放进同一事务）。
    - 第三步：在锁内换入视图、推进 head、扇出。第二、三步之间放了故障点 `store.before_publish`。
    - 索引流每 1,000 条写一次检查点。
  - **注册表**（`mod.rs`）：
    - 懒加载：在读池的一个读事务里读检查点和其后的事件。`view_v` 不符时，会话流用最近 20 个回合快照重建，索引流用日志里还留着的事件重建。
    - 淘汰：句柄、写者和订阅都持有 `StreamHandle`；最后一个放掉后，空闲满 `stream_idle`（10 分钟，已登记在 `durations.rs`）就移出内存，每次 `open` 时清理。
    - 并发加载靠淘汰计数，防止登记过期的状态。
- **`store::log`**：三张表的读写。
  - 读出的事件和回合快照的 op 都先升级。
  - 回合快照用纯 INSERT 写，第二次写会失败。
  - 按 5.4.5 裁剪，裁剪点停在最早的一条"年轻"事件之前，保留下来的日志保持连续。
  - `read_events` 是公开的，供 T033 补发。
- **`store::history::page`**：输入、输出都是协议的 `HistoryCursor`。一页一个回合，用 `paginate` 分页，`op_index` 是页的起点。
- **测试**：
  - PROP-stream-seq：shuttle 穷举。
  - PROP-stream-replay：proptest，走真实存储路径。
  - 第二步与第三步之间崩溃的用例。
  - 各模块的单元测试。
  - xtask 的 rust 作业加了 `test-host-shuttle` 一步。

## 关键取舍

- **`turn_snapshots.v`**：定为 `ops` 的版式，`TURN_SNAPSHOT_LAYOUT = 1`，即 `{type, v, data}` 的数组。每个 op 带自己类型的 `v`，读时逐个升级；读到别的版式报 `Corrupt`。表结构不变，`0001` 的注释已改。
- **检查点为什么在第一步算**：只在批里有 `turn.completed` 时复制视图，在副本上应用后，第三步直接换进去。这样检查点、回合快照和事件落在同一个事务里，第二步之后崩溃也不会出现"回合已结束、却没有快照"的状态。平时的批照常在锁内应用，不复制。
- **视图拒收的事件**：照样写入、照样扇出，每次加载也同样跳过，所以内存和重放的结果一致。这些事件放在 `Appended::rejected` 里，交给写者记日志。
- **淘汰不靠定时器**：每次打开流时按 `Clock` 判断有没有超过空闲时长，不需要再登记一个 actor。
- **shuttle 的跑法**：`RUSTFLAGS=--cfg shuttle`，用单独的 `target/shuttle`，免得冲掉平时构建的缓存。测试模块叫 `interleavings`，免得在 `streams` 里遮住同名 crate。第一版用一把 shuttle 锁模拟日志，DFS 十分钟没跑完；去掉这把锁后穷举约 6 秒。"先提交、后发布"由追加的写法保证，这个用例只验证锁内的不变式。
- **规模**：
  - 最终新增 2,041 行、删除 14 行（相对合入的 main，不含 `Cargo.lock` 和 `docs/`）。其中实现约 1,215 行，测试和测试辅助约 826 行。
  - 中途两次超限预警，按裁定把上限提到约 2,050 行。
  - 超出估计的主要是测试和夹具经 rustfmt 展开后的行数；没有删用例。
- **新依赖**：
  - `shuttle = "=0.9.7"`（Apache-2.0，开发依赖）及其闭包，`cargo deny` 通过。闭包包括 `shuttle-engine`、`shuttle-schedulers`、`shuttle-std`、`corosensei`、`bitvec`、rand 0.8 等。
  - `proptest` 用工作区已有的版本。
  - rusqlite 开了 `fallible_uint`。

## 验证方式

- **PROP-stream-seq（shuttle）**：
  - 场景：`check_dfs` 穷举一个写者发布两批、一个快照订阅和一个补发订阅的全部调度。
  - 断言：任何订阅者看到的 seq 都连续、不重复，快照正好停在 `at_seq`。
  - 变异检查：把订阅改成"放锁之后才登记"，shuttle 找到了失败的调度。
- **PROP-stream-replay**：生成符合宿主不变式的会话脚本，最多 30 个回合，让回合真正离开窗口；经 `StreamWriter` 随机分批写入真实的库，保留条数取小值。断言：
  - 内存里的流、检查点加其后事件的重新加载（裁剪已发生）、从头 fold 整个日志，三者一致；
  - 检查点停在最后一个结束回合的批末尾，并且确实裁剪过；
  - 快照加全部 `history/page`，等于从头重放整个日志。
  - 变异检查："历史页丢掉大回合的后几页""检查点少记一个 seq"都能测出来。第一版这两项都没测出来，原因是回合太少，历史为空；已经改正，见 `notes.md`。
- **第二步之后、第三步之前崩溃**（`prop_stream_replay_crash_between_commit_and_publish_loads_the_batch`，test-harness）：
  - 子进程在回合结束那一批提交后，于 `store.before_publish` 退出，退出码 99。
  - 重启后加载，视图等于 fold 全部事件，head 为 5；检查点在 5，回合快照在；`PRAGMA integrity_check` 为 ok。
- **单元测试**：
  - 订阅的两种模式、慢订阅者被重置、单写者；
  - `Batch::with` 失败时整批不提交，seq 会重新发出；
  - 淘汰的时间边界；
  - 检查点作废时的重建（会话流和索引流）；
  - 索引流每 1,000 条一个检查点；
  - 读时升级出错；
  - 裁剪保持连续；
  - 回合快照只能写一次；
  - 历史游标的翻页。
  - 变异检查："重建时少取一个回合""空闲判断由 `<` 改成 `<=`"，都能测出来。
- **CI**：在干净的 worktree 里跑了 `cargo xtask ci pr --record docs/tasks/T026-store-streams/ci/`。
  - rust 581、ts 64、license 1，全部通过；其中 `test-host-shuttle` 一步通过，用时 11.2 秒。
  - 摘要 `ci/pr-7bd06ca90321.json` 里有 PROP-stream-seq 和 PROP-stream-replay，均为 pass；已提交。
  - 本地另外用 `--cfg shuttle` 跑了一遍 clippy，没有警告。
  - 用例名都以编号的 snake_case 开头。
- **约束核对**：
  - 没有 `unsafe`；时间只经 `Clock` 获取；SQLite 只在写线程或读池上运行。
  - 每个提交都单独通过 fmt、clippy（工作区全特性和宿主三种特性组合）和测试。
  - 中间提交里临时加过的 `expect(dead_code)`，到 `HEAD` 已全部去掉。

## 提交列表

```
054d1d3 docs(task): record the T026 pr CI summary
7bd06ca docs(task): record the T026 final line counts and trade-offs
2e5f1e0 test(store): add PROP-stream-replay on the real log and the crash case
c64d56c test(store): add PROP-stream-seq under shuttle and run it in CI
d11d7e3 refactor(store): page history by the protocol's HistoryCursor
e74785d Merge branch 'main' into feat/T026-store-streams
926061c docs(task): record the T026 per-commit line counts and what remains
fca7733 feat(store): read history pages from the turn snapshots
484c48b feat(store): append through a stream's one writer and keep a stream registry
aaabc0a feat(store): read and write the event log, checkpoints and turn snapshots
18dfa2b feat(store): add the stream state, its subscriptions and the fan-out
50efd6d docs(task): record the T026 line counts and the size warning
4b5a7f5 refactor(store): move VIEW_VERSION next to the views in cupper-protocol
```

## 遗留问题 / 范围外发现

- **设计文档**：5.4.2 里 `turn_snapshots.v` 的注释、5.2.8 里"索引流每 1,000 条一个检查点"，请编排者按裁定改。
- **给 T033**：
  - 网关的发送队列实现 `Sink`，它的两个方法都在流锁内调用，只能排队，不能回调流。
  - 订阅时把响应、快照页或补发标记放进 `subscribe` 的 `start` 回调里。
  - 补发用 `store::log::read_events(conn, stream, after, head, ..)`。
  - `history/page` 用 `store::history::page`；不带游标时，用 `StreamHandle::read` 取窗口里最早的 `turn_no`；`max_bytes` 自己截到 48,000。
  - 补发时读到已被裁掉的事件，按 `cursor_expired` 处理（编排者已转出）。
- **给 T032 和 T043**：
  - 命令结果和它产生的事件放进同一个 `Batch`，用 `with`。
  - `Appended::rejected` 不为空说明写者违反了视图规则，要记日志。
  - 检查点作废后，会话配置、能力和队列要由 T043 重发（编排者已转出）。
- **给 T037**：组装时用 `Streams::new(writer, readers, clock, StreamsConfig::new(epoch, &durations))`。
- **CI 的局限**：
  - CI 只用 `--cfg shuttle` 跑测试，不跑这一配置下的 clippy；我在本地跑过，干净。
  - shuttle 这一步会覆盖调用方环境里的 `RUSTFLAGS`。
