# T024 汇报

- 执行者：子 agent（Rust 工程师，宿主存储）
- 分支：`feat/T024-store-writer`
- 完成日期：2026-10-19

## 做了什么

写线程、读池、两档持久性和 `data_epoch` 判定都做完了。验收标准全部满足，`cargo xtask ci pr` 通过。

代码都在 `crates/cupper-host/src/store/`。另外改了三处：
- `Cargo.toml`：加了依赖。
- `lib.rs`：加了一行 `pub mod store;`。
- `durations.rs`：登记了一个时长。

**连接与持久性**（`store/connection.rs`）
- `open_writer(path, Database::Host | Raw)`：`journal_mode=WAL`，`synchronous=NORMAL`，`fullfsync=OFF`，`foreign_keys=ON`，`busy_timeout=5000`。`host.db` 开 `checkpoint_fullfsync`，`raw.db` 不开。
- `open_reader(path)`：只读打开，`busy_timeout=5000`。
- `Durability::{Normal, Full}`：`Full` 就是 `synchronous=FULL` 加 `fullfsync`。

**写线程**（`store/writer.rs`）
- 用法：`Writer::spawn(conn, WriterConfig, &Quiescence) -> (Writer, WriterThread)`。`Writer` 可以克隆，`submit(durability, |conn| ...)` 返回 `Receipt<T>`。回执既可以 `.await`，也可以在线程里 `wait()`。
- **组提交**：最多攒 256 个操作（`MAX_BATCH`），攒满或队列为空就提交。队列空了以后再等一次，最长 `linger`。
- **savepoint**：一批是一个事务，每个操作一个 savepoint。
  - 操作自己出错，只回滚它的 savepoint，它的回执是 `WriteError::Op`，同批其他操作照常提交。
  - 写线程自己的语句失败（BEGIN、SAVEPOINT、RELEASE、ROLLBACK TO、COMMIT），或者 SQLite 在 I/O 错误后自己结束了事务，整批作废。批里每个操作都得到 `WriteError::Batch`，包括还没轮到执行的；自己已经出错的操作仍是 `Op`。
  - 写线程停止后，回执是 `WriteError::Stopped`。
- **持久性**：每批按批内最高的持久性提交，需要时在 BEGIN 之前切换 pragma。
- **回执**：整批有结果之后才回复。
- **静止屏障**：`WRITER = ActorKind::new("store_writer")`。`spawn` 在调用方线程里 `enter`，`Participant` 一直持有到线程结束。
- **停止**：`WriterThread::stop()` 先提交它之前提交的操作，再结束线程并 join。
- **2 ms**：`WriterConfig::for_startup(&payload, &durations)`。`test_config.clock` 为 `manual` 时 `linger` 为 0；否则取 `durations.rs` 新登记的 `STORE_BATCH_LINGER`（`store_batch_linger`，2 ms）。

**读池**（`store/readers.rs`）
- `Readers::open(path)` 打开 4 个只读连接。
- `read(|conn| ...)` 先用信号量等到空闲连接，再借出连接，在 `spawn_blocking` 里执行。
- 借出的连接放在一个 drop guard 里：读操作 panic 时连接照样归还，panic 再在调用方重新抛出。

**`data_epoch`**（`store/epoch.rs`）
- 纯函数 `decide(LastExit, Boot) -> EpochStep`：只有"异常退出 × 重启过"才返回 `Bump`，`kill -9` 不加 1。
- `LastExit::from_meta(last_start_at, last_clean_exit_at)`、`Boot::since(meta 里的 boot_session_uuid, current)` 从 meta 的值得出两个输入。
- `current_boot_session(&payload)`：test-harness 构建里如果给了 `test_config.boot_session_uuid` 就用它，否则读 `cupper_sys::boot_session_uuid()`。
- `BootSession` 按 UUID 解析，大小写不敏感，输出与内核相同的大写格式，用来写进 meta。

**依赖**
- `rusqlite = "=0.40.2"`：bundled（SQLite 3.53.2），`default-features = false`，2026-08-08 发布。
- `cupper-sys`：path 依赖。

## 关键取舍

1. **"只在 SystemClock 下等 2 ms"由启动配置判断，不给 `Clock` 加方法。**
   - 这 2 ms 本来就是 5.2.16 里唯一不经 `Clock` 的等待，写线程也不持有 `Clock`。
   - 给 `Clock` 加"是否虚拟时间"的查询，会让别的模块写出测试和生产走不同路径的代码，违背 `Clock` 抽象的本意。
   - 判断集中在 `WriterConfig::for_startup` 一处，T037 调用即可。
2. **"只等一次"**：用 `recv_timeout(linger)` 实现，不按截止时间计算，因为 `Instant::now` 被禁用。额外延迟最多等于 `linger`。停止请求也会结束等待。
3. **切换持久性必须在 BEGIN 之前。** 已实测：事务里改 `synchronous` 会报 "Safety level may not be changed inside a transaction"。写线程记住当前档位，只在需要时切换，失败了下一批会重试。
4. **操作是通用闭包 `FnOnce(&Connection) -> rusqlite::Result<T>`。** 追加事件、upsert 这类具体操作由 T026 等在上面封装。错误类型用 rusqlite 自己的，保持有类型的错误。
5. **整批作废时不重试剩下的操作。** 这种情况意味着磁盘满或 I/O 出错，规则简单，结果也确定。
6. **rusqlite 关掉默认特性。**
   - 起因：默认的 `cache` 特性会引入 foldhash（Zlib），我这个分支的 allowlist 里没有 Zlib。
   - 我没有改 `deny.toml`：按 ADR-11，那需要你来审。
   - 后来发现 main 已经因 T050 的审阅加入了 Zlib，所以把 `Cargo.toml` 的注释改成了不会过时的理由：存储层不用语句缓存，另一个默认特性只对 wasm 有用。T026 需要 `prepare_cached` 时再打开 `cache`。
7. **epoch 的边界情况：**
   - `last_clean_exit_at == last_start_at` 算正常退出：ManualClock 下这两个时间可能落在同一毫秒。
   - 上次的启动会话 UUID 缺失或解析不了，按"重启过"处理：最坏只多一次全量快照。
8. **raw.db 只做了 pragma 配置（`Database::Raw`）。** 写线程目前只写 `host.db`；原始帧的有界队列归 T027。
9. **没有加故障点**：M1a 的故障点归 T069。

## 验证方式

- **CI**：在干净的 worktree、提交 bd13113 上跑 `cargo xtask ci pr --record docs/tasks/T024-store-writer/ci/`，通过：rust 294 项、ts 64 项、license 1 项。
  - `UNIT-writer-savepoint`、`UNIT-epoch-decision` 都是 pass。
  - 摘要 `pr-bd13113ca51c.json` 已提交，内容里没有路径和用户名。
  - 编排者提醒的 `cupper-sys` 偶发失败，这次没有出现。
- **对照验收标准：**
  - [x] **UNIT-writer-savepoint**：
    - 同一批里一个操作失败（它先插入 2，再插入重复的 1），插入的 2 随它的 savepoint 一起回滚。其余两个操作提交，表里是 `[1, 3]`。每张回执对应各自操作的返回值。
    - 够数：第 256 个操作让它那一批作废，前 256 个全部是 `Batch`，之后的 44 个自成一批并提交，从而证明边界正好是 256。
    - 整批作废：没执行到的操作也得到 `Batch`，已经自己失败的保持 `Op`，写线程接着处理下一批。
    - 队列为空：`linger` 为 0 时立即提交；`linger` 生效时，等待期间到达的操作并入同一批；停止请求会结束等待。
  - [x] **UNIT-epoch-decision**：
    - 决策表：第一次启动、正常退出、异常退出 × 同一次开机、重启过，只有"异常退出 × 重启过"时加 1。
    - 从 meta 推导两个输入的各种情况。
    - 用例 ④：`kill -9` 后同一次开机不加 1，正常重启电脑后加 1，记下这个行为。
    - 真实的 `kern.bootsessionuuid`，以及 test-harness 下注入的 UUID 和格式错误的 UUID。
  - [x] **静止屏障**：写线程运行时 `running(WRITER) == 1`，`stop` 之后为 0。
  - [x] **pragma 读回**：
    - 打开连接时：`host.db` 和 `raw.db` 的六个 pragma 都读回核对；读连接核对 `wal`、5000，并确认写入被拒，报 ReadOnly。
    - 按批切换：用操作自己在批内读 pragma 核对。普通档和 Full 档同在一批时，两者都读到 FULL 加 fullfsync；下一批恢复 NORMAL；`checkpoint_fullfsync` 始终为 1。
  - [x] CI 摘要已随任务提交。
  - [x] 带编号的测试名以编号的 snake_case 开头。
- **其他检查：**
  - 四种特性组合下 clippy（`-D warnings`）和测试都通过；rustdoc（`-D warnings`）也通过。
  - 稳定性：store 测试连跑 50 次和 20 次，读池测试 30 次，"等待期间到达的操作并入同一批"的用例 100 次，全部通过。
- **变异检查**，都被测出来了：
  - 批上限差一；
  - 失败的操作不回滚 savepoint；
  - 持久性取 min 而不是 max；
  - 读池信号量多给一个许可；
  - 归还连接的代码被删掉。
  - 去掉 linger 那一项 100 次里测出 96 次，原因见遗留问题第 4 条。
- **规模**：新增 1,253 行（不含 Cargo.lock 和 CI 摘要），其中实现加文档约 690 行，测试约 560 行。超过 M 档的 900 行，在约 1,500 行的上限以内。

## 提交列表

```
ff3efcb docs(task): record the T024 pr CI summary
bd13113 build(host): give the lasting reason for rusqlite without default features
e377b90 feat(store): decide on data_epoch from the last exit and the boot session
6852eee feat(store): add the pool of four read-only connections
8d062e1 feat(store): add the writer thread with group commit and savepoints
c75c151 feat(store): open SQLite connections with the two durability levels
```

## 遗留问题 / 范围外发现

1. **合并冲突**：main 上已经合并了 T050。`crates/cupper-host/Cargo.toml`（ACP SDK、futures、tokio 特性、dev-dependencies）、`src/lib.rs`（`pub mod driver;`）、`Cargo.lock` 会冲突，两边都保留即可。我核对过，main 上 `cupper_sys::boot_session_uuid()` 的签名没有变。
2. **Zlib**：main 已经把 Zlib 加进 allowlist。T026 需要语句缓存（`prepare_cached`）时，给 rusqlite 打开 `cache` 特性即可，不需要再走许可证审批。
3. **交接事项（建议写入 handoff-notes）：**
   - **T025**：
     - 用 `open_writer(path, Database::Host)` 或 `Database::Raw` 打开连接，在这个连接上跑完迁移，再交给 `Writer::spawn`。迁移需要自己的事务，`VACUUM INTO` 也不能在事务里执行，所以不能作为写线程的操作。
     - `Readers::open` 放在写连接打开之后。
   - **T037**：
     - 启动第 5 步：`decide(LastExit::from_meta(..), Boot::since(meta.boot_session_uuid, current_boot_session(&payload)?))`，然后写 `last_start_at` 和 `boot.to_string()`。
     - `Writer::spawn` 之前先 `quiescence.register(store::WRITER)`；写线程配置用 `WriterConfig::for_startup(&payload, &durations)`。
     - 有序退出时，把 `last_clean_exit_at` 作为 `stop()` 之前的最后一个操作提交。
   - **T026、T032、T121**：
     - 回执在提交后才返回，可以直接作为"追加三步"的第二步。
     - 写 `pending_external` 的操作用 `Durability::Full`。
     - 操作不能自己结束事务；违反时整批作废。
   - **T027**：写线程目前只有 `host.db` 的连接。原始帧（`raw.db`，有界队列 4,096）需要在 `Batcher` 里加第二个连接和队列（按 5.2.8，每个数据库一个事务），或者另开线程；`Database::Raw` 的 pragma 已经备好。
   - **T047**：写线程现在只登记"运行中"。实现忙或空闲时，阻塞在 `inbox.recv()` 并且手里没有批次才算空闲。ManualClock 下 `linger` 为 0，不会有真实时间的等待。
   - **T069**：故障点 `store.before_commit` 和 `store.after_commit` 的自然位置，是 `Batcher::commit` 里 COMMIT 之前，以及 `Batcher::flush` 里提交之后、回复回执之前。
4. **"只等一次"没法确定性地测试。** 要区分"等待"和"不等"，必须让一条消息在写线程检查队列之后才到达，这只能靠时序。`collect` 层面的用例对正确的代码永远通过，对"去掉等待"的变异约 96% 能测出来。"只等一次而不是每次空了都等"这一点，靠代码审阅保证。
5. 事务被 SQLite 自己结束时，`Batch` 错误里带的是 "no such savepoint: store_op"，真正的原因在出错的那个操作自己的 `Op` 错误里。我认为可以接受，记在这里。
6. 本任务没有写 `notes.md`，思路和取舍都在本汇报里。
