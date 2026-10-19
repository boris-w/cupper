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

## 编排者裁定后（方案二，上限约 1,800 行，硬停止线 1,850 行）

按模块分步提交，每个提交带本模块的单元测试，单独通过 fmt、clippy（工作区全特性和宿主三种特性组合）和测试。相对分支起点的累计新增行数（不含 `Cargo.lock`、`docs/`）：

| 提交 | 内容 | 本提交新增 | 累计 |
|---|---|---|---|
| `4b5a7f5` | `VIEW_VERSION` 挪到协议 crate | 8 | 8 |
| `18dfa2b` | shuttle 门面、`StreamView`、流状态与扇出，及其单元测试 | 491 | 499 |
| `aaabc0a` | `store::log`（读时升级、回合快照只写一次、裁剪），测试辅助移入 `testutil` | 497（删 37） | 963 |
| `484c48b` | 单写者与追加三步、注册表、懒加载、淘汰、故障点、`stream_idle` | 683（删 10） | 1,636 |
| `fca7733` | `history/page` 的读取 | 117（删 3） | 1,750 |

按类别：实现约 1,210 行（`streams/mod.rs` 318、`log.rs` 279、`state.rs` 221、`append.rs` 211、`view.rs` 67、`history.rs` 67、`sync.rs` 16，其余是配置、登记和注释）；单元测试和测试辅助约 540 行（`streams/tests.rs` 193、`state.rs` 137、`log.rs` 86、`testutil.rs` 77、`history.rs` 47）。

中间提交里有两处临时的 `#[cfg_attr(not(test), expect(dead_code, ...))]`：`streams::state` 在注册表那次提交之前没人用，`log::turn_ops`、`log::turn_before` 在 `history` 那次提交之前没人用。用到它们的提交里都已去掉，`HEAD` 上没有。

变异检查：检查点作废时少取一个回合、空闲判断由 `<` 改成 `<=`，相应的用例都失败了。

### 合入 main 之后还剩的部分（估计）

| 剩余 | 估计 |
|---|---|
| `store::history::Cursor` 换成协议的 `HistoryCursor` | 约 -6 |
| xtask 的 shuttle 步骤、`proptest` 开发依赖、`mod shuttle` 声明 | 约 15 |
| PROP-stream-seq（复用 `state` 测试里的 `Collect` 和 `batch`） | 约 80 |
| PROP-stream-replay（脚本生成器约 60，三条性质约 90） | 约 150 |
| 第二步与第三步之间崩溃的用例 | 约 50 |
| 合计 | 约 290 |

预计总计约 2,040 行，超过硬停止线 1,850 约 190 行。超出的主要原因是单元测试和夹具比估计的多（rustfmt 展开后约 540 行，开工前估约 200 行）。
