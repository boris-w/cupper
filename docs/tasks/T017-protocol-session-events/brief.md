# T017 协议事件目录 II：会话流、条目、ToolCall 与交互请求

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T017-protocol-session-events`
- 依赖：T011（已合并）。M1a，关键路径：T011 → **T017** → T014 → …

## 背景

T011 开工前按模块估算发现会超过规模上限，于是拆成两个任务（见 `docs/tasks/T011-protocol-events/notes.md`）。T011 已经完成事件基础、领域类型与索引流，本任务做剩下的会话流部分。这些契约要在 M1a 冻结（设计 5.12.5 第 2 组），冻结之后只能附加，不能修改。

## 范围

**做**（计划 T017 一行；设计 5.3.5、5.3.6、5.3.11）：
- **会话流的全部事件**：
  - `session.configured`（含 `project_config`、`confirmation_required`）、`session.capabilities`；
  - `workspace.*`（`workspace.failed` 按 5.3.11 第 6 条）；
  - `turn.started`、`turn.completed`（时间字段、`final_item_ids`、用量、`error_code`）；
  - `item.started`、`item.delta`、`item.updated`、`item.completed`（共用 `ItemPatch`，见 5.3.11 第 5 条）；
  - `queue.updated`（条目带 `content`、`reply_to`、`created_by`）；
  - `snapshot.recorded`（`stat`、`reason`）、`usage.updated`。
- **条目联合**：用 `open_union!` 的 `in tag "kind"` 形式，包括：
  - `user_message`（`reply_to`）、`agent_message`、`thought`；
  - `tool_call`：`ToolCall` 的全部字段，以及按 `tool_kind` 的必填字段表 `missing_required`；
  - `late_tool_output`：`output` 可选（5.3.11 第 3 条）；
  - `plan`、`notice`、`error`。
- **交互请求**：
  - `interaction.requested`、`interaction.resolved`；
  - 请求联合：`permission`、`question`、`plan_confirm`、`auth_url`；
  - `Option` 与 `Effect`：含 `unknown`、`tier_after`、`rule`，`ends_turn` 用 `Option<bool>`；
  - 回答联合；`request_digest`。
- **合并**：`item.delta` 的 `field: text|output`，按 `(item_id, field)` 合并，实现 `Mergeable`，并标注 `x-mergeable`。

**不做**：方法（T012、T013）；视图（T014）；schema 导出（T015）。

## 验收标准

- [ ] 新增 UNIT-proto-events-session：会话流事件、条目、交互请求都能往返，覆盖未知类型、未知枚举值，以及残缺数据在严格和宽容两种模式下的表现；`item.delta` 按 `(item_id, field)` 合并，并有合并上限；`x-mergeable` 只出现在 `item.delta` 上；`request_digest` 的计算。
- [ ] 汇报里附上 5.12.5 第 2 组中本任务负责部分的对照表。T011 汇报里已经列出它负责的部分，两边合起来要覆盖完整。
- [ ] 仍能编译到 `aarch64-apple-ios-sim`。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T017-protocol-session-events/ci/` 通过，摘要随任务提交。
- [ ] 测试名以 `unit_proto_events_session_` 开头。

## 交接事项（来自 T011）

- 在 `EventBody` 里加会话流的变体。`stream_kind()` 是穷尽的 `match`，新变体不分类就编译不过。
- `item.delta` 变体加 `#[schemars(extend("x-mergeable" = true))]`，并为 `EventBody` 实现 `Mergeable`。
- 测试复用 `testutil::{roundtrip, open_values}`。
- 惯例：
  - `lib.rs` 只放 `mod` 声明；
  - ID、时间、路径分别用 `ids`、`TimestampMs`/`DurationMs`、`WirePath`，以便自动带上注解；
  - 协议里的整数不超过 2^53−1。

## 规模（重要）

T011 的拆分估计本任务约 1,100 行。**开工前先写一份模块级的行数估计，放在 `notes.md`。** 动手之后，每完成一个模块统计一次行数（`CLAUDE.md` 守则第 8 条）。预计会明显超出约 1,500 行的上限时，先停下来汇报。

## 参考资料

- 设计：`03-protocol-v1.md` 5.3.5、5.3.6、5.3.11；`12-milestones.md` 5.12.5。
- T011 的代码、`notes.md` 和 `report.md`（`docs/tasks/T011-protocol-events/`）；SP-4a 的 `events.rs`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。
