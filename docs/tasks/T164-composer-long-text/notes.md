# T164 工作笔记

- 执行者：子 agent（工程师；T029 的执行者接手）

## 开工前核对现状

- `HostClient` 持有 `#port`（`HostPort`，已有 `sendBlob`）。`blob/put` 是"天然"幂等，走 `request`，不走 `command`。
- 窗口桥按握手的 `max_pending_upload_bytes` 放行分片，`blob/put` 之后清掉这个哈希的计数（T076）。
- 输入框：`composer/send.ts` 的 `useSend().send(text, attachments)` 是唯一组装内容块的地方。`useHost().handshake` 可能为空（组件测试里就是），这时帧预算取协议常量 `MAX_FRAME_BYTES`。
- `text_blob` 块的形状是 `{type: "text_blob", blob: {hash, media_type, size}}`。

## 开工前的规模估计

| 模块 | 内容 | 草估 |
|---|---|---|
| `HostClient.putBlob` | Web Crypto 算 sha-256，按 `MAX_BLOB_CHUNK_BYTES` 切片发出，空内容也发一个最后一片，再 `blob/put`；返回 `BlobRef` | 35 |
| 输入框 | 按整条请求参数 JSON 的 UTF-8 长度对照帧预算（留 1 KiB 给信封和 `command_id`），超过就上传文本、换成 `text_blob`；失败沿用现有路径 | 30 |
| **实现小计** | | **65**，乘 1.1 约 70 |
| `putBlob` 的单元测试 | 切片、偏移、最后一片、哈希、`blob/put` 的参数；`conflict`、`too_large`；空内容 | 70 |
| UNIT-composer-long-text | 超过预算走上传和 `text_blob`；没超过照常走文本；上传失败草稿放回、有提示 | 70 |
| **合计** | | 约 **210** |
