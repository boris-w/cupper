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

## 完成之后

### 实际规模

按 `git diff --numstat`（相对估计提交 4be15c1，不含 `Cargo.lock` 和文档）：新增 514 行，删除 57 行。估计 405，主要多在两处用例：
- `recording_export.rs` 271 行，其中单测约 75 行；
- IT 所在的 `tests/lifecycle.rs` +135 −19，其中把 IT-lifecycle 里拉起假 agent 宿主的步骤抽成了共用函数。

| 模块 | 新增 |
|---|---|
| `Recorder`、`store::Dir` 的 stderr 和 note、`AdapterLog::read` 的回调、`launch::start` 的两条 note | 75 |
| `diag/recording_export`（含单测）、登记、`diag/mod.rs` | 281 |
| UNIT-unsupported-set：`Builds` 带上本构建有没有这个方法 | 19 |
| 假 agent `stderr`、`status.rs` 注释 | 4 |
| IT-recording-export 与共用的假 agent 宿主 | 135 |

### 做法与取舍

- **`seq`**：同一实例的 wire 帧、stderr 帧和 note 共用 `Recorder` 的计数，`instance_started` 在连接之前写，所以 `seq` 是 1。`instance_exited` 在等到进程退出时写，带 `code` 和 `signal`（形状同 SP-1 转换后的录制）。如果还有 stdout 没读完，它的 `seq` 可能排在最后几行之前，格式允许。
- **导出范围**：带过这个会话（`agent_session_id`）的实例都算，每个实例里去掉点名了别的会话的帧。没有 `sessionId` 的帧都保留，包括 `initialize`、通知和回复，所以同一个实例里别的会话的回复也会混进来；兼容套件每个宿主只跑一个会话，不受影响。
- **分段**：实例按第一帧的时间排序。段里的第一帧不是 `instance_started` 时（retention 删掉了它，或者是 T116 之前记的实例），补一条 `seq` 0、`t` 0 的 note。
- **头行**：`source: host`，`adapter` 按 `agent` 列定（`claude-agent-acp`、`codex-acp`），`adapter_version`、`cli_version` 取第一个实例的 `adapters` 行，`cli` 用 agent 名；`scenario` 填会话 ID，`usage` 填 `gold-b`、`replay`，由 T064 入库时改写。参数仍是 `{session_id}`，没有改协议类型。
- **blob**：JSONL 经 `BlobStore::put` 存入，媒体类型 `application/jsonl`，受 `max_blob_bytes` 限制。没有引用，所以过了孤儿 blob 的宽限期会被清掉。`blob/get` 还没有执行者（T029），IT 按 `blobs/sha256/<前两位>/<hash>` 直接读文件。
- **构建**：模块和登记都在 `cfg(any(test-harness, dev-auth))` 下。UNIT-unsupported-set 的 `Why::Builds` 带上"本构建有没有这个方法"，`test/*` 只看 test-harness，`diag/recording_export` 看 test-harness 或 dev-auth；夹具像宿主一样登记它。
- **静止屏障**：原始帧不持有 `Hold`。IT 轮询 `raw.db`，按 `ts, seq` 排序，等到 note 依次是 started、exited、started 之后才导出。之后需要等原始帧落盘的用例照此做：轮询 `raw.db`，等期望的帧或 note 出现。
- **IT 里的 sanitize**：
  - 运行 `node tools/sanitize/src/main.ts sanitize --in … --out … --base <临时目录> --base <它的真实路径>`，退出码 0 表示脱敏后扫描无发现；
  - 帧里的 cwd 是真实路径（`/private/var/…`），所以两种写法都作为 `--base` 传入；
  - 导出的文件和输出都在用例的临时目录里，随目录删除，不入库。

### 交接

- **T064**：
  - 导出结果在 blob 里。`blob/get` 有执行者之前（T029），只能读数据目录里的文件；
  - 头行的 `scenario`、`usage` 要按剧本改写；
  - 入库前用 `tools/sanitize sanitize --base` 传入运行时用到的临时根目录（真实路径和链接路径都传）。
- **范围外**：`AdapterLog` 打开失败时，launch 不读适配器的 stderr，管道直接关掉，这是 T055 起的原有行为。这时 stderr 帧也就没有了，适配器再写 stderr 会收到 SIGPIPE。要改的话，可以在没有日志时也起一个只写原始帧的读取任务。
