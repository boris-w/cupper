# T116 工作笔记

- 执行者：子 agent（Rust 工程师；T043、T055、T008 的执行者）

## 开工前读过的

- 任务书；设计 5.9.3、5.9.9，03 里 `diag/recording_export` 的一行（`{session_id}` → `{blob}`，`ui_local`，只在 test-harness、dev-auth 构建里有）。
- 现有代码：
  - `cupper-protocol::recording`（`Header`、`Frame`、`Recording::parse`、`segments`）；`methods::admin`（`DiagRecordingExportParams`、`DiagExportResult`）；
  - `driver/tap`（每个实例的 `seq` 计数在 `Tap` 里）、`store::raw_writer`（`Dir` 只有 send、recv、recv_nonjson；`raw.sql` 的 `dir` 列已经写明 stderr、note）；
  - `diag/stderr.rs`（`AdapterLog::read`）、`driver/runtime/launch.rs`（`start`）；
  - `supervisor/harness.rs`、`supervisor/mod.rs`；`gateway/dispatch/tests.rs` 的 UNIT-unsupported-set；
  - `store::blobs`（`put`）；`tools/sanitize/src/main.ts`（`sanitize --in --out --base`）。
- 发现：
  - `blob/get` 还没有执行者（`Until("T029")`），所以 IT 拿不到导出的内容，只能按 `blobs/sha256/<前两位>/<hash>` 直接读数据目录里的文件。
  - 假 agent 没有往 stderr 写一行的剧本命令，要加一个 `stderr <text>`。

## 做法（开工前）

- **一个实例一个编号器**：`Tap` 里给原始帧编号的部分拆成 `Recorder`（实例 ID、时钟、`RawSink`、`seq` 计数），由 `Tap`、stderr 读取和实例的启动、退出共用，这样四种帧在同一个实例里按同一个 `seq` 排序。
  - `instance_started` 在拉起之后、连接之前写，所以它的 `seq` 是 1；
  - `instance_exited` 在等到进程退出时写，带上退出状态。
- **stderr**：`AdapterLog::read` 每读到一行，除了写日志和尾部，还交给一个回调；回调把这行写成 `dir: stderr` 的原始帧（结构化脱敏，同其他帧）。
- **导出**：
  1. 从 `host.db` 取这个会话的 `agent_session_id`；
  2. 在 `raw.db` 里找带过这个会话的实例，按第一帧的时间排序，依次编为 0、1、2…；
  3. 每个实例的帧按 `seq` 排序，去掉点名了别的会话的帧；
  4. `t` 是相对这个实例第一帧的毫秒数，回拨时取不减；
  5. 头行的 `adapter_version`、`cli`、`cli_version` 取 `adapters` 表里第一个实例的那一行；`scenario` 填会话 ID，`usage` 填 `gold-b`、`replay`，由 T064 入库时按需改写；
  6. JSONL 放进 blob store，返回 `BlobRef`。
- **静止屏障**：原始帧不持有 `Hold`。IT 轮询 `raw.db`，等期望的 note 落盘之后再导出。

## 开工前的规模估计

**实现**

| # | 模块 | 草估 |
|---|---|---|
| 1 | `Recorder`（拆自 `Tap`）、`store::Dir` 加 stderr 和 note | 40 |
| 2 | `AdapterLog::read` 的逐行回调 | 5 |
| 3 | `launch::start`：编号器、两条 note、stderr 回调 | 20 |
| 4 | `diag/recording_export`：查询、换算、分段、头行、写 blob；端口与登记（test-harness、dev-auth） | 140 |
| 5 | 假 agent 的 `stderr <text>`，设计 09 加一行 | 8 |
| 6 | UNIT-unsupported-set 改为 `Builds`；`status.rs` 的过时注释 | 2 |
| | 实现小计 | **约 215**，×1.15 ≈ **250** |

**测试**

| 用例 | 草估 |
|---|---|
| 导出的换算（`t`、分段、过滤别的会话）：用造的帧 | 50 |
| `Recorder` 的 stderr 帧和 note 帧 | 20 |
| IT-recording-export：真实宿主加假 agent，两段（第一段带 stderr 和退出），格式库读回，node 跑 sanitize | 70 |
| 测试小计 | **约 140**，×1.1 ≈ **155** |

**合计约 405 行**，在约 700 的上限以内。
