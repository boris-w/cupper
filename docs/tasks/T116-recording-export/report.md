# T116 汇报

- 执行者：子 agent（Rust 工程师；T043、T055、T008 的执行者）
- 分支：`feat/T116-recording-export`
- 完成日期：2026-10-20

T116 已完成：`diag/recording_export` 可以把一个会话的原始帧导出成录制格式 v1，stderr 和实例的启动、退出也写进了原始帧。IT-recording-export 用真实宿主加假 agent 跑通，读回、分段、node 跑 `tools/sanitize` 都通过。PR 套件在干净的 worktree 里通过，摘要已提交。规模新增 514 行，在约 700 的上限以内。

## 做了什么

1. **stderr 和 note 写进原始帧**：
   - `Tap` 里给帧编号的部分拆成 `Recorder`，同一个实例的 wire 帧、stderr 帧和 note 共用一个 `seq` 计数；
   - `store::Dir` 加了 `Stderr`、`Note`（`raw.sql` 的 `dir` 列本来就写着这两种）；
   - `AdapterLog::read` 加了逐行回调：每行 stderr 仍写进适配器日志，同时写成一条 `dir: stderr` 的原始帧，脱敏方式和其他帧一样；
   - `launch::start` 在连接之前写 `instance_started`，所以它的 `seq` 是 1；进程退出时写 `instance_exited`，带 `code` 和 `signal`，形状和 SP-1 转换后的录制一致。
2. **`diag/recording_export`**（`diag/recording_export.rs`，只在 test-harness、dev-auth 构建里编译和登记，在 `Host::start` 里紧挨着 harness）：
   - 范围：带过这个会话的每个适配器实例是一段，按实例开始的先后排；点名了别的会话的帧去掉。
   - 时间：`raw.db` 的 `ts` 换算成相对本段第一帧的 `t`，系统时间回拨时取不减。
   - 缺失的开头：retention 删掉了开头的 `instance_started` 时，补一条 `seq` 0 的。
   - 头行：取自第一个实例的 `adapters` 行。
   - 输出：用 `Header::to_line` 和 `Frame::to_line` 写成 JSONL，经 `BlobStore::put` 存进 blob，返回 `{blob}`。
   - 不脱敏：宿主只做了原始帧那一层结构化脱敏，入库前由调用方跑 `tools/sanitize`。
3. **UNIT-unsupported-set**：删掉了 `Until("T116")`；`Why::Builds` 改成带一个"本构建有没有这个方法"的布尔值，`diag/recording_export` 看 test-harness 或 dev-auth，`test/*` 只看 test-harness；测试夹具像宿主一样登记这个方法。
4. **假 agent**：加了 `stderr <text>` 剧本命令，设计 09 的命令表补了一行。
5. **顺手**：`status.rs` 里过时的"Sampling is T008's"注释已改掉。

## 关键取舍

- **参数没变**：仍是 `{session_id}`，没有改协议类型。头行的 `scenario` 先填会话 ID，`usage` 先填 `gold-b`、`replay`，由 T064 入库时按剧本改写。
- **导出范围的近似**：按会话导出时，没有 `sessionId` 的帧都保留，包括 `initialize`、通知和回复。所以同一个实例里如果还有别的会话，它的回复也会混进来。兼容套件每个宿主只跑一个会话，不受影响。
- **静止屏障**：原始帧不持有 `Hold`。IT 轮询 `raw.db`，等 note 依次是 started、exited、started 之后才导出。这个做法已写进 `notes.md`，后面需要等原始帧落盘的用例照此做。
- **读导出结果**：`blob/get` 还没有执行者（T029），IT 只能按 `blobs/sha256/<前两位>/<hash>` 直接读数据目录里的文件。

## 验证方式

- `cargo xtask ci pr --record docs/tasks/T116-recording-export/ci/` 在干净的 worktree 里通过，摘要是 `pr-2e173bbbcda5.json`：

  | 套件 | 通过 |
  |---|---|
  | rust | 1702 |
  | protocol | 185 |
  | ts | 353 |
  | license | 1 |
  | testdata-scan | 3 |

  前两次运行时 license 作业都因为拉不到 GitHub 上的 advisory 数据库而失败（`SSL_ERROR_SYSCALL`，属于网络问题），第三次通过。
- **IT-recording-export**（`tests/lifecycle.rs`，`it_recording_export_two_instances_read_back_and_sanitize`）：
  - 用例过程：真实宿主加假 agent。第一回合输出文本并写一行 stderr，第二回合让适配器退出，第三回合让它按退避重启（`adapter_backoff_ms` 设为 1）。然后经网关导出。
  - 读回：`Recording::parse` 能读回，共两段，每段都以 `instance_started` 开头，第一段里有 stderr 帧 `oops` 和 `instance_exited`。
  - 脱敏：用 node 跑 `tools/sanitize ... sanitize --in --out --base <临时目录及其真实路径>`，退出码 0。临时文件随用例目录删除，不入库。
  - 连跑 3 次都通过，跑完没有残留进程。
- **单测** `it_recording_export_segments_instances_and_times_frames`：用造的帧验证分段、`t` 的换算（含时间回拨）、过滤别的会话、补开头的 note、头行，以及不存在的会话返回 `None`。
- **发布构建**：不登记这个方法。四种特性组合（无特性、dev-auth、test-harness、全部）的 UNIT-unsupported-set 和 `clippy -D warnings` 都通过；UNIT-release-features 在 CI 的无特性运行里照常通过。

## 提交列表

```
5e1f76d docs(task): T116 CI summary
2e173bb docs(task): T116 notes: sizes, choices, handoffs
5d3615d test(host): IT-recording-export through a real host
f5bccc2 feat(host): `diag/recording_export` in test and development builds
d4f0621 feat(host): an adapter's stderr and its notes as raw frames
18c760b feat(fake-agent): `stderr <text>` writes a line to stderr
4be15c1 docs(task): T116 size estimate and approach
```

## 遗留问题 / 范围外发现

- **规模**：新增 514 行（不含 lock 和文档），估计是 405。多出来的主要是两处用例：
  - 单测约 75 行；
  - IT 所在的 `tests/lifecycle.rs` +135 −19，其中把 IT-lifecycle 里拉起假 agent 宿主的步骤抽成了共用函数。
- **给 T064 的交接**：
  - `blob/get` 有执行者之前，只能读数据目录里的文件；
  - 头行的 `scenario`、`usage` 要改写；
  - 跑 sanitize 时，用例的临时根目录要把链接路径和真实路径都作为 `--base` 传入；
  - 导出受 `max_blob_bytes` 限制；导出的 blob 没有被引用，过了宽限期会被当作孤儿清掉。
- **原有问题（T055 起）**：`AdapterLog` 打开失败时，launch 不读适配器的 stderr，管道直接关掉。这时 stderr 帧也就没有了，适配器再写 stderr 会收到 SIGPIPE。可以在没有日志时也起一个只写原始帧的读取任务，已写进 `notes.md`，这次没改。
- **帧的顺序**：`instance_exited` 在等到进程退出时写。如果还有 stdout 没读完，它的 `seq` 可能排在最后几行之前，录制格式允许这样。