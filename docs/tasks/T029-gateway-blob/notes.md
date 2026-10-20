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

## 实现中定下的几点

- **上传缓冲**（`gateway/blobs.rs` 的 `Uploads`，放在每条连接的 `Dispatch` 里）：
  - 按 32 字节的哈希收集分片，偏移必须接在已有字节之后；
  - 单个超过 32 MiB，或这条连接合计超过 64 MiB，这次上传就记为"太大"，已收的字节立即释放，之后的分片不再收；
  - 出现缺口（偏移接不上）记为"乱序"，同样释放、不再收；
  - `blob/put` 在读循环里同步取出这份上传，再异步存储，所以之后同一哈希的上传从头开始。
- **`blob/put` 的错误**：
  - 声明的 `size` 超过 32 MiB，或者上传被记为"太大"：`too_large`；
  - 乱序、收到的字节数与 `size` 不符、内容与哈希不符（`BlobStore` 的 `Mismatch`）：`conflict`；
  - 没有收到任何分片而 `size` 为 0：存一个空 blob。
- **分片的权限**：
  - 分片不经方法表，按 `blob/put` 的 `write` scope 自己检查，含提权授予的 scope；
  - 没有权限或头部解析不了的分片直接丢弃；之后的 `blob/put` 本来就会被策略拒绝。
- **`blob/get` 的截取**：
  - `offset`、`length` 截到 blob 的范围之内（超出不报错）；
  - 每个分片最多 `BlobChunkHeader::MAX_DATA` 字节，整帧不超过 64,000；
  - 范围为空时也发一个空的、标了最后一片的分片，结果里的 `chunks` 不为 0；
  - 分片先经 `push_frame` 进发送队列，结果在 future 完成后才写，所以分片一定在结果之前。
- **CLI 的 `Client`**：加了 `send_blob_chunk` 和 `Incoming::Blob`，IT 用它们在真实宿主上收发分片；`ClientError` 加了 `Frame` 一种。
- **占位的去除**：
  - `Target::serves()` 只为 blob 留着，一并删掉；
  - `Routes::default()` 不再登记 blob 方法，改由 `routes.blobs(store)` 登记，生产组装和分发测试都调用它；
  - UNIT-unsupported-set 的允许表删掉 blob 两行。

## 没有做的：host-client 的 `putBlob`、`getBlob`

Rust 部分和用例做完已到约 458 行，放不下 TS 那一组（约 90 行），留给 T086、T145。需要的形状：
- **`putBlob(bytes, mediaType)`**：
  - 用 `crypto.subtle.digest` 算 sha-256；
  - 按 `MAX_BLOB_CHUNK_BYTES` 切片，经 `HostPort.sendBlob` 发出，`HostPort` 已有这个方法；
  - 再 `request("blob/put", {hash, size, media_type})`。
- **`getBlob(hash, range?)`**：经已有的 `onBlobChunk` 收集分片，按 `offset` 拼起来，等 `blob/get` 的结果给出 `chunks` 数后收齐。

## 发现的范围外问题

- **生产组装里没有登记订阅和历史**：`supervisor/mod.rs` 没有调用 `routes.streams(streams, readers)`，只有测试调用。所以真实宿主对 `stream/subscribe`、`history/page` 回 `unsupported`；桌面的索引订阅、T134 `confirm_config` 的会话流快照，在真实宿主上都会失败。按守则不在本任务里改。
- **上传缓冲的条目数没有上限**：每个只发了一片就超限或乱序的哈希会留下一个空条目，直到 `blob/put` 或断开。条目不占数据字节，但数量没有上限。需要的话加一个"每连接未决上传数"的上限。

## 实际规模

| 部分 | 新增 / 删除 |
|---|---|
| `gateway/blobs.rs`（实现约 175，单元测试约 68） | 243 / 0 |
| 接线（`dispatch.rs`、`routes.rs`、`conn.rs`、`handshake.rs`、`mod.rs`、组装处） | 约 57 / 20 |
| CLI 的分片收发 | 31 / 2 |
| IT-gw-blob（`tests/lifecycle.rs`） | 125 / 0 |
| 分发测试的调整 | 约 4 / 2 |
| **合计** | **约 458 / 24**，超过约 450 行的线，低于 520 的硬停止线 |

实现部分只有 Rust，按估计算是 180（乘 1.1 约 200），实际约 263。多在 `blobs.rs` 的错误分支和格式化展开上。测试估计 130，实际约 197。
