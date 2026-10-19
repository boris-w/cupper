# T027 汇报

- 执行者：子 agent（Rust 工程师，宿主存储）
- 分支：`feat/T027-store-blob-raw-retention`（worktree `.claude/worktrees/T027`）
- 完成日期：2026-10-19

## 做了什么

blob 存储、原始帧写入 `raw.db`、保留清理三块都做完了，给 T047 的"立即执行一轮清理"入口也留好了。验收标准全部满足，`cargo xtask ci pr` 通过（摘要 `ci/pr-73d073bd2925.json`）。

代码都在 `crates/cupper-host/` 下。

**原始帧（`src/store/raw_writer.rs`）**

- `RawFrame`、`Dir`、`RawSink` 从 `driver/tap` 挪进 store。挪动的原因：队列的消息类型是 store 内部的。tap 和 `acp.rs` 只改了引用。
- **有界队列**：一个 std mpsc，帧的数量用原子计数限制在 `RAW_QUEUE_CAPACITY = 4096`。
  - 推入时队列已满，或者写线程已经停了，就丢弃并计数。推入不会阻塞调用方。
  - 控制消息（裁剪、停止）不占帧的名额，按先进先出排在之前的帧后面。
- **写线程**：`raw.db` 单独一个线程（`store-raw-writer`），在静止屏障里登记为 `store_raw_writer`。
  - 一个事务最多写 `MAX_BATCH`（256）帧。单条 INSERT 失败只丢这一条。事务失败，或者 SQLite 自己结束了事务，整批都计入 `dropped`。
  - 停止请求之后还排着的帧也计入 `dropped`。
- **对外接口**：
  - `RawWriter::spawn(conn, &quiescence) -> (RawWriter, RawWriterThread)`；
  - `RawWriter::sink()`：给 tap 用；
  - `RawWriter::prune(RawRetention { max_age, max_bytes }, now_ms)`；
  - `RawWriterThread::stop()`：先提交停止之前排队的帧，再结束线程。
- **裁剪**：
  - 先删 `ts` 早于期限的帧。
  - 再看在用的字节数，即 `(page_count − freelist_count) × page_size`。超过上限，就按 `ts` 从旧到新每次删 256 行，直到不超过。
  - 最后，空闲页至少占四分之一、而且没有帧在排队时，才 `VACUUM`。

**BlobStore（`src/store/blobs.rs`）**

- 文件路径是 `blobs/sha256/<前 2 位>/<hash>`。
- **写入（`put(data, media_type, expected)`）**：
  - 先写 `blobs/tmp/` 下的临时文件，fsync，改名，再 fsync 目录；然后经写线程 upsert `blobs` 行，刷新 `last_used_at`。
  - 同一内容只存一份。复用已有文件前会先核对它的哈希，所以已有文件坏了，再 `put` 一次就修好了。
  - 超过 `max_blob_bytes`（32 MiB）回 `TooLarge`。带了 `expected` 而哈希不符，回 `Mismatch`，什么都不写。
- **读取（`get`）**：读文件并核对哈希。文件不存在回 `NotFound`，哈希不符回 `Corrupt`。不是 64 位小写十六进制的哈希一律当作不存在，所以拼不出 `blobs/` 之外的路径。
- **可达性**：只看 `blob_refs`。
  - `sweep(grace)` 先删"没有引用、而且上次 `put` 早于宽限期"的行，再删 `sha256/` 下所有没有对应行的文件，并清空 `tmp/`。
  - `put` 和 `sweep` 拿同一把 `tokio::sync::Mutex`，所以清理看到的"有文件没有行"和 `tmp/` 里的文件只能是崩溃留下的。
- **引用（`add_ref(conn, hash, &SessionId)`）**：在调用方自己的写操作里使用。blob 已经不存在时返回 `false`，这时应该让客户端重新上传。

**保留清理（`src/store/retention.rs`）**

各类清理的规则：

| 清理 | 保留 | 何时运行 |
|---|---|---|
| `RawFrames` | 14 天，在用不超过 512 MiB | 启动时，之后每小时 |
| `Audit` | 90 天 | 启动时，之后每天 |
| `Adapters` | 运行中的实例，加最近 100 条已结束的 | 启动时 |
| `OrphanBlobs` | 有引用的；没引用的保留到上次 `put` 后 1 天 | 启动时 |
| `Backups` | 按文件名里的时间戳保留最近 3 份；`host.partial` 和其他文件不动 | 启动时（也就是迁移成功之后） |

接口：

- `Maintenance::run(cleanup)`：立即执行一轮，给 T047 用。
- `Maintenance::tick(&mut Schedule)`：执行到期的清理。
- `Maintenance::spawn(&quiescence)`：基于 `Clock::timer_at` 的循环，在静止屏障里登记为 `store_maintenance`。
- `Schedule`：纯的，只算哪些清理到期、下一个截止时间是什么。

`durations.rs` 新登记 5 个时长：`raw_frames_max_age`、`raw_frames_period`、`audit_max_age`、`audit_period`、`orphan_blob_grace`。

**其他**：`Cargo.toml` 加了 `sha2`（工作区依赖），`Cargo.lock` 只多一行；`store/testutil.rs` 加了测试辅助。

**规模**：新增 1,485 行（不含 `Cargo.lock` 和 `docs/`），其中实现约 900 行，测试和测试辅助约 585 行。比计划的 M 和开工前的估计（约 1,190 行）多，在约 1,500 行的上限之内。每个模块的统计记在 `notes.md`。

## 关键取舍

1. **原始帧另开一个线程，没有放进 `Batcher`。**
   - 交接事项允许两种做法。`raw.db` 的裁剪和 `VACUUM` 可能要几秒，放进 `Batcher` 会挡住 `host.db` 的事件追加。另开线程后，挡住的只有原始帧，队列满了丢弃，这正是设计允许的代价。
   - T024 的组提交一行没动。
   - 这一点和设计 5.2.3 线程表里"存储写线程 1 个（`host.db` 和 `raw.db`）"不一致，见遗留问题第 1 条。
2. **队列用一个 std mpsc 加原子计数，没有用 tokio 的有界 channel。** 写线程是阻塞线程，要同时等帧和控制消息，只能合成一个 channel。
3. **`raw.db` 的大小按在用的页计算。** 删行之后文件不会缩小，按文件大小判断会一直删下去。
4. **"空闲时 `VACUUM`"解释为两个条件同时满足**：空闲页至少占四分之一，并且没有帧在排队。
5. **blob 的写入和清理互斥。** 不加锁时存在一个竞态：清理删了行，新的 `put` 看到文件还在就跳过写入，清理接着删掉文件，最后留下有行没文件的 blob。
6. **周期性清理在启动时也先跑一轮。** 宿主会空闲退出，一个进程可能活不到一天，每天一次的 `audit` 清理可能永远轮不到。
7. **`Schedule` 没有套用 `pure::Core`。** 只有两个周期定时器，也没有输入，`Core` 的输入、动作和 `TimerTable` 都用不上。它仍然是纯的，不读时钟、不做 I/O。
8. **`backups/` 按文件名里的时间戳排序。** 按整个文件名排，`host-v10-…` 会排在 `host-v9-…` 前面。

## 验证方式

- 每个模块提交前都单独跑过：`cargo fmt`、clippy（工作区全特性，以及宿主的默认、`dev-auth`、`test-harness` 三种组合）、`cargo test -p cupper-host`（全特性和默认特性）。
- 在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T027-store-blob-raw-retention/ci/`：rust 844 个通过，protocol 176，ts 232，license 1；UNIT-raw-queue、UNIT-blobstore、UNIT-retention 都是 pass。摘要已提交。

逐条对照验收标准：

- [x] **UNIT-blobstore**（5 个用例）：
  - 写入与去重：内容只存一份，`last_used_at` 被刷新；
  - 读取核对哈希；
  - 文件损坏时报 `Corrupt`，再 `put` 一次修好；
  - 带 `expected` 而哈希不符时报 `Mismatch`，不落盘；
  - 写到一半崩溃：只留下一个残缺的临时文件，`get` 回 `NotFound`，之后的 `put` 正常；
  - 大小上限：32 MiB 接受，再多 1 字节回 `TooLarge`。
- [x] **UNIT-raw-queue**（3 个用例）：
  - 没有消费者时推入 4,098 帧不阻塞，丢弃计数为 2；写线程启动后写入 4,096 行；写线程停止后再推入也计数；
  - 一批最多 256 帧，遇到控制消息就截断；停止请求后面排着的帧计入丢弃；
  - 单条失败只丢这一条。
- [x] **UNIT-retention**（7 个用例，全部用 `ManualClock` 推进时间，不睡眠）：
  - 原始帧 14 天的边界（含边界值，推进 1 ms 之后再核对）；
  - 超过大小上限时先删最旧的，并 `VACUUM`；
  - `audit` 90 天的边界；
  - `adapters` 只留运行中的和最近 100 条已结束的；
  - 孤儿 blob：有引用的和宽限期内的保留；过期的、没有行的文件、`tmp/` 里的残留都删掉；之后对它 `add_ref` 返回 `false`；
  - `backups/` 只留最近 3 份（跨 v9、v10、v11 的版本号，`host.partial` 不动）；
  - 调度：启动时全部运行；差 1 ms 到期时不运行；到期后运行；延迟的一轮只跑一次，下一个周期从实际运行时刻算起。
- [x] CI 摘要已随任务提交。
- [x] 测试名都以用例编号的 snake_case 开头。

另外按变异的思路核对过：比较运算从 `<` 改成 `<=`、去掉 `ORDER BY ts`、去掉 `VACUUM`、去掉引用判断、按整个文件名排序备份、去掉"停止请求之后的帧计入丢弃"，对应的用例都会失败。

## 提交列表

```
dc84f5f docs(task): T027 pr CI summary
73d073b docs(task): T027 notes with the actual line counts
9a18d06 fix(host): count raw frames queued behind a stop request as dropped
a61854c feat(host): retention cleanups on a Clock schedule
8ebe545 feat(host): content-addressed blob store
6c317a4 feat(host): write raw frames to raw.db on a thread of their own
```

## 遗留问题 / 范围外发现

1. **设计需要修订**：5.2.3 的线程表和 5.2.8 的"原始帧"一行，都写成写线程同时持有 `host.db` 和 `raw.db`。现在 `raw.db` 有自己的写线程，建议由编排者改设计。
2. **交接事项**（建议写入 handoff-notes）：
   - **T037**：
     - 组装宿主时，在静止屏障里登记 `store::RAW_WRITER` 和 `store::retention::MAINTENANCE`。
     - 启动顺序：`open_writer(raw.db, Database::Raw)` → `prepare_raw` → `RawWriter::spawn(conn, &quiescence)`，再把 `raw.sink()` 交给 T055；`BlobStore::new(data_dir, writer.clone(), clock)`；`Maintenance::new(writer, raw, blobs, data_dir.join("backups"), clock, durations).spawn(&quiescence)`。
     - `Maintenance::spawn` 要放在"按 `adapters` 表清理残留"之后，否则残留的实例会被当作仍在运行。
     - 有序退出时先停掉清理任务，再 `RawWriterThread::stop()`。
     - 清理失败目前直接吞掉，等下一轮重试，因为宿主还没有日志。有了 `host.log` 之后要记下来。`RawSink::dropped()` 目前也还没有地方展示。
   - **T033**：
     - `BlobStore::put(data, media_type, Some(hash))`：`Mismatch` 对应 `conflict`，`TooLarge` 对应 `too_large`。客户端声明的 `size` 由 T033 自己和 `data.len()` 比对，`put` 不接收这个参数。
     - `get` 每次都读整个文件并核对哈希，`offset`/`length` 由 T033 自己截取。
     - 图片的 10 MiB 限制（按 agent 而定）不在 `BlobStore` 里。
   - **T043、T055**：事件引用 blob 时，在同一个写操作里调用 `store::blobs::add_ref`；返回 `false` 时让客户端重新上传。T055 用 `RawWriter::sink()` 构造 `Tap`。
   - **T047**：`test/run_maintenance` 的 `raw_frames`、`audit`、`orphan_blobs` 对应 `Maintenance::run(Cleanup::…)`；`ledger` 接 T032 的账本。静止屏障要把 `store_raw_writer` 和 `store_maintenance` 这两类参与者算进去。
   - **T032**：`commands` 的 48 小时清理归 T032，本任务没做。两边都会在 `durations.rs` 的同一位置追加时长，合并时两边都保留即可。
3. **合并冲突提示**：本分支改了 `durations.rs`、`store/mod.rs`、`store/testutil.rs`、`crates/cupper-host/Cargo.toml`（加 `sha2`）和 `Cargo.lock`，与同时进行的 T032 可能在这些文件上有冲突，都是追加，保留两边即可。
4. **只有时长能经 `test_config.durations` 覆盖**。`RAW_MAX_BYTES`（512 MiB）、`KEEP_ENDED_ADAPTERS`、`KEEP_BACKUPS` 是常量，进程级测试很难经济地覆盖大小上限；单元测试是直接给 `RawWriter::prune` 传一个小上限。
5. **tap 里删掉了一个用例**：`unit_tap_drops_and_counts_frames_when_the_queue_is_full`。队列挪进了 store，这项行为现在由 UNIT-raw-queue 覆盖，UNIT-tap 的其余用例都还在。
6. **其他小事**：`blobs.last_used_at` 只在 `put` 时刷新，读取不写库。`unit_blobstore_size_limit_is_max_blob_bytes` 在 debug 构建下约 1 秒（要哈希两次 32 MiB）。
