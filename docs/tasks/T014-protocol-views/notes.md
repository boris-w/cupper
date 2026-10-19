# T014 工作笔记

## 行数估计（开工前）

### 估算方法

T017 的代码估计基本准确（1,077 行，估 1,200），测试低估了约 70%（567 行，估 330）。本任务以逻辑为主、类型为辅，按 T017 `item.rs`、T010 `coalesce.rs` 的实际密度估代码；测试按"每个性质测试约 40 行，加上共用的事件序列生成器"单独估，并在 T017 的基础上留出余量。

### 按模块估计

| 模块 | 内容 | 代码 | 测试 |
|---|---|---|---|
| `view/mod.rs`（新） | `View` trait（`apply`、`snapshot_ops`）、`ViewError`、`Window`（20 个回合、2,000 个条目）、`OpLimits`（delta 与单个 op 的上限）、`op_bytes`、按字节分页的 `paginate` | 90 | 20 |
| `view/index.rs`（新） | `IndexView`：项目、会话摘要、agent 状态、宿主状态、客户端、待处理的提权；`apply`；快照 ops。`notification.intent` 不进视图 | 120 | 70 |
| `view/session.rs`（新） | `SessionView`：配置、能力、工作区、队列、用量；回合按 `turn_no` 排序；条目和交互请求按到达顺序排在所属回合里；没有 `turn_id` 的条目的归属；`ItemPatch` 的应用；`late_by_ref`；窗口裁剪；回合淘汰后仍待处理的请求 | 360 | 150（UNIT-view-window、UNIT-view-late-by-ref） |
| `view/ops.rs`（新） | 快照 ops 与回合快照 ops 的生成：长文本按 `MAX_DELTA_ESCAPED` 拆成 `item.delta`；超过单个 op 上限的 `item.started` 把可选字段拆到 `item.updated` | 110 | — |
| `view/props.rs`（新，仅测试） | 按宿主行为生成会话流事件序列的生成器；PROP-view-coalesce；测试 #22 的两条性质（窗口快照加全部历史等于从头重放；切分后应用等于不切分） | — | 230 |
| `sync.rs` 修改 | `SubscribeMode`、`ResyncReason`、订阅时的四种情况（纯函数）；客户端的同步核心 `StreamSync<V>`：快照分页组装、连续性检查、`stream/reset`、快照时整体重置 | 130 | 80 |
| `lib.rs` | 1 个 `mod` 声明 | 1 | — |
| **合计** | | **约 810** | **约 550** |

**总计约 1,360 行（±10%：1,220–1,500）**，超过计划的 M（400–900 行），但在约 1,500 行的上限以内。

**停止线**：每完成一个模块统计一次（`git diff --shortstat main..HEAD -- crates/cupper-protocol`）。按已完成模块的实际偏差推算，总数预计超过 **1,650 行**时，停下来汇报。到时可以拆出去的部分：`StreamSync` 交给 T045（Mirror）；测试 #22 的两条性质交给 T026。

## 实际行数

| 完成的模块 | 新增 | 其中测试 | 估计（代码 + 测试） | 累计新增 |
|---|---|---|---|---|
