# T011 协议事件目录与领域类型

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T011-protocol-events`
- 依赖：T010（已合并）。M1a 第 2 波，处在关键路径上：T010 → **T011** → T014 → T026 → …

## 背景

宿主下发的全部语义都靠事件表达。客户端只负责排版和本地化，这是"瘦客户端"的约定（设计 5.3.5）。本任务在 `cupper-protocol` 里定义两条流的全部事件和领域类型，包括 M1b 才会产生的那些。契约必须在 M1a 冻结（5.12.5 第 2 组），冻结之后只能附加，不能修改。

## 范围

**做**（计划 T011 一行；设计 5.3.5、5.3.6、5.3.7）：
- 索引流和会话流的全部事件。
- 条目类型：`user_message`（含 v1.5 的 `reply_to`）、`agent_message`、`thought`、`tool_call`、`plan`、`notice`、`error`、`late_tool_output` 等。
- `ToolCall` 和按 `tool_kind` 列出的必填字段表（含 `query`、`url`、`parent_item_id`、`output_state`、`output_tail`、`output_bytes`）；各条目的时间字段；`turn.completed.final_item_ids`；`item.delta` 的 `field: text|output`。
- 交互请求的数据和 `Option.effect`（含 `unknown`、`tier_after`、`rule`），`request_digest`。
- 能力描述 `SessionCapabilities`（含 `turn.interrupt_stops_commands`）。
- `PromptContent`（含 `review_comment`、`text_blob`）。
- `SessionSummary`，含 v1.4 的 `pending_kind`。
- `queue.updated`：条目带完整的 `content`（v1.4）和 `reply_to`（v1.5）。
- `AgentStatus`；`project.upserted.trust_state`；`session.configured`，含 `confirmation_required`、`project_config`；`snapshot.recorded`，含 `stat`、`reason`。
- 开放枚举、`Unknown` 兜底、严格模式开关：CI 里的金样例开严格模式，"类型已知但数据残缺"的事件直接失败；客户端不开，把这类事件降级成未知事件。
- 可合并的事件实现 T010 的 `Mergeable`，变体上标 `x-mergeable`。

**不做**：方法（T012、T013）；视图与同步规则（T014）；schema 导出与 TS（T015）。

## 验收标准

- [ ] 新增 UNIT-proto-events：每种事件都能往返，包括未知类型、未知枚举值、残缺数据（分别在严格模式和宽容模式下测）。
- [ ] 5.12.5 第 2 组里属于事件和领域类型的项，逐项都有定义；汇报里附一张对照表。
- [ ] `cupper-protocol` 仍能编译到 `aarch64-apple-ios-sim`。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T011-protocol-events/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（来自 T010，见 `docs/tasks/handoff-notes.md`）

- `Fallback{kind, summary}` 里的 `summary` 是界面文字，和"宿主只下发语义码"的原则冲突。请定下 `summary` 的填法，并写明理由，例如：只放语义码和参数；或者只作为最后兜底的英文短句。
- `ErrorData.resolved_by` 现在放在 `details` 里。定义好相关类型之后，以附加的方式改成具名字段。
- 惯例：
  - `lib.rs` 只放 `mod` 声明；
  - ID 用 `ids` 里的类型；
  - 时间用 `TimestampMs`、`DurationMs`，路径用 `WirePath`，这样 `x-id`、`x-time`、`x-path` 会自动带上；
  - 协议里的整数不超过 2^53−1。

## 规模（重要）

计划把本任务估为 L（900–1,500 行）。事件和类型很多，加上往返测试，很可能超出。**请先按模块列一个拆分计划，估好每部分的行数，写进 `notes.md` 并提交**，再开始写代码。

如果预计会明显超出 1,500 行，先停下来汇报，并附上建议的拆分点，例如：
- 索引流和领域类型；
- 会话流的条目；
- 交互请求和能力描述。

编排者会据此拆成两个任务。动手之后，按 `CLAUDE.md` 守则第 8 条，每完成一个模块统计一次行数。

## 参考资料

- 设计：`03-protocol-v1.md` 5.3.5、5.3.6、5.3.7；`12-milestones.md` 5.12.5；`07-desktop.md` 5.7.4、5.7.5：界面需要哪些字段，例如 `pending_kind`、请求面板的编号。
- ADR-04、ADR-06；SP-4a 的 `events.rs`；`docs/tasks/T010-protocol-base/report.md`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。

## 范围调整（2026-10-19，第 2 轮）

执行者开工前按模块估算，约 2,150 行，超出上限，于是停下来汇报（见 `notes.md`）。编排者采纳了它的拆分建议：

- **本任务（T011）收窄为"事件基础、领域类型与索引流"**，即 notes 里建议的 T011a：
  - `open_union!` 和严格/宽容两种解码；
  - `EventBody` 的骨架和所属流的判定；
  - 公共领域类型；
  - 索引流的全部事件；
  - `SessionCapabilities`；
  - `fallback.summary` 的填法；
  - `ErrorData.resolved_by` 改为具名字段。

  估计约 1,055 行。
- **新增 T017"会话流、条目、ToolCall 与交互请求"**，即 notes 里的 T011b，在本任务合并后派发。
- **用例编号**：原来的 UNIT-proto-events 拆成两个子编号。本任务产出 UNIT-proto-events-index，T017 产出 UNIT-proto-events-session。
- **设计空缺的 11 条建议和 `fallback.summary` 的填法**，编排者全部采纳，已写进设计 v1.7（5.3.11、5.4.4），按设计实现即可。
- **验收标准**调整如下：
  - UNIT-proto-events-index：覆盖索引流与领域类型的往返，包括未知类型、未知枚举值、残缺数据在严格和宽容两种模式下的结果，以及 `open_union!` 宏本身；
  - 汇报里的第 2 组对照表，只列本任务负责的部分；
  - 其余验收标准不变。
