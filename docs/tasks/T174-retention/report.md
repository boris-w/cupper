# T174 汇报：Q16 保留参数复核

结论：自用构建的日志里只有 10 次订阅，全部是不带游标的快照，没有任何续传样本，保留参数没有调整的依据，保持不变；补上了 PROP-retention-boundary。

PR #133：https://github.com/boris-w/cupper/pull/133 ，在 ffd09673 上 8 个检查全部通过：https://github.com/boris-w/cupper/actions/runs/45144879419

## 做了什么

- **确认日志位置**（先读代码）
  - 网关把每次订阅作为审计记录 `stream.subscribed` 发出，审计 writer 只把它写进 host.log（每行一个 JSON），不进 `audit` 表。
  - 记录的 `detail` 字段里有 `stream`、`mode`、`reason`、`lag`。
  - 自用构建的日志在 `Library/Logs/cupper-local/host.log`，按 10 MB × 5 轮转。
- **统计**：用临时脚本只输出计数，日志原文没有打印、摘录或保存，脚本用完已删，也没读别的目录。
  - 只有一个 host.log，约 19 KB、84 行，没有轮转出的旧文件，只覆盖 2026-11-10 约一个半小时。
  - `stream.subscribed` 共 10 次：index 5 次，会话流 5 次。
  - 10 次全是不带游标的快照，也就是窗口初次加载；续传 0 次，`cursor_expired`、`epoch_changed`、`cursor_ahead` 都是 0。
  - 游标落后量没有样本，p50、p90、最大值都算不出来；靠近保留边界的 0 次。
- **调整结论**：这段使用没有触及任何一个值（窗口 20 回合、日志 1 万条或 7 天、原始帧 14 天且不超过 512 MB），保持不变。复核结果写进了设计 5.13 Q16，写明要等积累了带游标的重连样本之后再复核。
- **PROP-retention-boundary**（`store/log.rs` 的测试）：在真实日志上随机生成事件时间、保留条数和保留时长，然后 prune，断言：
  - 留下的是连续到 head 的一段，最近 `events` 条和所有比 `age` 新的事件都在，`cut + 1` 就是保留的第一条；
  - 游标正好在 `first_seq − 1` 时续传，再往前一条时是 `cursor_expired` 快照；
  - 游标在 head 时续传，`head + 1` 时是 `cursor_ahead`。

## 关键取舍

- 只改文档、不改参数：数据完全没走到续传路径，改哪个值都没有依据。
- 第一次跑性质测试时，失败的原因是测试里借用的辅助函数一次最多只读 9 条，属于测试本身的写法问题，不是代码缺陷。改成用 SQL 计数后，没有提交它留下的 proptest-regressions 文件。

## 验证方式

- 本机：fmt 通过；cupper-host 在有和没有 test-harness 两种特性下 clippy 都过；store::log 的测试全过，含新的性质测试。
- PR 上 8 个检查全部通过。
- 规模：+48 行（停止线 200）。

## 提交列表

- ea4683f6 test(host): PROP-retention-boundary
- ffd09673 docs(design): Q16 reviewed in M1b, the values kept（含 notes；按你的要求 amend 过，去掉了具体的使用时刻）

## 遗留问题 / 范围外发现

1. **会话流名被整段脱敏**：host.log 里会话流的流名变成了 `<redacted>`，原因是 Redactor 把 `detail` 字段整个当长字符串处理，会话 ID 被替换掉了。现在只能靠排除法区分 index 和会话流。以后要按流的种类统计，需要让 detail 记录流的种类，或者让流名不被脱敏（按任务书不在本任务改，另立任务）。
2. **日志覆盖太短**：自用构建的日志只覆盖 11-10 那一个半小时。作者平时用的是不是这个构建，由你去确认；在作者回答之前，我没有读别的目录。复核要等积累了带游标的重连样本再做。
