# T026 工作笔记

## 开工前的行数估计

计划估为 L，上限约 1,500 行。按模块估（含测试，不含 `Cargo.lock`、CI 摘要和 `docs/`）：

| 模块 | 实现与文档 | 测试与测试辅助 |
|---|---|---|
| `cupper-protocol::view` 加 `VIEW_VERSION`，`store::upcast` 改用它 | ~15 | — |
| `store/log.rs`：`events`、`checkpoints`、`turn_snapshots` 的读写，读时升级，裁剪 | ~200 | — |
| `store/streams/sync.rs`：`cfg(shuttle)` 门面 | ~15 | — |
| `store/streams/view.rs`：索引、会话两种视图的统一外壳（fold、JSON） | ~60 | — |
| `store/streams/state.rs`：流状态、订阅、扇出接口（`Sink`）、第三步 | ~200 | — |
| `store/streams/append.rs`：单写者、追加三步、回合结束的检查点和回合快照 | ~170 | — |
| `store/streams/mod.rs`：注册表、懒加载、淘汰 | ~200 | — |
| `store/history.rs`：`history/page` 的读取 | ~70 | — |
| `durations.rs`、`failpoint.rs`、`store/mod.rs`、`Cargo.toml`、工作区 lint、xtask 的 shuttle 步骤 | ~40 | — |
| PROP-stream-seq（shuttle） | — | ~110 |
| PROP-stream-replay（proptest，真实存储路径，含脚本生成器）和崩溃用例 | — | ~320 |
| 单元测试：订阅、慢订阅者、单写者、淘汰、历史游标、检查点作废、读时升级、测试夹具 | — | ~200 |
| 合计 | ~970 | ~630 |

总计约 1,600 行，比上限多一点，属于"约 1,500"的范围。写的时候要省：测试夹具共用，生成器只覆盖宿主的不变式需要的几种事件。如果做到一半预计超过 1,800 行，就停下来汇报，拆成"流与扇出"和"检查点、回合快照与历史"两个任务。

## 实际行数与超限预警（2026-10-19，实现写完、测试未写时）

相对分支起点（`bbd309c`）的新增行数，不含 `Cargo.lock` 和 `docs/`：

| 模块 | 新增 | 估计 |
|---|---|---|
| `VIEW_VERSION` 挪到 `cupper-protocol::view`（已提交 `4b5a7f5`） | 8 | ~15 |
| `store/log.rs` | 274 | ~200 |
| `store/streams/mod.rs`（注册表、懒加载、淘汰、`StreamError`、配置） | 336 | ~200 |
| `store/streams/append.rs`（单写者、追加三步、回合结束） | 241 | ~170 |
| `store/streams/state.rs`（流状态、订阅、`Sink`、第三步） | 218 | ~200 |
| `store/streams/view.rs`、`sync.rs` | 83 | ~75 |
| `store/history.rs` | 63 | ~70 |
| `durations.rs`、`failpoint.rs`、`store/mod.rs`、`Cargo.toml`、工作区 lint | 22 | ~40 |
| 实现合计 | 1,245 | ~970 |

实现比估计多约 270 行，主要是 rustfmt 展开和文档注释。还没写的部分：

| 剩余 | 估计 |
|---|---|
| xtask 的 shuttle 步骤、`0001` 里 `turn_snapshots.v` 的注释 | ~15 |
| PROP-stream-seq（shuttle） | ~100 |
| PROP-stream-replay（夹具、脚本生成器、三条性质） | ~230 |
| 第二步与第三步之间崩溃的用例（故障点、子进程） | ~60 |
| 单元测试：订阅的两种模式和慢订阅者、单写者、淘汰、历史游标、检查点作废、读时升级 | ~150 |
| 合计 | ~555 |

预计总计约 1,800 行，超出上限约 20%。即使把 `Batch` 的构造方法改成公开字段、压缩文档，也在 1,700 行左右。按守则第 8 条停下来汇报。

实现已能编译，三种特性组合下 clippy 都干净，原有测试全部通过；新代码还没有测试，所以没有提交，留在 worktree 里。

### 拆分建议

- **方案一：拆成两个任务。**
  - T026a 流与扇出（留在本分支）：`cfg(shuttle)` 门面、`StreamView`、流状态（订阅、`Sink`、第三步）、注册表（懒加载只读事件、淘汰）、单写者和追加三步（含 `Batch::with`）、`log.rs` 里事件的写入和读取、故障点和崩溃用例、PROP-stream-seq 和 xtask 的 shuttle 步骤、`VIEW_VERSION` 挪动（已完成）。估计实现约 850 行，测试约 330 行，合计约 1,180 行。
  - T026b 检查点、回合快照与历史（依赖 T026a）：回合结束同事务写检查点和不可变的回合快照再 `trim`、索引流每 1,000 条一个检查点、检查点作废时用最近 20 个回合快照重建、按 5.4.5 裁剪、`history/page`、`turn_snapshots.v` 的含义、PROP-stream-replay。估计实现约 400 行，测试约 330 行，合计约 730 行。
  - 代价：分界线穿过追加的第一、二步和加载路径，T026a 要先拿掉检查点那一支，T026b 再加回来；存储链上多一轮派发和审阅。
- **方案二：不拆，上限提到约 1,800 行。** 理由是计划 2.1 对 T026 的说明："流状态、检查点、回合快照共用'追加三步'和'回合快照写入后不可变'两条不变式，PROP-stream-replay 必须同时覆盖三者"。三者本来就在同一条追加路径和同一个加载函数里，代码已经写完，剩下的主要是测试。
