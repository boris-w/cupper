# T164 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增 164 行、删除 12 行，在约 300 的线内。
- **CI**：执行者在干净的 worktree 里跑 `cargo xtask ci pr --record` 通过，摘要 `pr-08c520f4d235.json`（rust 1851、protocol 185、ts 436、license、testdata-scan、ui 16，UI-csp-zero 通过）。分支基于 `e4c3eb4`，此后 main 上只有计划和任务书的改动，没有代码，所以没有重跑。

## 逐项意见

1. **`putBlob`**：
   - 用 Web Crypto 算哈希，不加依赖；
   - 每片不超过 `MAX_BLOB_CHUNK_BYTES`、只有最后一片标 `last`，空内容也发一个空的最后一片，与 T029 宿主端的约定一致；
   - 返回值直接就是 `text_blob` 要的 `BlobRef`；
   - `too_large`、`conflict` 带语义码，断线以 `ConnectionLost` 结束。
2. **输入框**：
   - 预算按整条请求参数的 UTF-8 字节数，取握手的 `max_frame_bytes` 再留 1 KiB 给外壳；
   - 只换文本块，附件原样保留；
   - 失败走 T086 已有的路径（草稿放回、toast）。
   - 接入点完全照 T086 的交接。
3. **断线不自动重试**：分片发出后连接断了，重连时宿主那边已经没有这批分片，交给用户再发一次。简单、可以接受。
4. **用例**：`putBlob` 的切片、哈希、错误码、空内容；输入框的长文本、短文本、`too_large` 的失败路径。

## 遗留

- `getBlob` 仍归第一个在界面里读 blob 的任务（T029 的交接，已在 `handoff-notes.md`）。
