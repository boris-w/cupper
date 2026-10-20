# T168 工作笔记

## 开工前的核对

- `handoff-notes.md` 里第一列含 T168 的行：T165（`Body`、`isEnabled(option, truncated)`、`Terminal`）。另有"第一个在界面里读 blob 的任务"两行：T029（`getBlob` 的约定：用 `onBlobChunk` 按 `offset` 收集，收齐 `blob/get` 结果里的 `chunks` 数；分片一定先于结果；空范围也有一片）、T082（用户气泡里的长文本块现在只显示"长文本"）；T164（`putBlob` 在 `HostClient` 里，sha-256 用 `crypto.subtle.digest`）。
- `tests/ids.toml` 的 T168 段：UNIT-blob-fulltext。
- 数据：请求面板的 `Content` 有 `text` 和 `blob {blob: BlobRef, preview?}` 两种（命令 `tool.command`、说明 `detail`、计划 `plan`）；路径列表是字符串数组，没有 blob 形态。用户消息里的长文本块是 `text_blob {blob: BlobRef}`，没有预览，只有大小。

## 规模估计（开工前）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `getBlob` | `blob/get`，按 `offset` 收集分片，核对片数、连续性和 sha-256 | 45 | 50 |
| 全文的 hook | 文本直接可用；blob 按需下载，载入中、失败可重试；请求结束后丢掉结果 | 30 | — |
| 请求面板 | 权限请求的内容改成组件（hook 不能放在分支里）；命令和说明都能展开下载；展开且全部载入后才传 `truncated={false}`，收起或失败时回到"只能拒绝" | 55 | 50 |
| 用户气泡 | 长文本块：显示大小和"显示全文"，点开才下载，载入后展开显示，沿用折叠 | 40 | 30 |
| 文案 | i18n | 10 | — |
| 合计 | | 180 | 130 |

合计约 **310 行**，停止线约 350。
