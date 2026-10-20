# T029 汇报

- 执行者：子 agent（工程师；T077、T134、T136、T135 的执行者接手）
- 分支：`feat/T029-gateway-blob`
- 完成日期：2026-10-20

## 先说两件要您处理的事

1. **真实宿主不提供订阅和历史（范围外，没有改）**：生产组装（`supervisor/mod.rs`）里没有调用 `routes.streams(streams, readers)`，只有测试调用。所以真实宿主对 `stream/subscribe`、`history/page` 回 `unsupported`。受影响的是：
   - 桌面订阅索引流；
   - T134 `confirm_config` 取会话流快照；
   - T090 的 e2e。

   修起来很小：在组装处加一行，把 `streams`、`readers` 登记进去。但这两个值在 `test-harness` 构建里已经被种子模块拿走（move），要调整一下顺序。按守则没有顺手改。建议排在 T090 之前。
2. **规模**：新增 460 行、删除 24 行，超过约 450 行的线，低于 520 的硬停止线。超出是写测试时发生的：实现写完时统计约 263 行，按测试估计推算约 390 行，没有贴线；测试实际约 197 行，比估计的 130 行多。host-client 的 TS 辅助因此放不下，留给 T086、T145，见遗留问题第 1 条。

## 做了什么

**1. `0x02` 分片的接收与缓冲**（`gateway/blobs.rs` 的 `Uploads`，放在每条连接的 `Dispatch` 里，断开时随连接丢弃）
- 按哈希收集分片，偏移必须接在已收字节之后。
- 单个 blob 超过 32 MiB，或这条连接合计超过 64 MiB：这次上传记为"太大"。出现缺口：记为"乱序"。两种情况都立即释放已收的字节，之后的分片不再收。
- 分片不经方法表，按 `blob/put` 的 `write` scope 自己检查（含提权授予的 scope）。没有权限或头部解析不了的分片直接丢弃。
- `conn.rs` 的 `on_frame` 在 `initialize` 之后把 `0x02` 帧交给 `Dispatch::chunk`。

**2. `blob/put`**
- 在读循环里同步取出这个哈希的上传，再异步经 `BlobStore::put` 存储；之后同一哈希的上传从头开始。
- 返回 `too_large` 的情况：声明的 `size` 超过 32 MiB，或上传被记为"太大"。
- 返回 `conflict` 的情况：乱序、字节数与 `size` 不符、内容与哈希不符。
- 成功时返回 `{hash}`。

**3. `blob/get`**
- 读出 blob，`offset`、`length` 截到 blob 范围之内（超出不报错）。
- 按 `BlobChunkHeader::MAX_DATA` 切片，单帧不超过 64,000 字节，经 `push_frame` 先发分片，future 完成后才写结果，所以分片一定在结果之前。
- 范围为空时也发一个空的、标了"最后一片"的分片。
- 不存在的 blob 回 `not_found`。

**4. 其他接线**
- `HOST_FEATURES` 加了 `blob.get`、`blob.put`。
- `Target::Blobs(BlobStore)` 由新的 `routes.blobs(store)` 登记，生产组装和分发测试都调用它。占位、`Target::serves()`，以及 UNIT-unsupported-set 允许表里 blob 的两行都删掉了。

**5. CLI 的 `Client`**
- 新增 `send_blob_chunk`，`blob/get` 的分片改为以 `Incoming::Blob` 给出（原来直接丢掉），`ClientError` 加了 `Frame` 一种。
- IT 用它们在真实宿主上收发分片，CLI 以后读录制导出也能用上。

## 关键取舍

- **截取越界不报错**：`offset`、`length` 超出 blob 范围时截到范围之内，不当参数错误。
- **空范围也发一片**：结果里的 `chunks` 不会是 0，客户端总能看到一个标了"最后一片"的分片。
- **出错就释放**：超限或乱序时立即释放已收的字节，内存不随坏的上传增长。
- **缓冲按哈希组织**：同一条连接上同时进行的多个上传互不影响。

## 验证方式

- **IT-gw-blob**（`tests/lifecycle.rs`，真实宿主，harness 客户端）：
  - 一条连接发了 3 片中的 2 片后断开，下一条连接用同一哈希完整上传，成功；
  - 15 万字节、3 片的 blob 往返，整段和 `offset` 70,000、`length` 10 的截取都一致；分片的哈希、偏移连续；
  - `size` 不符、哈希不符都回 `conflict`；`size` 超过 32 MiB 回 `too_large`；未知哈希回 `not_found`；
  - 握手协商出的特性里有 `blob.put`。
  - **连跑 10 次全绿**，每次约 1.2 秒。
- **单元测试**：缓冲的顺序拼接、缺口、单个超限后不再增长、合计超限，以及取走一份后腾出空间（用小的上限）。
- 分发测试、CLI 测试、clippy 全部通过。
- **`cargo xtask ci pr --record docs/tasks/T029-gateway-blob/ci/` 通过**，摘要 `pr-d2b87921fc90.json` 已提交：

  | 作业 | 结果 |
  |---|---|
  | rust | 1827 通过 |
  | protocol | 185 通过 |
  | ts | 424 通过 |
  | license | 1 通过 |
  | testdata-scan | 3 通过 |
  | ui | 跳过：截图依赖的路径没有改动 |

## 提交列表

```
de10c13 docs(task): record the T029 pr CI summary and final count
d2b8792 feat(host): blob/put and blob/get through the gateway
daeeda0 feat(cli): send and receive blob chunks
257131b docs(task): estimate T029
```

## 遗留问题 / 范围外发现

1. **host-client 的 `putBlob`、`getBlob` 没有做**（约 90 行，放不下，留给 T086、T145）。需要的形状：
   - `putBlob`：用 `crypto.subtle.digest` 算 sha-256，按 `MAX_BLOB_CHUNK_BYTES` 切片，经已有的 `HostPort.sendBlob` 发出，再请求 `blob/put`；
   - `getBlob`：用已有的 `onBlobChunk` 收集分片，按 `offset` 拼起来，收齐 `blob/get` 结果里给出的 `chunks` 数为止。
2. **订阅和历史没有登记进生产组装**：见开头第 1 条。
3. **缓冲的条目数没有上限**：每个刚发第一片就超限或乱序的哈希，会留下一个不占数据的空条目，直到 `blob/put` 或断开。需要的话加一个"每连接未决上传数"的上限。
4. 分支基于 a0e2d37，没有同步 main。