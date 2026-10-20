# T116 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增 514 行（不含 lock 和文档），在约 700 的上限之内；估计 405，多出的是两处用例和 IT-lifecycle 抽出的共用函数。
- **CI**：执行者的摘要 `pr-2e173bbbcda5.json` 通过。合并 main 之后在本 worktree 重跑 `cargo xtask ci pr --record`，`pr-aa07657fe33c.json` 通过（rust 1754、protocol 185、ts 353、license、testdata-scan）。
- **合并冲突**（编排者解决）：只有 `launch.rs` 的 import。保留本分支要用的 `ExitStatusExt` 和 `json`；main 上 T115 删掉了这里的哈希核对，`fs` 和 `sha2` 不再需要。

## 逐项意见

1. **一个实例一个 `seq`**：wire 帧、stderr 帧和 note 共用 `Recorder` 的计数，`instance_started` 的 `seq` 是 1。stderr 帧和其他帧走同一层结构化脱敏。好。
2. **导出方法的边界**：
   - 只在 test-harness、dev-auth 构建里编译和登记；
   - scope 是 `UiLocal`，权限矩阵已有对应的一行；
   - 导出的是未脱敏的录制，入库前由调用方跑 `tools/sanitize`，与设计 5.9.3 一致；
   - UNIT-unsupported-set 去掉了 `Until("T116")`，四种特性组合都覆盖。
3. **分段与时间**：按实例分段；`t` 相对段内第一帧，系统时间回拨时取不减；retention 删掉开头时补 `seq` 0 的 `instance_started`。单测都有。
4. **按会话导出的近似**：没有 `sessionId` 的帧都保留。兼容套件每个宿主只跑一个会话，不受影响。记进交接，T064 照此使用。
5. **IT 的等待方式**：原始帧不持有 `Hold`，IT 轮询 `raw.db` 等 note 齐了再导出。做法已写进 `notes.md`，可以接受。

## 范围外发现的归属

- **`AdapterLog` 打开失败时 stderr 管道被关掉**（T055 起就有）：适配器再写 stderr 会收到 SIGPIPE，stderr 帧也没有了。这是缺陷，交给 T063（驱动与回合的进程级用例，缺陷按 1.2 的上限就地修），见 `handoff-notes.md`。
- **给 T064 的四条**：
  - `blob/get` 由 T029 实现，在那之前直接读数据目录里的 blob 文件；
  - 头行的 `scenario`、`usage` 要改写；
  - sanitize 的 `--base` 要同时传链接路径和真实路径；
  - 导出受 `max_blob_bytes` 限制，导出的 blob 过了宽限期会被当作孤儿清掉，要及时取走。
  - 都已记进交接。
