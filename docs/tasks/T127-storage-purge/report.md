# T127 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务与存储）
- 分支：feat/T127-storage-purge
- 完成日期：2026-11-09

## 做了什么

1. **协议**（只做了附加，schema-compat 0 breaking）：
   - `host/storage`：
     - 参数 `Empty`，返回 `{host_db, raw_db, blobs, snapshots, worktrees, partial, measured_at}`。
     - scope 为 read，幂等性为天然。
   - `host/purge_snapshots`：
     - 参数 `{command_id}`，返回 `{freed_bytes}`。
     - scope 为 privileged，属于命令，可能返回 `conflict/turn_running`。
   - `ConfirmAction::PurgeSnapshots`；catalog 的矩阵加两行；样例；重新生成了 TS 类型。
   - 宿主的策略矩阵：`host/purge_snapshots` 为 `CNNNNYN`，`host/storage` 为 `YYYYYYY`。
   - `policy/describe` 对清除返回 `purge_snapshots`，`details` 是 `{snapshot_bytes}`。
2. **每个仓库一道闸**（`GitTool` 里的 `Gates`，所有克隆共用一张表）：
   - `snapshot::open` 拿仓库的共享锁，放进 `Opened`，到 `close` 时放开。从 `add`、`write-tree` 到 `diff-tree`，一次快照自始至终都在同一把读锁里。
   - `diff::patch` 用的是缓存里的 `Place`，不经过 `open`，它也按仓库目录拿共享锁。
3. **`snapshot_missing`**：
   - `diff-tree` 失败后，用新加的快照档命令 `cat-file -e` 查两棵 tree。有一棵不在，就返回 `ShotError::Missing`，映射为 `unavailable/snapshot_missing`。
   - 正常路径不多跑命令。回合 diff、`file_diff` 和还原用的补丁都走这条路。
4. **`host/storage`**：
   - 在阻塞线程池里统计。截止时间是经 `Clock` 取的 `storage_walk`；超时就 `partial: true`，已经算到的照样返回。
   - 各项的算法：
     - `host_db`、`raw_db`：连同 `-wal`、`-shm` 一起算；
     - `snapshots`：git 工具的整个目录，包括对象库、私有 LFS 存储、临时 index 和残留；
     - `worktrees`：sessions 表里 `workspace_kind = 'worktree'` 的路径，只算 worktree 根目录下的。根目录先取真实路径再比较，因为 sessions 表里存的是真实路径。
5. **`host/purge_snapshots`**，步骤依次是：
   1. 按目录名顺序逐个拿各仓库的写锁；
   2. 拿齐之后再看有没有回合在运行或在准备，有就返回 `conflict/turn_running`，什么都不动；
   3. `mark_external`，写 `pending_external`；
   4. 把各仓库的 `objects` 和各工作区的 `ws/<wt>/lfs` 改名为 `*.purge-<pid>.<n>`，再新建空的 `objects`（0700）；
   5. 放开所有写锁，清空 `DiffCache`；
   6. 删掉改名后的目录，`freed_bytes` 是删掉的文件字节数之和；
   7. 完成命令。
6. **启动清理**：宿主启动时，在阻塞线程池里删掉残留的 `*.purge-*`。
7. **actor 的"有回合"标记提前写**：见下面专门一节。

## "有回合"标记那 3 行，以及为什么没有确定性用例

- **改动**：actor 在派出回合快照之前，就先把会话标成"有回合"。原来这个标记要到 `input()` 末尾才写。
- **窗口在哪里**：
  - 回合快照是在 `commit()` 末尾、写库完成之后才派出去的，原来的标记在 `input()` 末尾写。
  - 两者之间没有 await，除非同一批里还有后续输入，而后续输入只有 coalescer 的 `Input::Spilled`。
  - 所以窗口只在两种情况下打开：一是多线程运行时上，快照在别的线程里抢先做完；二是同一批里接着处理一个 spill，它的写库要等。
- **窗口打开时的后果**：清除可能看到"没有回合"，删掉刚拍的起始 tree。这个回合的 diff 会返回 `snapshot_missing`，不会出现错误的数据。
- **更正**：这个窗口比我开工前在 notes 第 4.3 节写的要窄。这 3 行能把它完全关上。
- **为什么没有确定性用例**：
  - 我试过让写库停住，但停住的是快照派出之前那次写库，用例只会超时，证明不了什么。
  - 要确定性地覆盖，得在"派出快照"和"写标记"之间加一个只给测试用的钩子。按 main 的意见，没有加。

## 私有 LFS 存储的处理，以及启动清理跳过本进程 pid 的目录

- **私有 LFS 存储一起清**：
  - 改名的是整个 `ws/<工作区>/lfs`，不只是 `lfs/objects`，这样 git-lfs 的 `tmp/` 也一起清掉。
  - 改名后不重建：git-lfs 用到时自己会建，和全新的 GIT_DIR 一样。
  - 大小算进 `snapshots` 和 `snapshot_bytes`；删除和启动清理都覆盖它。
- **清除之后的行为**：暂存或还原一个只在私有存储里有对象的 LFS 指针，会按 T237 返回 `lfs_object_missing`，提示用户 `git lfs fetch`。
- **启动清理跳过本进程 pid 的目录**：
  - 清除改名用的名字是 `*.purge-<pid>.<n>`。启动清理只删别的进程留下的，跳过名字里带本进程 pid 的目录。
  - 这样启动清理不会和这次运行刚开始的清除抢着删同一个目录；后者会因为文件不在而报错。
  - 遇到 pid 复用时，同 pid 的旧残留要等下一次启动再删，只多占些空间。

## 新增的时长 `storage_walk`

- `durations.rs` 新增 `STORAGE_WALK = "storage_walk"`，设计值 5 s。
- 它是两处统计大小的遍历时限：`host/storage`，以及清除确认框描述里的 `snapshot_bytes`。
- 截止时间经 `Clock` 取；到时停止遍历，返回已经算到的部分，`host/storage` 同时置 `partial: true`。
- 设计 5.2.16 的时长表里还没有这一项，请合并时补上。

## 设计偏差

1. **5.2.10 "清除快照对象"**：
   - 设计只写了 `<数据目录>/git/<repo_key>/objects/`。实际还清了各工作区的私有 LFS 存储：整个 `ws/<wt>/lfs` 改名后删除，不重建。
   - 启动清理跳过名字里带本进程 pid 的目录。
   - 以上是开工前 main 定的做法。
2. **5.2.16**：新增时长 `storage_walk` = 5 s。
3. **5.8.3**：两个方法的矩阵行已经写在代码里（`host/purge_snapshots` 为 `CNNNNYN`，`host/storage` 为 `YYYYYYY`），文档的表请对照补上。
4. **actor 的"有回合"标记提前到快照派出之前**：设计里没有写到这一层。
5. **没有 git 时**：
   - 清除返回 `unsupported/not_a_repo`（"no git to run"），`snapshots` 和 `snapshot_bytes` 都是 0。
   - 理由：快照在 git 工具的目录里，没有 git 时这一次运行也不会拍快照。
   - 前一次运行留下的快照对象，要等有了 git 才能清。

## 关键取舍

1. **互斥按 notes 第 2 节做**：每个仓库一把 `tokio::sync::RwLock`，快照共享，清除独占。
   - 只有清除会同时拿多把锁，并且按名字顺序拿，所以不会死锁。
   - 先拿锁再查回合，关掉了"查完回合到拿到锁之间开了一个回合"的窗口：拿锁之后才开始的快照要等读锁，最后写进新的空库。
2. **`DiffCache` 整个清空**：比按仓库挑简单，代价只是下一次 diff 要重算。
   - 持有旧 token 的调用返回 `stale_diff`。
   - 如果某个 diff 在清除之前取出、清空之后才写回缓存，它的补丁会因为 tree 不在而返回 `snapshot_missing`。用例覆盖了这种情况。
3. **快照目录从 git 工具取**：清除、`snapshots`、`snapshot_bytes` 用的都是同一个目录，与快照实际写入的位置一致。
4. **崩溃残留**：用"直接构造残留"的方式测，没有加故障点。

## 验证方式

- **本机**（单份跑）：
  - cupper-host 的 lib 测试 508 个通过；两条 IT 顺序连跑 3 次都通过。
  - 反向验证：把 `open` 里的共享锁换成一把无关的锁，快照档那条用例在"the purge waits"处失败。
  - clippy、fmt 干净；cupper-protocol 的测试通过。
  - schema-compat 0 breaking（12 项附加）；trace-ids 检查一致（452 个 ID）。
- **IT-snapshot-purge 的两条用例**：
  - `orchestrator::session::purge_tests::it_snapshot_purge_frees_the_snapshots_and_old_turns_go_missing`，会话夹具：
    - 回合运行时被拒（`conflict turn_running`）；window、cli 的 principal 被拒（`forbidden scope`）；
    - 清除前的大小：`host_db` 等于实际三个文件之和，`raw_db` 为 0，`blobs` 等于构造的 1000 字节，`partial` 为 false；
    - 清除：`freed_bytes` 大于 0，且正好等于 `snapshots` 前后之差；
    - describe 返回的 `action` 和 `snapshot_bytes`；
    - 旧回合的 diff 返回 `unavailable snapshot_missing`；旧 token 的还原返回 `conflict stale_diff`；
    - 新回合照常有 diff；
    - tree 被删、diff 还在缓存里时，还原返回 `unavailable snapshot_missing`；
    - `worktrees` 计入根目录下的 300 字节；
    - `ManualClock` 推进过截止时间后，`partial: true`。
  - `git::snapshot::tests::it_snapshot_purge_waits_for_snapshots_and_the_next_go_to_the_new_store`，快照档：
    - 清除等在途的快照：一个没关的 `Opened` 拿着读锁时，`hold` 停在 pending；
    - 清除拿着锁时开始的快照会等它，之后完整写进新的空库，下一张快照对它算出的 diff 正确（1 个文件，+1 −1）；
    - 私有 LFS 存储一起改名；
    - 启动清理删掉别的运行留下的对象库和 LFS 残留，本进程的保留。
- **验收标准逐条**：
  - [x] IT-snapshot-purge 和 `host/storage` 的用例通过；test-repeat 每份 10 次、共 4 份，40 次通过、0 次失败。
  - [x] 测试只用临时目录里的仓库和数据目录，只用假 agent。
  - [x] schema-compat 0 breaking；trace-ids 检查通过。
  - [x] PR 上 GitHub CI 的 8 个检查全部通过。
- **PR**：https://github.com/boris-w/cupper/pull/87
  - CI 运行：https://github.com/boris-w/cupper/actions/runs/42589560470 ，head 为 ea23e9e2。compat-framework、e2e、license、protocol、rust、testdata-scan、ts、ui 全部 pass。
  - test-repeat：https://github.com/boris-w/cupper/actions/runs/42589557370
    - 参数：`--features test-harness --lib`，filter `it_snapshot_purge`，每份 10 次、共 4 份，macos-15。
    - 日志末尾是 "40 passed, 0 failed"。脚本要求每次运行都匹配到 "test result: ok. N passed"（N ≥ 1），所以过滤词写错不可能蒙混过关。
    - Run 步骤从 09:04:43 到 09:04:58，约 15 s，与本机每次约 0.5 s、每份连跑 10 次相符。
- **规模**：`cargo xtask size` 为 +624 −13，在放宽后的硬停止线 640 以内。
  - 中途在 +436 时，预计要超过原来的硬停止线 550，停下来报告过；main 把硬停止线放到了 640。
  - 各模块的实际行数与估计见 notes 第 4.2、4.4 节。

## 提交列表

```
ea23e9e2 docs(T127): notes on the tests and the running mark
566d4fb4 test(host): IT-snapshot-purge
4b409456 fix(host): the purge and host/storage find the snapshots through the git tool
8a655f5f docs(T127): notes on the product code and the size ahead
e5102af4 feat(host): host/storage and host/purge_snapshots
2cae39f1 feat(git): snapshot_missing for a diff of trees that are gone
e898d996 feat(git): one gate per repository for snapshots and their purge
5d84ec0e feat(protocol): host/storage and host/purge_snapshots
81cc7b57 Merge main into feat/T127-storage-purge
1994731d docs(T127): where the work stands, and where it goes on
f837d7b5 docs(T127): the estimate, the purge's gate, and what main decided
```

## 遗留问题 / 范围外发现

1. **设计文档待补**：见"设计偏差"第 1 至 3 条（5.2.10、5.2.16、5.8.3）。
2. **清除不等的一处**：暂存或还原时，要从私有 LFS 存储往用户仓库复制对象（`lfs::supply`），这一步不拿闸。
   - 清除如果正好在复制时把存储改名，复制会因为找不到源文件而失败，返回 `git_error` 或 `lfs_object_missing`。
   - 不会写进不完整的对象：复制先写临时文件，核对 sha256 之后才改名到位。
   - 这一处没有加锁。
3. **界面提示**：清除之后，暂存或还原只在私有存储里有对象的 LFS 指针会返回 `lfs_object_missing`。界面要提示用户 `git lfs fetch`，属于 T147 或之后的界面任务。
4. worktree 保留在本机，等后续轮次。
