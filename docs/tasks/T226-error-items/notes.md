# T226 notes

## 规模估计（开工前）

停止线 350，硬停止线 450（`cargo xtask size` 只计新增行）。

| 模块 | 实现 | 测试 |
|---|---|---|
| `orchestrator/failure.rs`：`Failure`、`FailureCode → (Guidance, retryable)` 映射、stderr 尾部截取 | 60 | 35（映射表、尾部截取） |
| `orchestrator/mod.rs`：notice 的"即时条目"抽成通用函数，error 条目共用 | 8 | — |
| `turn.rs`：`Outcome` 带 `Failure`，`end()` 在 Settle 前写 error 条目；`AdapterExited` 带上 stderr | 20 | — |
| `core.rs`：error 条目前先 Interject（缓冲的文字先出、活动组结束） | 5 | — |
| 会话 actor 与依赖：agent 错误文本进核心前完整脱敏 | 20 | — |
| IT-error-item（会话层：三个时机、不重复、脱敏） | — | 110 |
| 既有用例随事件序列调整 | — | 20 |
| E2E-error-guidance（nightly） | — | 55 |
| 合计 | ~113 | ~220 |

预计约 330，贴近停止线；测试写紧凑。

## 实现记录

- 第一个模块（`failure.rs`、回合机、核心、actor 的脱敏、IT-error-item）完成后 `cargo xtask size` 为 +345。超出预估的主要是 `failure.rs` 的文档和 rustfmt 展开的断言；把映射表用例改成一张字符串表、去掉与 IT 重复的 `Failure::of` 单元断言、actor 里就地脱敏（不另写函数）之后回到 +303。
- E2E-error-guidance 加上后为 +337。

## 取舍

- **写在哪里**：`error` 条目由回合机在 `end()` 里写，位置在 Settle 之前。回合机对每个回合只调用一次 `end()`（之后回合进入 `Ending` 或结束），所以"同一个回合不重复写"是结构保证，不需要另记标志。写在 Settle 之后会违反 `session_props` 的不变式"条目在回合 Settle 之前开始"。
- **Interject**：核心在把 `error` 条目交给 actor 之前，先给合并器一个 `Interject`（与交互请求相同）：缓冲中的文字先写出，活动组结束。条目顺序因此是"已有的文字 → error 条目 → 回合收尾 → 结束快照 → `turn.completed`"。
- **三个时机**：
  - attach 返回错误：回合处于 `Preparing` 时以该错误结束（CLI 缺失、没登录、运行时失败、`project_untrusted` 等都在这里）；
  - prompt 返回错误：`Prompting` 或 `Cancelling` 中；K6 的"会话找不到、重发一次"不结束回合，不写；
  - 适配器意外退出：`Prompting`、`Cancelling`、`Autonomous` 中；`raw` 取 stderr 尾部最后 2,000 个字符，从行首开始，单行超长时截在字符边界。空闲时适配器退出只照旧写 `agent_restarted`，没有回合失败，不写 error 条目。
- **脱敏**：`AgentError.message` 原注释写"截断、脱敏"，实际 Claude、Codex 的分类器只截断（`MAX_LABEL_CHARS`），没有脱敏。改为由会话 actor 在 attach 结果和 `PromptFailed` 进入核心之前，用完整脱敏（`Redactor::for_this_machine`，与适配器日志同一套规则）处理 message。stderr 尾部在读入时已经过同一个 Redactor，不再处理。
- **映射**（`guidance()`，穷举匹配，新增 `FailureCode` 会编译失败）：与初稿相同，另外两处取舍：
  - `host_restarted` → `retry`（初稿归"其余 → none"）：回合被宿主重启打断，重发就是恢复办法；目前宿主不以这个码写失败回合，只是先定下映射；
  - `project_untrusted`、`other`、未知取值 → `none`：信任项目走信任流程，没有对应的 guidance。
  - `retryable` 只在 guidance 为 `retry` 时为真：表示"不做别的、原样重发就可能成功"。登录、升级、换模型之后也要重发，但需要用户先做事，记为 false。
- **E2E**：`stderr e2e-boom; sleep 100; exit 3`。stderr 的读取任务与实例结束是两条路径，`end()` 取尾部时不等读取任务读到 EOF；脚本里停 100 ms，保证那一行先被读到。产品上理论上可能丢最后几行，见汇报的遗留问题。
