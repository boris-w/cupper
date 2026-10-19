# T014 汇报

- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T014-protocol-views`
- 完成日期：2026-10-19

**结论**：两个视图、`apply`、窗口、`late_by_ref`、快照 ops 的生成与切分、订阅规则都做完了。三个验收编号和测试 #22 的两条性质都通过；能编译到 `aarch64-apple-ios-sim`；在干净的 worktree 里跑 `cargo xtask ci pr --record` 也通过了。

**请编排者确认两件事：**
1. **规模超了约 10%。** `crates/cupper-protocol` 新增 1,657 行、删除 1 行，其中代码约 838 行，测试约 819 行，超出的部分都在测试。第二个模块写完时，按当时的偏差推算总数约 1,860 行，超过了 `notes.md` 里定的 1,650 行停止线。我按开工前写好的预案收窄了范围，推算降到约 1,560 行后继续做完。每个模块的行数记在 `notes.md`。
2. **范围调整：客户端同步状态机交给 T045。** 这部分包括快照分页组装、事件流程和 `stream/reset` 处理。计划里 T045 的范围本来就是"迁移 SP-4a 的 client 与 mirror：……订阅与连续性、epoch 重置"，放在 T014 会和它重复。T014 交付规则本身，T045 用它们写 `Mirror`：
   - `subscribe_mode`：宿主侧的四种订阅情况，纯函数；
   - `SubscribeMode`、`ResyncReason`：`Snapshot` 的文档写明，不论原因，客户端都整体丢弃视图和游标；
   - 已有的 `StreamCursor`：连续性检查。

## 做了什么

- **`view/mod.rs`（新）**：
  - `View` trait（`apply`、`snapshot_ops`）、`ViewError`（`Missing`、`Duplicate`、`OtherStream`）；
  - `Window::V1`：20 个回合、2,000 个条目；
  - `OpLimits::V1`：delta 用 `MAX_DELTA_ESCAPED`，单个 op 不超过 48,000 字节，也就是一页历史的上限；
  - `op_bytes`；`paginate`：按 JSON 字节贪心分页，`stream/snapshot` 和 `history/page` 共用。
- **`view/index.rs`（新）**：`IndexView`。
  - 项目、会话、待处理的提权按 ID 存成 map；`host`、`agents`、`clients` 各保留最新的一条。
  - `notification.intent` 不进视图。删除不存在的项是错误；另一条流的事件也是错误。
- **`view/session.rs`（新）**：`SessionView`。
  - 配置、能力、工作区、队列、用量。
  - 回合按 `turn_no` 升序，每个回合的条目和交互请求保持到达顺序。
  - `items`、`requests` 按 ID 存成 map；另有 `tail`、`detached`、`late_by_ref`。
  - `ItemPatch`：出现的字段整体替换，只设置这一类条目有的字段；未知类型的条目把全部字段并进去。
  - `trim(&Window)`：宿主在回合结束时调用。
- **`view/ops.rs`（新）**：
  - `snapshot_ops_within`、`turn_ops`（回合快照，供 T026 存储和服务历史）。
  - 文本按上限拆成 `item.delta`；超大的 `item.started` 把可选字段逐个拆成 `item.updated`。
- **`sync.rs`**：`SubscribeMode`、`ResyncReason`、`Resume`、`LogExtent`、`subscribe_mode`。判断顺序：先比 epoch，再看 `cursor_ahead`，最后看 `cursor_expired`。
- **测试**：
  - `view/fixtures.rs`：测试用的事件构造函数；
  - `view/props.rs`：按宿主行为生成会话流事件的生成器和各条性质测试。
- 视图实现了 `JsonSchema` 和 `Deserialize`；JSON 往返检查已通过，T026 可以直接把视图存成检查点。

## 关键取舍

设计没写到、由我定下的规则如下，完整说明见 `notes.md`：

- **没有 `turn_id` 的条目**（notice、回合外的请求）：
  - 有进行中的回合就并入它，否则进 `tail`；下一个**新**回合（`turn_no` 比已有的都大）开始时把 `tail` 收进去。
  - 并入后，条目的 `turn_id` 改成这个回合的 ID。这样回合快照带着它，历史页也能把它放回原处。按"应用时的上下文"判断归属的几种写法，在"历史页中途分页、中间夹着实时事件"时都会出错。
  - 请求的 `digest` 原样保存，回答时的核对不受影响。
- **窗口**：
  - 从新到旧数已结束的回合，最多 20 个、条目合计不超过 2,000 个，遇到第一个放不下的就停。只数 item，不数请求。
  - 进行中的回合和 `tail` 永远保留。
  - **最新的一个已结束回合总是保留**，即使它一个就超过 2,000 个条目。否则窗口里可能一个回合都没有，历史页送来的回合会被当成新回合，把 `tail` 收走。
- **被淘汰回合里仍待处理的请求**进 `detached`，快照照样带上，满足"全部待处理的交互请求"。它的回合经历史页回来时，重复的 `interaction.requested` 把它放回原位，并保留期间的状态；请求解决后，下一次 `trim` 把它删掉。
- **视图 JSON**：可选的值缺省时不出现；map、列表、字符串总是出现。`ItemView` 是 `{item, text, output, end?}`。delta 文本单独存放，因为 `ToolCall` 自己有 `output` 字段；输出完成后也不清空，界面按 `output_state` 选择显示哪个。

## 验证方式

| 验收标准 | 结果 |
|---|---|
| PROP-view-coalesce：合并后应用等于逐条应用 | 通过。`prop_view_coalesce_merged_events_apply_like_single_ones`：合并上限随机取 6–200，事件逐个经过 `StreamCursor` |
| UNIT-view-window：淘汰、进行中的回合不淘汰、向前插入历史时按 `turn_no` 排序 | 通过。分别是 `unit_view_window_trims_finished_turns_and_keeps_the_running_one`、`..._history_inserts_older_turns_in_order`、`..._keeps_pending_requests_of_evicted_turns` |
| UNIT-view-late-by-ref | 通过。反向索引能关联到原条目；原回合不在窗口里时，晚到条目本身仍带标题和命令；晚到条目也被淘汰后，索引项随之删除，历史页回来时再加上 |
| 测试 #22 ①：窗口快照加全部 `history/page` 等于从头重放 | 通过。`unit_view_window_snapshot_and_history_replay_the_log`：窗口大小和页大小随机，回合快照在回合结束时保存 |
| 测试 #22 ②：快照切分后应用等于不切分 | 通过。`prop_view_coalesce_split_snapshot_applies_like_unsplit`：delta 上限 6–40、op 上限 100–600、页大小随机 |
| 编译到 `aarch64-apple-ios-sim` | debug 和 release 都通过 |
| `cargo xtask ci pr --record` | 在干净的 worktree 里于 `1d6cbab` 运行，通过：rust 266 项、ts 64 项、license 1 项。PROP-view-coalesce、UNIT-view-window、UNIT-view-late-by-ref 都是 pass。摘要是 `docs/tasks/T014-protocol-views/ci/pr-1d6cbab4d6f7.json`，不含路径和用户名。`cupper-sys` 的偶发失败这次没有出现 |
| 测试命名 | 新测试都以 `prop_view_coalesce_`、`unit_view_window_`、`unit_view_late_by_ref_` 开头 |

另外：
- 视图进 schema：有测试固定两个视图的必填字段，并确认整数有上下界。
- 两个视图的 JSON 往返都已检查。
- `clippy --all-targets -D warnings`、`RUSTDOCFLAGS="-D warnings" cargo doc` 都通过；没有新增依赖。
- **性质测试的有效性**：往已提交的代码里注入了 5 种错误，每种都有测试失败。这 5 种是：不收 `tail`、不改 `turn_id`、拆分时丢字段、不保留最新的已结束回合、淘汰时不清 `late_by_ref`。

## 提交列表

```
3562432 docs(task): record the T014 pr CI summary
1d6cbab docs(task): record the T014 implementation notes
f5abcd6 test(protocol): check that both views survive a JSON round trip
b583c3e refactor(protocol): drop the unused SessionView::pending_requests
3570017 test(protocol): check that the views' JSON is in the schema
6911af0 feat(protocol): add the stream/subscribe rules
88feeeb test(protocol): add PROP-view-coalesce and the window replay properties
b8409a8 feat(protocol): add SessionView with its window and snapshot ops
2812bef feat(protocol): add the View trait, op paging and IndexView
fed1262 docs(task): add the T014 module plan and line estimate
```

分支从 `3c5b6e1` 拉出。之后 main 合入了 T024、T021 的修复和 T012 的任务书，但都没有改 `crates/cupper-protocol`。

## 遗留问题 / 范围外发现

1. **规模**：1,657 行，超出上限约 10%，原因和处理见开头。
2. **请编排者写进设计的规则**（5.3.3、5.3.11，TS 的 reducer 要照做）：没有 `turn_id` 的条目怎样归属；窗口总是保留最新的已结束回合；`detached`；"2,000 个条目"只数 item。
3. **宿主要守的不变式**（T043、T026）：
   - 引用某个回合的事件都要写在它的 `turn.completed` 之前，包括 `snapshot.recorded` 的 `phase: end`、被放弃条目的 `item.completed`、它的请求的 `interaction.resolved`。回合快照在 `turn.completed` 时生成。
   - 请求要在所在回合结束前解决。回合内的请求已经满足：中断时标 `cancelled`，适配器退出时标 `expired`。没有 `turn_id` 的请求，如果在接收它的回合结束后才解决，而且解决前那个回合已被淘汰，该回合的历史页会把它显示成 pending。建议宿主照 K9 为它开一个 autonomous 回合，并让这个回合一直保持到请求解决；或者在回合结束时取消它。
4. **测试编号**：`IndexView`、订阅规则、schema 检查都没有自己的编号，暂时归到 UNIT-view-window 或 PROP-view-coalesce，对照表在 `notes.md`。需要更细的追溯时，请在设计里加编号，例如 UNIT-view-index、UNIT-sync-subscribe。
5. **交接**：
   - **T012**（正在并行）：`stream/subscribe` 的结果请复用 `sync::SubscribeMode`、`sync::ResyncReason`，不要另定义。`lib.rs` 两边各加一行 `mod`，合并时有一处小冲突。
   - **T026**：
     - 每个 `turn.completed` 之后，在同一个事务里先存 `turn_ops(&turn_id, &OpLimits::V1)` 作为回合快照，再调用 `trim(&Window::V1)`；
     - 检查点可以直接存视图的 JSON；
     - `history/page` 用 `paginate(回合 ops, max_bytes)`，`op_index` 就是页的起点。
   - **T033**：订阅时调用 `subscribe_mode(resume, &LogExtent)`；快照分页用 `paginate(snapshot_ops(), 帧负载减去信封的余量)`。
   - **T045**：
     - 实现客户端状态机：遇到 `Snapshot`（任何原因，包括未知的 mode）就丢弃视图和游标，核对 `at_seq`、`epoch` 和页序号后组装，之后的事件经 `StreamCursor` 再 `apply`；
     - 遇到 `ViewError` 时不带游标重新订阅；
     - `stream/reset` 保留游标。
   - **T015**：把 `IndexView`、`SessionView` 导出进 `v1.json`。
   - **T072**：照上面的规则和视图 JSON 实现 TS 的 reducer。JSON 对象的键顺序没有含义，比较时按深度相等。
