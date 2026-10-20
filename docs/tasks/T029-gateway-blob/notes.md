# T029 工作笔记

- 执行者：子 agent（工程师；T077、T134、T136、T135 的执行者接手）

## 开工前核对现状

- **网关里的占位**：
  - `Routes::default()` 把 `blob/get`、`blob/put` 登记为 `Target::Blobs`；
  - `Target::serves()` 对它返回 false，所以现在回 `unsupported`；
  - `Dispatch::answer` 里是 `unreachable!`；
  - `conn.rs` 的 `on_frame` 在 `initialize` 之后丢掉非 JSON 帧；
  - `HOST_FEATURES` 只有 `stream.coalesce`、`history.page`。
- **`BlobStore`**：`put(data, media_type, expected)` 会核对单个大小上限（32 MiB）和哈希；`get(hash)` 读出整个 blob 并核对哈希。
- **CLI 的 `Client`**：不能发 `0x02` 帧，收到的 `0x02` 帧直接丢掉。在真实宿主上跑 IT-gw-blob 要给它补上收发分片，约 20 行。
- **范围外的发现**：生产的组装（`supervisor/mod.rs`）没有调用 `routes.streams(streams, readers)`，只有测试调用。也就是说，真实宿主对 `stream/subscribe`、`history/page` 回的是 `unsupported`。按守则不在本任务里改，写进汇报。

## 开工前的规模估计

| 模块 | 内容 | 草估 |
|---|---|---|
| 上传缓冲 | 每条连接 `Dispatch` 里的 `Uploads`：按哈希收集分片，偏移要接得上；单个不超过 32 MiB、合计不超过 64 MiB，超过就标记，不再增长；`write` scope 检查 | 60 |
| `blob/put`、`blob/get` | 取出缓冲，核对 `size`，经 `BlobStore::put` 核对哈希；按 `offset`、`length` 截取，经 `push_frame` 分片下发，再回结果 | 70 |
| 接线 | `Target::Blobs(BlobStore)` 和 `routes.blobs()`，去掉占位和 `serves()`；`on_frame`；`HOST_FEATURES`；组装处登记 | 30 |
| CLI 的分片收发 | `Client::send_blob_chunk`、`Incoming::Blob` | 20 |
| **Rust 小计** | | **180**，乘 1.1 约 200 |
| 测试：缓冲的单元测试 | 偏移、上限、标记 | 40 |
| 测试：IT-gw-blob | 真实宿主：往返、截取、`conflict`、`too_large`、断开后丢弃、`not_found` | 90 |
| **合计（不含 TS）** | | 约 **330** |
| host-client 的 `putBlob`、`getBlob`（可选） | 切片、sha-256、`blob/put`；`blob/get` 收集分片；用例 | 约 90 |

先做 Rust 部分，再按实际行数决定 TS 部分放不放得下（约 450 行的线）。
