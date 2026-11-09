# T127 存储用量与清除快照对象

- 派发日期：2026-11-04
- 执行者：子 agent（Rust 工程师，宿主 Git 服务与存储）
- 分支：`feat/T127-storage-purge`
- 依赖：T099（已合并）。T147 依赖本任务

## 背景

快照档会把没被 gitignore 的未跟踪文件也复制进 cupper 私有的对象库，其中可能有密钥（安全建议 #15）。M1 不按规则回收快照对象（那是 M2 的事），所以设置页"存储"要显示各类数据的大小，并提供"清除快照对象"（5.4.5）。

接口名在派发前已按 P-6 补进设计（计划 v1.3.88）：

- 5.3.4：`host/storage`、`host/purge_snapshots`；
- 5.3：`policy/describe` 的 `purge_snapshots`；
- 5.2.10："清除快照对象"一段；
- 5.7.3：`PrivilegedAction` 的 `purge_snapshots`；
- 5.8.2：scope 表。

## 范围

**做**（计划 T127 一行；设计 5.2.10"清除快照对象"、5.3.4、5.4.5；P-6）：

1. **`host/storage`**（scope read，附加）：
   - 返回 `{host_db, raw_db, blobs, snapshots, worktrees, partial, measured_at}`，都是字节数。
   - `worktrees` 只算 cupper 自己建的 worktree（worktree 根目录下）。遍历要有时限，超时时 `partial: true`，已算到的照样返回。
   - 计算放在阻塞线程池里，不在 actor 里做；时间经 `Clock`。
2. **`host/purge_snapshots`**（scope privileged，只能经原生确认，附加）：
   - 有回合在运行或在准备时，回 `conflict/turn_running`，什么都不清。
   - 清除所有仓库的私有对象库：先把对象库改名到同目录下的临时名、建一个空库，再删旧库，返回 `{freed_bytes}`。中途崩溃时，启动清理会删掉残留的临时库。
   - **与快照的互斥**：快照档的调用不拿 `RepoLock`，所以清除与快照之间要另有互斥，不能出现"一棵 tree 指向已被删掉的 blob"。设计写的方向是每个仓库一道闸，快照共享、清除独占。**具体做法开工前写进 `notes.md`，告诉 `main` 之后再动手。**
   - 清除后清空这些仓库的 `DiffCache`。`turns` 表里的 tree id 保留，用到它们的 `git/diff`、`git/file_diff`、回合范围的 `git/revert` 回 `unavailable/snapshot_missing`。现有代码里这条路径要核对，没有的话补上。
   - 先写 `pending_external` 再动手（照 T121 的写操作路径），`policy/describe` 回 `purge_snapshots`，`details` 是 `{snapshot_bytes}`。
3. **用例**：
   - IT-snapshot-purge（已登记）：
     - 清除前后的大小；
     - 清除后旧回合的 diff 和还原回 `snapshot_missing`；
     - 清除后新回合照常拍快照、照常有 diff；
     - 有回合在运行时被拒；
     - 只能经原生确认：window、cli 的 principal 被拒；
     - 中途崩溃后的残留被清理，可以用故障点或直接构造残留。
   - `host/storage` 的单元或集成用例：
     - 各项大小与构造的数据相符；
     - 遍历超时时 `partial: true`，时间经 `Clock` 推进。
4. 协议照 ADR-04，契约已冻结，只能附加，`schema-compat` 0 breaking；两个方法都要声明 `SCOPE` 和 `IDEMPOTENCY`。

**不做**：
- 设置页"存储"的界面，以及桌面端 `PrivilegedAction` 的实现（T147）；
- 按规则回收快照对象（M2，NFR-11）。

## 验收标准

- [ ] IT-snapshot-purge 和 `host/storage` 的用例都通过，IT 在 runner 上连跑 10 次（由 `main` 触发 test-repeat）。
- [ ] 测试只用临时目录里的仓库和数据目录。
- [ ] `schema-compat` 0 breaking；trace-ids 检查通过。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：470；硬停止线：640（2026-11-04 开工前估约 455，不拆；原为 400、480。2026-11-09 产品代码写完 +436，加用例估约 615，硬停止线由 550 放到 640）。
- 开工前定下（执行者在 `notes.md` 里提出，`main` 同意）：每个仓库一把 RwLock，快照共享、清除独占（按目录名排序逐个拿写锁，拿齐后再检查回合）；私有 LFS 存储一起清，大小算进 `snapshots`。

## 参考资料

- 设计：
  - 5.2.10（快照档、`DiffCache`、并发、"清除快照对象"）；
  - 5.2.8（`pending_external`）；
  - 5.3.4、5.3 的 `policy/describe`；
  - 5.4.5（保留与回收）；
  - 5.8.2（scope）。
- 卷宗：T061（快照档）、T120、T221（diff 与 `DiffCache`）、T121（写操作路径）、T125（worktree 根目录）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 子进程一律经 `cupper-sys::spawn`；时间经 `Clock`；每条事件流只有一个写者。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
