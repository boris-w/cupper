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

（每完成一个模块补一行。）
