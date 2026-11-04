# T127 notes

## 1. 按模块估行数

| 模块 | 做什么 | 估计 |
|---|---|---|
| 协议 | `host/storage`、`host/purge_snapshots` 的参数和结果（`SCOPE`、`IDEMPOTENCY`）、`ConfirmAction::PurgeSnapshots`、catalog 两行、样例 | 60 |
| 闸（`git/` 里） | 每个仓库一道读写闸，`GitTool` 共享；`snapshot::open` 拿共享闸，放在 `Opened` 里随 `close` 放开；`diff::patch` 这类沿用缓存 `Place` 的调用也拿共享闸 | 35 |
| `host/storage` | 各项大小：`host.db`、`raw.db` 连同 `-wal`/`-shm`，`blobs/`，各仓库的 `objects/`（加私有 LFS 存储，见 2.4），worktree 根目录下 cupper 建的 worktree。在阻塞线程池里遍历，截止时间经 `Clock` 取，到时 `partial: true` | 70 |
| `host/purge_snapshots` | 拿闸、检查回合、`pending_external`、改名加建空库、删旧库、算 `freed_bytes`、清 `DiffCache` | 70 |
| 启动清理 | 删掉残留的 `objects.purge-*`（和 LFS 的同类目录） | 15 |
| `snapshot_missing` | 回合范围的 diff、file_diff、revert 用到的 tree 已经不在时，回 `unavailable/snapshot_missing`（见 2.5） | 20 |
| 路由与策略 | 两个方法的端口；`policy/describe` 的 `purge_snapshots`（`details: {snapshot_bytes}`）；矩阵一行 | 25 |
| **产品小计** | | **约 295** |
| 测试：IT-snapshot-purge | 会话夹具：清除前后的大小；旧回合的 diff 和还原回 `snapshot_missing`；新回合照常；回合运行时被拒；window、cli 的 principal 被拒；构造残留后启动清理 | 110 |
| 测试：`host/storage` | 各项大小与构造的数据相符；截止时间设为 0 时 `partial: true` | 40 |
| 测试：策略矩阵、describe | 一行，加一条断言 | 10 |
| **测试小计** | | **约 160** |
| **合计** | | **约 455** |

超过停止线 400，在硬停止线 480 以内。可以拆的地方只有 `host/storage`（约 110，含用例）；它和清除是设置页"存储"的同一块，我倾向不拆，停止线放到约 470。

## 2. 清除与快照之间的互斥

### 2.1 闸

- **一个仓库一道闸**，键是私有目录名（`<数据目录>/git/<repo_key>/` 的 `repo_key`，即 `key(common_dir)`）。用 `tokio::sync::RwLock<()>`，`Arc` 存在 `GitTool` 里，所有克隆共享同一张表（`Mutex<HashMap<String, Arc<RwLock<()>>>>`）。
- **快照共享**：`snapshot::open` 拿到 `Place` 后取这个仓库闸的读锁（owned guard），放进 `Opened`，`close` 或 drop 时放开。这样从 `add -A`、`write-tree` 到 `diff-tree`，一次快照从头到尾都在同一个读锁里，不会出现"写了 blob，还没 `write-tree` 就被清掉"。所有经 `open` 的路径都被覆盖：回合快照（`take`）、`git/diff`、`git/status`、`files/search`、`worktree/check`。
- **沿用缓存的调用**：`diff::patch`（暂存、取消暂存、还原、`file_diff` 用的补丁）直接用缓存里的 `Place` 跑快照档，不经 `open`。它也要按 `Place` 的仓库键取读锁。
- **清除独占**：按目录名排序后逐个取写锁。只有清除会同时拿多道闸，快照每次只拿一道，所以不会死锁。tokio 的 `RwLock` 按 FIFO 排队，清除要等在途的快照做完；清除排上队之后，新来的快照排在它后面。

### 2.2 清除的步骤

1. 列出 `<数据目录>/git/*/objects`，按目录名排序，取全部写锁。
2. 拿到锁之后再检查有没有回合在运行或在准备（`Activity.running_turns`：会话只要有回合就算，`Preparing` 也算）。有的话立刻放锁，回 `conflict/turn_running`，什么都不动。

   先拿锁再检查，是为了关掉"检查完到拿到锁之间开了一个回合"这个窗口：拿锁之后开的回合，它的起始快照要等读锁，清除完才会用新的空库拍，tree 是完整的。拿锁之前已经开了的回合，检查时就会被看到。
3. `ticket.mark_external()`（`pending_external`，与 T121 的写操作同法）。
4. 逐个仓库：`objects` 改名为同目录下的 `objects.purge-<unique>`，再新建空的 `objects`（0700）。
5. 放开全部写锁，在阻塞线程池里统计改名后目录的大小，然后删掉它们。`freed_bytes` 是删之前统计出的总和。
6. 清空 `DiffCache`：全部清空，比按仓库挑简单，代价只是下一次 diff 重算。
7. `ticket.complete`。

中途崩溃：已经改名的目录名以 `objects.purge-` 开头，宿主启动时（放在现有的启动恢复里）删掉。日志里的 `pending_external` 让重试拿到 `outcome_unknown`。

### 2.3 为什么不会出现"tree 指向已删的 blob"

- 一棵新 tree 的 blob 和它本身，都在同一个读锁里写进同一个对象库。
- 清除只在没有任何读锁时改名，并且改名之后，所有新来的读锁看到的都是新的空库。
- 旧库里的 tree 只能被 `turns` 表里的 tree id 和 `DiffCache` 引用：缓存清空；表里的 id 走 2.5。

### 2.4 要你定的一处：私有 LFS 存储一起清吗

T235 之后，快照档的 git-lfs 把它 clean 过的文件内容放在 `ws/<工作区>/lfs/objects/` 里。这和对象库一样，可能有密钥（安全建议 #15 的理由同样成立）。

- **建议一起清**，同样走"改名加删除"，大小也算进 `snapshots`。代价是：清除之后，暂存或还原一个只在私有存储里有对象的 LFS 指针，会按 T237 回 `lfs_object_missing`。行为是对的，提示用户 `git lfs fetch`。
- 设计 5.2.10 只写了 `objects/`。要不要把 LFS 存储加进去，你定。

### 2.5 `snapshot_missing`

现状：回合范围的 diff 在 `diff-tree` 里引用不存在的 tree 时，git 失败，按 `git_error` 返回。改成：`diff-tree` 失败后，用 `cat-file -e <tree>`（快照档）查这两棵 tree 在不在，不在就回 `ShotError::Missing`，RPC 映射为 `unavailable/snapshot_missing`。正常路径不多跑命令。

`git/file_diff` 和回合范围的 `git/revert` 都先要拿到 diff token，清除之后得重新 `git/diff`，于是走到同一条路径。持有旧 token 的会因为缓存清空回 `stale_diff`。

## 3. 定下的做法（2026-11-04，main）

- 不拆：`host/storage` 和清除同属设置页"存储"。停止线 470，硬停止线 550。
- 互斥按第 2 节做；两点要有用例：拿锁之后才开的回合，起始快照进新的空库，回合的 diff 正常；清除等在途的快照做完才动手（故障点或可控的延迟让快照停在锁里）。
- 私有 LFS 存储一起清：改名加删除，大小算进 `snapshots`，启动清理也覆盖；清除之后 `lfs_object_missing` 的行为是对的。设计 5.2.10 由 main 在合并时补。

## 4. 进度（2026-11-04 收工）

- 已提交：本 notes（估算、互斥方案、main 的决定）。产品代码还没开始写，工作区干净。
- 下次从协议开始：`handshake.rs` 加 `host/storage`（`Empty` → `HostStorageResult`，`read`、天然）和 `host/purge_snapshots`（`{command_id}` → `{freed_bytes}`，`privileged`、命令、`conflict/turn_running`），`admin.rs` 的 `ConfirmAction::PurgeSnapshots`，`catalog.rs` 的分组与矩阵两行，样例，`pnpm gen`。
- 之后按第 1 节的模块顺序：闸（`GitTool` 里的仓库读写锁，`snapshot::open` 与 `diff::patch` 取读锁）→ `snapshot_missing` → `host/storage` → `host/purge_snapshots`（含私有 LFS 存储）→ 启动清理 → 路由与 `policy/describe` → 用例（含"拿锁之后开的回合"和"清除等在途快照"两条）。
