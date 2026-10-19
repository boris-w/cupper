# T027 存储 IV：blob、原始帧与保留清理

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主存储）
- 分支：`feat/T027-store-blob-raw-retention`
- 依赖：T025（已合并）。T033（网关 II，关键路径）、T037 依赖它。

## 背景

存储层已经有：
- 写线程和读池（T024）；
- 表结构和迁移（T025）；
- 流、检查点、回合快照和历史（T026）。

本任务补上存储层剩下的三块：
- 按内容寻址的 blob 存储；
- 原始帧写入 `raw.db`；
- 各类保留清理（5.4.5）。

blob 在网关上的传输（`blob/get`、`blob/put` 的分片）归 T033。

## 范围

**做**（计划 T027 一行；设计 5.2.8、5.4.1、5.4.3、5.4.5）：
1. **`BlobStore`**：
   - 按内容寻址（5.4.4 的 `BlobHash`）；
   - 写入时先写临时文件，再改名；同一内容只存一份；
   - 读取时核对哈希；
   - 大小上限按 5.3.9；
   - 引用计数或可达性，以保留清理能安全删除为准，由你定，在 `notes.md` 里说明。
2. **原始帧写 `raw.db`**：
   - 线路分接（T050 的 `driver/tap`）输出的原始帧进入有界队列，容量 4,096；
   - 由写线程批量写入 `raw.db`：每个数据库一个事务（5.2.8）；
   - 队列满了就丢弃并计数，不能反压 ACP。
3. **保留清理**（5.4.5 的表）：
   - 各类数据的保留期；
   - 清理经 `Clock` 定时，时长接 `test_config.durations`（`durations.rs`）；
   - `backups/` 只保留最近 3 份（来自 T025 的交接）。
4. **`test/run_maintenance` 要用的入口**：给 T047 留一个"立即执行一轮清理"的函数。

**不做**：
- blob 的网关传输（T033）；
- 事件日志的裁剪（T026 已做）。

## 验收标准

- [ ] 新增 UNIT-blobstore：写入、去重、读取时核对哈希、哈希不符时报错、写到一半崩溃不留下可读的坏文件、大小上限。
- [ ] 新增 UNIT-raw-queue：有界队列满了就丢弃并计数，不阻塞调用方；批量写入 `raw.db`。
- [ ] 新增 UNIT-retention：按 5.4.5 逐类核对保留期；用 `ManualClock` 推进时间，不睡眠；`backups/` 只留最近 3 份。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T027-store-blob-raw-retention/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **写线程**（T024）：
  - 写线程目前只有 `host.db` 的连接。原始帧需要在 `Batcher` 里加第二个连接和队列（每个数据库一个事务），或者另开一个线程，由你定；
  - `Database::Raw` 的 pragma 已经备好。
- **备份**（T025）：
  - `backups/` 只保留最近 3 份，迁移成功后回收；
  - 备份文件名是 `host-v<版本>-<YYYYMMDDTHHMMSSZ>.db`，按名字排序就是按时间排序；
  - 忽略 `host.partial`。
- **原始帧的方向**（T050）：`Dir` 目前有 `send`、`recv`、`recv_nonjson`，`stderr` 和 `note` 由写入它们的任务补上。表的 `dir` 列已经按 5.4.2 允许五种取值。
- **惯例**：
  - 时间只经 `Clock`，定时只经 `Clock`；
  - SQLite 只在写线程和读池上运行；
  - 用到 `ManualClock` 的集成测试，声明 `required-features = ["test-harness"]`；
  - 只要改了协议类型，就跑 `pnpm gen` 并提交生成物（T015）。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 规模

计划估为 M（400–900 行）。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**预计会超出约 1,500 行时，必须先停下来汇报**。

## 参考资料

- 设计：
  - `02-host.md` 5.2.8（事件存储、写线程、原始帧）；
  - `04-data-model.md` 5.4.1、5.4.3、5.4.4（`BlobHash`）、5.4.5（保留与回收）；
  - `03-protocol-v1.md` 5.3.9（限额）。
- T024、T025、T026 的代码和汇报（`store/`）；T050 的 `driver/tap`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
