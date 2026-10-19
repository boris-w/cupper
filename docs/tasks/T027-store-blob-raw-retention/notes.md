# T027 工作笔记

## 开工前的行数估计

计划估为 M（400–900 行），硬上限约 1,500 行。按模块估（新增行数，含测试，不含 `Cargo.lock`、CI 摘要和 `docs/`）：

| 模块 | 实现与文档 | 测试 |
|---|---|---|
| `store/raw_writer.rs`：`RawFrame`、`Dir` 从 `driver/tap` 挪来；有界队列 `RawSink`；`raw.db` 的写线程（批量写入、按时间和大小裁剪、空闲时 `VACUUM`、停止） | ~280 | — |
| `driver/tap`、`driver/acp.rs` 改用挪走的类型 | ~15 | — |
| `store/blobs.rs`：`BlobStore`（写临时文件、fsync、改名；去重；读取核对哈希；大小上限；孤儿清理）、`add_ref` | ~200 | — |
| `store/retention.rs`：`Maintenance`（各类清理、立即执行一轮的入口、`Clock` 定时的循环）、纯的 `Schedule`、`backups/` 只留 3 份、`adapters` 只留 100 条已结束 | ~200 | — |
| `durations.rs` 登记 5 个时长；`store/mod.rs`；`Cargo.toml` 加 `sha2` | ~30 | — |
| UNIT-raw-queue | — | ~130 |
| UNIT-blobstore | — | ~130 |
| UNIT-retention（逐类核对、`Schedule`、`ManualClock` 推进） | — | ~200 |
| 合计 | ~725 | ~460 |

总计约 1,190 行，超出 M 的估计，但在约 1,500 行的上限之内。T026 的经验是实现会比估计多约 25%（rustfmt 展开和文档注释），照此约 1,350 行，所以要省：测试夹具共用，不写只为测试存在的开关。做到一半预计超过 1,500 行就停下来汇报。

## 取舍

### 原始帧另开一个写线程（`store-raw-writer`），不放进 `Batcher`

交接事项允许两种做法。选另开线程，原因：

- **隔离**：`raw.db` 的批量写入、按大小裁剪和 `VACUUM` 都可能耗时（512 MB 的 `VACUUM` 是秒级）。放进 `Batcher` 就会挡住 `host.db` 的事件追加；另开线程时，挡住的只是原始帧，队列满了丢弃并计数，正是设计允许的代价（ADR-06：原始帧只用于重算和排障）。
- **不动 T024 的组提交**：`Batcher` 的攒批、savepoint、回执逻辑和测试保持原样。
- "每个数据库一个事务"天然成立：一个线程只有一个连接。

和设计 5.2.3 线程表"存储写线程 1 个（`host.db` 和 `raw.db`）"不完全一致，汇报里提出，由编排者决定是否修订设计。

### 队列：一个 std mpsc，帧数用原子计数限住

写线程要同时等两种消息：分接层送来的帧，和清理、停止这类控制消息。阻塞线程上没法同时等 tokio 的有界 channel 和另一个 channel，所以合成一个 std mpsc：

- 帧：先在原子计数上占一个位置，超过 4,096 就丢弃并计数，不阻塞调用方；写线程取出帧时归还位置。
- 控制消息不占帧的名额，永远能送达，按 FIFO 排在之前的帧后面（裁剪前，之前的帧已提交）。

`RawSink` 因此从 `driver/tap` 挪到 store：它的 channel 元素是 store 内部的消息类型。`RawFrame`、`Dir` 是 `raw_frames` 的行，一起挪过来。

### 写失败也算丢弃

单条 INSERT 失败只回滚这一条（SQLite 的语句级原子性），计入 `dropped`；`BEGIN` 或 `COMMIT` 失败，整批计入 `dropped`。宿主还没有日志设施，计数是唯一的出口。

### `raw.db` 的"总大小"按在用的页计

删行之后文件不会变小（空闲页留在 freelist 里，直到 `VACUUM`），按文件大小判断会一直删下去。所以用 `(page_count − freelist_count) × page_size`，超过上限就按 `ts` 从旧到新每次删 256 行，直到不超过。

"空闲时 `VACUUM`"：裁剪之后，空闲页至少占四分之一，并且队列里没有等着写的帧，才 `VACUUM`。

### blob：可达性看 `blob_refs`，写入和清理串行

- `blob_refs(hash, owner)` 是唯一的可达性依据；没有引用、`last_used_at` 早于宽限期（1 天）的 blob 才回收。
- 写入顺序：先写文件（临时文件、fsync、改名、fsync 目录），再经写线程 upsert `blobs` 行（刷新 `last_used_at`）。
- 清理：先经写线程删掉"没有引用并且过了宽限期"的行，再删掉 `blobs/sha256/` 下所有没有行的文件，并清空 `blobs/tmp/`。
- `put` 和清理拿同一把 `tokio::sync::Mutex`，整个过程互斥。所以清理看到的"有文件没有行"只能是崩溃留下的（改名之后、写行之前崩溃），"`tmp/` 里的文件"也只能是崩溃留下的，都可以直接删。不拿锁的话，"清理删行 → 新的 `put` 看到文件已在就跳过 → 清理删文件"会留下有行没文件的 blob。
- `add_ref` 只在 `blobs` 有这一行时才写入，返回是否写入。行已被清理时调用方拿到 `false`，让客户端重新上传，而不是留下指向不存在 blob 的引用。
- 去重时核对已有文件的哈希，不只看是否存在：已有文件坏了，同样内容再 `put` 一次就修好了。
- 读取不查表，只读文件并核对哈希；文件名就是哈希，格式不对（不是 64 位小写十六进制）的一律当作不存在，所以拼不出 `blobs/` 之外的路径。
- `last_used_at` 只在 `put` 时刷新，读取不写库。

### 清理的时机

- 设计 5.4.5 的周期：`raw.db` 每小时，`audit` 每天；孤儿 blob、`adapters`、`backups/` 在启动时。
- 宿主会空闲退出（T037），一个进程可能活不到一天，所以周期性的清理在启动时也先跑一轮，之后按周期。
- `Schedule` 是纯的：给定 `MonoTime`，算出到期的清理和下一个截止时间，不读时钟、不做 I/O。`Maintenance::tick` 读 `Clock`、执行到期的清理；`serve` 只是 `tick` 加 `Clock::timer_at` 的循环。
- 没有套用 `pure::Core`：只有两个周期定时器、没有输入，`Core` 的输入、动作、`TimerTable` 都用不上。
- `adapters` 的清理要排在 T037"按 `adapters` 表清理残留"之后（那一步会把残留的实例标为已结束）。

### 不做的部分

- `commands` 的 48 小时清理归 T032（它的 brief 第 4 条），`test/run_maintenance` 的 `ledger` 由 T047 接到账本上。
- 运行时旧版本归运行时安装器（5.2.12）；日志轮转归 5.11。
- blob 按会话回收是 M2。

### `backups/` 排序

文件名是 `host-v<版本>-<YYYYMMDDTHHMMSSZ>.db`。按整个文件名排序时 `host-v10-…` 排在 `host-v9-…` 前面，所以按最后一个 `-` 之后的时间戳排序（同一时间戳再按文件名）。`host.partial` 和其他文件不动。
