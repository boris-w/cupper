# T017 工作笔记

## 行数估计（开工前）

### 估算方法

T011 的估计偏低约 40%，原因是文档注释和开放枚举比预想的多。这次直接按 T011 `index.rs` 的实际密度折算：15 个结构体、16 个开放枚举、约 60 个字段，共 342 行。由此得到：

| 写法 | 行数 |
|---|---|
| 结构体本身（derive、文档、括号、空行） | 约 4.5 行 |
| 字段（必填约 2 行，可选约 3 行） | 平均约 2.3 行 |
| `open_enum!` | 约 6 行，每个取值另加 1 行，带文档的取值再加 1 行 |

测试按 T011 的实际情况单独估算：T011 的测试估计 150 行，实际写了 475 行。

### 按模块估计

| 模块 | 内容 | 类型数（结构体 / 联合 / 枚举） | 代码 | 测试 |
|---|---|---|---|---|
| `session.rs`（新） | 会话流里条目和交互请求以外的事件：`session.configured`（`project_config`、`confirmation_required`）、`workspace.*` 3 种、`turn.started`/`completed`（时间字段、`final_item_ids`、用量、`error_code`）、`queue.updated`（条目带 `content`、`reply_to`、`created_by`）、`snapshot.recorded`（`stat`、`reason`）、`usage.updated`；`DiffStat`、`ExcludedPath` 供 T013 的 `git/diff` 复用 | 16 / 0 / 11 | 320 | 20 |
| `item.rs`（新） | `item.started`/`delta`/`updated`/`completed`；条目联合（`kind` 作标签，8 种）；`ToolCall`（16 个字段）及按 `tool_kind` 的 `missing_required`；`late_tool_output`；`plan`、`notice`、`error`；共用的 `ItemPatch`（约 20 个可选字段） | 16 / 1 / 11 | 470 | 60 |
| `interaction.rs`（新） | `interaction.requested`/`resolved`；请求联合（`permission`、`question`、`plan_confirm`、`auth_url`）；选项与 `effect`（`unknown`、`tier_after`、`rule`，`ends_turn` 为 `Option<bool>`）；回答联合与 `AnswerValue`；`request_digest` | 14 / 2 / 6 | 330 | 50 |
| `event.rs` 修改 | `EventBody` 加 16 个会话流变体、`stream_kind()` 分类、`Mergeable`、`x-mergeable` | — | 75 | 200 |
| `lib.rs` | 3 个 `mod` 声明 | — | 3 | — |
| **合计** | | | **约 1,200** | **约 330** |

测试（UNIT-proto-events-session）的 330 行：每种事件、每个条目和请求成员的往返样例约 120 行；未知类型、未知成员、未知枚举值约 40 行；残缺数据在严格和宽容两种模式下的结果约 40 行；按 `(item_id, field)` 合并及上限约 40 行；`x-mergeable` 只在 `item.delta` 上约 15 行；`request_digest` 约 35 行；`missing_required` 约 40 行。

**总计约 1,530 行，按 ±10% 算在 1,380 到 1,680 行之间**，贴着 1,500 行的上限，但不算明显超出，所以开工。控制办法：

- 文档注释只写字段名看不出含义的地方；测试样例写紧凑；
- 每完成一个模块统计一次；按已完成模块的实际偏差推算，总数预计超过约 1,650 行时，停下来汇报。

T011 拆分时对本任务的估计是约 1,100 行。多出来的部分主要是 `ItemPatch`、`ToolCall` 的可选字段（每个约 3 行），以及按 T011 实际情况上调的测试。
