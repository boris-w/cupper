# T174 笔记

## 日志在哪里、记了什么

- 网关把每次订阅发成审计记录 `stream.subscribed`（`gateway/audit.rs`），审计 writer 只把它写进 host.log，不进 `audit` 表（`supervisor/audit.rs`）。一行 JSON，`fields.detail` 是 `{"stream", "mode", "reason", "lag"}`；`lag` 只在游标带着本宿主的 epoch、且没有超过 head 时才有。
- 自用构建的数据目录是 `Application Support/cupper-local`，日志按 `diag/log.rs` 的 `log_dir` 在 `Library/Logs/cupper-local/host.log`，10 MB × 5 轮转。

## 统计（2026-11-23；只用脚本输出计数，原文没有打印、摘录或保存，脚本用完已删）

- 只有一个 host.log（约 19 KB、84 行），没有轮转出的旧文件，只覆盖 2026-11-10 约一个半小时。
- `stream.subscribed` 10 次：index 5 次，会话流 5 次（流名被脱敏成 `<redacted>`，靠排除法判断）。
- 10 次全是不带游标的快照（`reason`、`lag` 都为空）；续传 0 次，`cursor_expired`、`epoch_changed`、`cursor_ahead` 都是 0。
- 游标落后量没有样本：p50、p90、最大值都无从计算；落在保留边界附近的 0 次。

## 结论

- 窗口 20 个回合、日志 1 万条或 7 天、原始帧 14 天且不超过 512 MB 都没被这段使用触及，没有调整的依据，不改；复核结果记进 5.13 Q16。
- 补 PROP-retention-boundary（`store/log.rs`）：随机的事件时间和保留条数、时长下 prune，留下的是连续到 head 的一段，含最近 `events` 条和所有不老于 `age` 的事件，`cut + 1` 是保留的第一条；游标正好在 `first_seq − 1` 续传，再往前一条是 `cursor_expired`，在 head 续传，`head + 1` 是 `cursor_ahead`。

## 发现（不在本任务改）

- 会话流的流名在 host.log 里被整段脱敏：`detail` 字段按长字符串处理，会话 ID 被替换。以后按流的种类统计，需要 detail 带流的种类，或者流名免于脱敏。
- 日志只覆盖 11-10 那一个半小时，作者平时用的是否是这个构建，由编排者向作者确认；在此之前不读别的目录。
