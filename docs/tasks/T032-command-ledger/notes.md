# T032 工作笔记

## 开工前的行数估计

计划估为 M（400–900 行），上限约 1,500 行。全部放在 `crates/cupper-host/src/ledger/`；`lib.rs` 只加一行，`durations.rs` 只在登记表里附加两项；`store/` 不改。

### 模块划分

| 文件 | 内容 |
|---|---|
| `mod.rs` | 模块文档（三种状态、调用顺序、与事件同事务）；`Ledger`（`std::Mutex` 下的内存表、写线程、`Clock`）；`load`（启动时从 `commands` 表加载 48 小时内的记录）；`begin`、`lookup`；`LedgerError` 和它到 `RpcError` 的映射；过期清理（`sweep`，加上经 `Clock` 定时、每小时一次的循环） |
| `ticket.rs` | `Ticket`（`begin` 给出的执行权）：`mark_external`（`pending_external`，`Durability::Full`）、`complete`、`reject`、丢弃时的处理；`Completion`：给 `Batch::with` 的行写入、`committed`、单独提交的 `commit`；`Waiter`（进行中的重复请求等待第一次执行的结果） |
| `row.rs` | `commands` 表的读写：upsert、删除、按时间清理、启动加载；`result` 列的 JSON 形态 |
| `hash.rs` | 参数哈希：去掉 `command_id`、`attestation`、`input` 后用 `canonical::digest` |
| `tests.rs` | UNIT-ledger |

### 估计

按 T026、T031 的密度（文档注释约占代码的 30%，rustfmt 展开长参数列表）：

| 部分 | 实现与文档 | 测试 |
|---|---|---|
| `mod.rs` | ~230 | — |
| `ticket.rs` | ~190 | — |
| `row.rs` | ~100 | — |
| `hash.rs` | ~25 | — |
| `lib.rs`、`durations.rs` | ~10 | — |
| `tests.rs`：夹具（内存里的 `host.db` 加写线程；同事务用例用临时目录和 `Streams`） | — | ~70 |
| `tests.rs`：首次执行与重放、参数不同、进行中的重复、执行前失败不记录、重启后 `outcome_unknown`、48 小时过期（`ManualClock` 驱动清理循环）、哈希不受三个字段和键顺序影响、principal 互不影响、错误码映射 | — | ~295 |
| `tests.rs`：与首批事件同事务（`Batch::with`），提交成功两者都在、失败两者都不在 | — | ~70 |
| **合计** | **~555** | **~435** |

总计约 990 行，高于计划的 M，低于约 1,500 行的上限。高出的原因：测试按验收标准逐条覆盖（7 条 UNIT-ledger 加同事务用例），同事务用例要搭真实的写线程和流。

- **停止线**：每完成一个模块就用 `git diff --numstat` 统计一次。按已完成模块的实际偏差推算，总数预计超过约 1,400 行时，停下来汇报。
- 压缩手段：测试共用一个夹具；参数哈希的用例表驱动。

## 实际行数

按 `git diff --numstat $(git merge-base main HEAD) HEAD -- crates/` 统计。实现写完、测试没写时统计过一次：实现 628 行（估计 555），按这个偏差推算总数约 1,120 行，低于停止线，于是继续写测试。

| 部分 | 提交 | 实现与文档 | 测试 | 估计 |
|---|---|---|---|---|
| `mod.rs` | `cb9a6de`、`fa60dcf` | 306 | — | ~230 |
| `ticket.rs` | `cb9a6de` | 181 | — | ~190 |
| `row.rs` | `cb9a6de`、`fa60dcf` | 108 | — | ~100 |
| `hash.rs` | `cb9a6de` | 25 | — | ~25 |
| `lib.rs`、`durations.rs` | `cb9a6de`、`fa60dcf` | 7 | — | ~10 |
| `tests.rs`：夹具和各项 UNIT-ledger | `cb9a6de`、`fa60dcf` | — | 404 | ~365 |
| `tests.rs`：与首批事件同事务 | `251eec0` | — | 71 | ~70 |
| **合计** | | **627** | **475** | ~990 |

合计 1,102 行，比估计多 11%，在约 1,500 行的上限之内。`mod.rs` 多出的约 75 行是模块文档（三种状态、调用顺序、执行者结束一次执行的三种方式）和 `lookup`、`Repeat` 两个对外类型。

## 过程记录

- **测试的存储**：用真实的 `host.db`、写线程和读连接池（临时目录借用 `gateway::testkit::TempDir`，`store::testutil` 是 `store` 私有的，不为此改 `store/mod.rs`）。"重启"就是在同一个库上重新 `Ledger::load`。
- **一次竞态**：`reject` 删除 `pending_external` 行时不等待回执。用例最初经读连接池读表，第一次运行就读在了删除提交之前。改成经写线程读（写线程按提交顺序执行），之后连跑 8 次都通过。
- **变异检查**：逐个改坏下面 8 处，每次都有用例失败，改回后全部通过：参数哈希不去掉 `input`；清理用 `<` 代替 `<=`；加载用 `>=` 代替 `>`；不比较方法名；`pending_external` 的执行被丢弃时不标"结果未知"；等待者拿到的结果不带 `replayed`；清理不动内存表；`reject` 不删 `pending_external` 行。
- **范围外**：`cargo doc -p cupper-host` 有一条早已存在的警告（`driver::tap` 的公开文档链接到私有项 `redact`），不是本任务引入的，没有改。
