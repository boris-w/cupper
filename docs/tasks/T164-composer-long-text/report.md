# T164 汇报

- 执行者：子 agent（工程师；T029 的执行者接手）
- 分支：`feat/T164-composer-long-text`
- 完成日期：2026-10-20

## 做了什么

输入框里超过帧预算的长文本，现在先经 `putBlob` 上传，再以 `text_blob` 发出；上传失败时草稿放回并提示原因。新增 164 行、删除 12 行，在约 300 行的线内，CI 全部通过（含截图回归和 UI-csp-zero）。

**1. host-client 的 `HostClient.putBlob(bytes, mediaType)`**（`client.ts`）
- 用 Web Crypto 的 `crypto.subtle.digest` 算 sha-256，没有引入新依赖。
- 在当前连接上经 `HostPort.sendBlob` 发 `0x02` 分片：每片最多 `MAX_BLOB_CHUNK_BYTES`，最后一片标记 `last`；空内容也发一个空的最后一片。
- 然后请求 `blob/put`，返回 `{hash, size, media_type}`，正好就是 `text_blob` 需要的 `blob`。
- 宿主回的 `too_large`、`conflict` 以 `RpcFailure` 抛出，带语义码。没有连接时立即以 `ConnectionLost` 拒绝。
- `getBlob` 按任务书不做。

**2. 输入框**（`composer/send.ts` 的 `useSend().send`）
- 内容块组装好之后、`client.command` 之前，拿整条请求参数 JSON 的 UTF-8 字节数去比帧预算。
- 帧预算取握手里的 `limits.max_frame_bytes`，没有握手结果时用协议常量，再减 1 KiB 留给信封和 `command_id`。
- 超过预算时，把文本上传为 blob（`text/plain`），在原位置换成 `{type: "text_blob", blob}`，附件原样保留；`session/send`、`session/create` 都经过这里。
- 上传放在原有的 `try` 里，失败时走现有路径：草稿和附件放回（只在用户没有接着打字时）、toast"消息没有发出（错误码）"。

## 关键取舍

- **只换文本**：附件（`file_ref`、`review_comment`）很小，原样保留。换成 blob 之后还超的，交给宿主回 `too_large`，同样走失败路径。
- **断线的情况**：分片发出之后、`blob/put` 之前连接断了，请求以连接丢失结束；重连后宿主那边已经没有这批分片，所以不自动重试，交给用户再发一次。
- **媒体类型用 `text/plain`**。

## 验证方式

- **`putBlob` 的单元测试**（`host-client/src/blob.test.ts`，mock 宿主）：
  - 15 万字节切成 3 片，偏移连续，最后一片才标 `last`，每帧不超过 64,000 字节，拼起来与原文一致，哈希与 Node 算出的 sha-256 相同；`blob/put` 的参数就是返回的 `BlobRef`；
  - 空内容只发一个空的最后一片；
  - `conflict`、`too_large` 以 `RpcFailure` 带码给出；没有连接时以 `ConnectionLost` 拒绝。
- **UNIT-composer-long-text**（`composer.test.tsx`，沿用 `setup()`，`setup()` 多返回了 `mock` 和 `conn`）：
  - 9 万字节的文本先上传：`conn.frames` 里有两片，哈希一致；`session/send` 的内容是 `text_blob`；
  - 短文本照常作为 `text` 发出，不上传；
  - `blob/put` 回 `too_large` 时没有发 `session/send`，草稿放回，并提示"消息没有发出（too_large）"。
- ESLint（含 T136 的规则）、tsc、prettier 都通过。
- **`cargo xtask ci pr --record docs/tasks/T164-composer-long-text/ci/` 通过**，摘要 `pr-08c520f4d235.json` 已提交：

  | 作业 | 结果 |
  |---|---|
  | rust | 1851 通过 |
  | protocol | 185 通过 |
  | ts | 436 通过 |
  | license | 1 通过 |
  | testdata-scan | 3 通过 |
  | ui | 16 通过（UI-csp-zero 为 pass） |

- **规模**：

  | 部分 | 新增 / 删除 |
  |---|---|
  | `putBlob` | 24 / 1 |
  | `send.ts` | 30 / 7 |
  | `putBlob` 的单元测试 | 64 / 0 |
  | 组件测试 | 46 / 4 |

## 提交列表

```
bed99b2 docs(task): record the T164 pr CI summary
08c520f feat(ui-core): send text past the frame budget as a text_blob
28455b8 feat(host-client): putBlob
9dec7de docs(task): estimate T164
```

## 遗留问题 / 范围外发现

1. **`getBlob` 没有做**：留给第一个在界面里读 blob 的任务。可以用已有的 `onBlobChunk` 按偏移收集分片，收齐 `blob/get` 结果里给出的 `chunks` 数为止。
2. 分支基于 e4c3eb4，没有同步 main。