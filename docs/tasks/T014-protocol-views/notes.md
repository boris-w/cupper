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

按守则第 8 条，每完成一个模块统计一次（`git show --shortstat <提交> -- crates/cupper-protocol`）：

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计（代码 + 测试） | 累计新增 |
|---|---|---|---|---|
| `view/mod.rs`（暂不含 `Window`、`OpLimits`）、`view/index.rs`（`2812bef`） | 316 | 140 | 约 285 | 316 |
| `view/session.rs`、`view/ops.rs`、`view/fixtures.rs`，`mod.rs` 补上 `Window`、`OpLimits`（`b8409a8`） | 946 / 2 | 350 | 约 645 | 1,262 |
| `view/props.rs`（`88feeeb`） | 272 | 272 | 230 | 1,534 |
| `sync.rs` 的订阅规则（`6911af0`） | 100 / 1 | 32 | 约 90（见下文"范围调整"） | 1,634 |
| 视图的 schema 检查（`3570017`） | 26 | 26 | 约 25 | 1,660 |
| 收尾：删掉没人用的方法、JSON 往返检查（`b583c3e`、`f5abcd6`） | 4 / 5 | 4 | — | **1,657 / 删 1** |

- 第二个模块完成时累计 1,262 行，比这两个模块的估计（约 930 行）多 36%。超出主要在测试（会话视图的单元测试 240 行、共用的测试构造函数 110 行）；代码多约 13%。
- 按这个偏差推算，原范围的总数约 1,860 行，超过 1,650 行的停止线。按开工前写好的预案调整范围（见下一节），推算降到约 1,560 行，于是继续。
- 最终新增 1,657 行、删除 1 行：代码约 838 行，测试约 819 行。比 1,500 行的上限多约 10%，超出部分都在测试。

## 范围调整：客户端同步状态机交给 T045

开工前的估计里有一个客户端同步核心 `StreamSync<V>`（快照分页组装、连续性检查、`stream/reset`、快照时整体重置，约 210 行）。第二个模块完成后推算总数超过停止线，按预案没有做，原因：

- 计划里 T045 的范围本来就写着"迁移 SP-4a 的 client 与 mirror：……订阅与连续性、epoch 重置"，SP-4a 的 `Mirror` 正是这个状态机。放在 T014 会和 T045 重复。
- T014 交付的是规则本身：`subscribe_mode`（宿主侧的四种情况）、`SubscribeMode`/`ResyncReason`（`SubscribeMode::Snapshot` 的文档写明客户端要整体重置）、已有的 `StreamCursor`（连续性检查），以及两个视图。T045 用它们写 `Mirror`，TS 的 `host-client` 照同样的规则实现。

测试 #22 的两条性质都做了，没有交给 T026。

## 实现时的取舍

**视图的 JSON 形状**（TS 视图要产出同样的 JSON）

- 可选的值缺省时不出现；map、列表、字符串总是出现。
- `IndexView`：`host?`、`agents?`、`clients?`（各自最新的一条），`projects`、`sessions`、`elevations`（按 ID 的 map，只有待处理的提权）。`notification.intent` 不进视图。
- `SessionView`：`configured?`、`capabilities?`、`workspace?`（最新的 `workspace.*`，用 `state` 区分）、`queue?`、`usage?`；`turns`（按 `turn_no` 升序，每个回合有 `started`、`completed?`、`snapshots`、`entries`）；`tail`；`items`、`requests`（按 ID 的 map）；`detached`；`late_by_ref`。
- `entries` 是 `{"item": id}` 或 `{"request": id}` 的列表，保持到达顺序（5.3.3 "条目和交互请求按原始的相对顺序排列"）。
- `ItemView` 是 `{item: ItemStarted（应用过全部补丁）, text, output, end?}`。`text`、`output` 分别是两种 delta 拼起来的文本，不放进条目本身，因为 `ToolCall` 自己有 `output` 字段；输出完成后也不清空，界面按 `output_state` 选择显示哪个（5.3.11）。
- `RequestView` 是 `{requested, resolved?}`，状态取 `resolved.status`，没有时是 `pending`。

**没有 `turn_id` 的条目放在哪里**（设计没写，需要编排者确认）

- 有进行中的回合时并入它；没有时放进 `tail`（最后一个回合之后）。下一个**新**回合（`turn_no` 大于已有的全部回合）开始时把 `tail` 收进自己的 `entries`。
- 并入之后，条目的 `turn_id` 改成这个回合。这样回合结束时写的回合快照就带着它，`history/page` 也能把它放回原处；否则历史页里的条目没有 `turn_id`，`apply` 无从判断它属于哪个回合（我试过几种按"应用时的上下文"判断的写法，都会在"历史页分页、中间插进实时事件"时出错）。
- 交互请求的 `digest` 是宿主按原始数据算的、视图原样保存，改 `turn_id` 不影响回答时的核对。

**窗口**（5.2.8、5.3.9）

- 从新到旧数已结束的回合，最多 20 个、条目合计不超过 2,000 个，遇到第一个放不下的就停，更早的全部淘汰。"条目"只数 item，不数交互请求。
- 进行中的回合和 `tail` 永远保留，也不计数。
- **最新的一个已结束回合总是保留**，即使它一个就超过 2,000 个条目。否则在没有进行中回合时，窗口里一个回合都没有，历史页送来的那个回合会被当成"新回合"把 `tail` 收走。
- `trim` 由宿主在回合结束时调用，客户端不淘汰（桌面端保留快照窗口加已翻的历史，5.7.6）。
- 被淘汰回合里仍待处理的请求进 `detached`，快照照样带上（5.3.3 "全部待处理的交互请求"）；它的回合经历史页回来时，重复的 `interaction.requested` 把它放回原位，保留它这期间的状态；解决之后，下一次 `trim` 把它删掉。

**apply 的其他规则**

- `ItemPatch`：出现的字段整体替换（5.3.11）；只设置条目这一类有的字段，其他字段忽略；未知类型的条目把全部字段并进去。`item.delta` 的 `field` 未知时忽略。
- 不认识的事件类型不改变视图；另一条流的事件、引用不存在的 ID、重复的 ID（含重复的 `turn_no`）都报错，客户端按缺口处理（重新订阅）。`IndexView` 删除不存在的项目、会话、提权也报错。
- `snapshot.recorded` 按 `phase` 每种保留一条，后写的替换先写的。

**快照 ops**

- 顺序：会话状态（配置、能力、工作区、队列、用量）→ `detached` 的请求 → 各回合（`turn.started`、`snapshot.recorded`、各条目、`turn.completed`）→ `tail`。
- 条目：`item.started` 带当前全部字段，文本按 `MAX_DELTA_ESCAPED` 拆成 `item.delta`，结束的条目加一个不带补丁的 `item.completed`。
- `item.started` 序列化后超过 48,000 字节（一页历史的上限，也给 `stream/snapshot` 的信封留了余量）时，把可选字段逐个拆成 `item.updated`。每个字段都是在一个实时事件里到达的，单独放得进一帧。
- `paginate` 按 JSON 字节贪心分页，单个 op 超过预算时独占一页。`stream/snapshot` 和 `history/page` 都这样分；`history/page` 的游标 `op_index` 就是页的起点。

**订阅规则**：先比 `epoch`（不同代次的 seq 无法比较），再看 `cursor_ahead`，最后看 `cursor_expired`。游标必须同时有 `epoch` 和 `after_seq`。

**测试编号**：计划只给了 PROP-view-coalesce、UNIT-view-window、UNIT-view-late-by-ref 三个编号，其余测试按内容归到前两个下面：

| 测试 | 编号 | 理由 |
|---|---|---|
| 测试 #22 ①：窗口快照加全部历史等于从头重放 | UNIT-view-window | 讲的就是窗口 |
| 测试 #22 ②：快照切分、分页后应用等于不切分 | PROP-view-coalesce | 与"合并后应用等于逐条应用"是同一类性质 |
| `IndexView` 的 apply 规则 | UNIT-view-window | 索引流没有淘汰，整个视图就是它的窗口 |
| `IndexView` 的快照分页 | PROP-view-coalesce | 同测试 #22 ② |
| 订阅的四种情况 | UNIT-view-window | 决定何时重发窗口 |
| 视图进 schema | UNIT-view-window | — |

需要更细的追溯时，请编排者在设计里加编号（例如 UNIT-view-index、UNIT-sync-subscribe）。

**性质测试的有效性**：给已提交的代码故意注入 5 种错误（不收 `tail`、不改 `turn_id`、拆分时丢字段、不保留最新的已结束回合、淘汰时不清 `late_by_ref`），每种都有测试失败。
