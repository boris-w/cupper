# T014 协议视图与同步规则

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T014-protocol-views`
- 依赖：T017（已合并）。属于 M1a，在关键路径上：T017 → **T014** → T026 → T033 → …

## 背景

客户端和宿主共用一套纯函数视图：快照就是一串事件，增量和快照都用同一个 `apply` 处理（设计 5.3.3、5.7.6）。Rust 视图由宿主生成快照时使用，也是 TS reducer 的对照基准。TS 和 Rust 两边用同一批金样例做交叉验证（5.9）。本任务在 `cupper-protocol` 里实现这些纯视图和同步规则。视图不做 I/O，要能编译到 iOS。

## 范围

**做**（计划 T014 一行）：
- `IndexView`：项目、会话摘要、agent 状态、客户端、提权，等等。
- `SessionView`，包括：
  - 窗口：最近 20 个回合或 2,000 个条目，见 5.2.8；
  - 进行中的回合；
  - 待处理的交互；
  - 配置、能力、队列。
- `apply`：按 `turn_no` 排序；`ItemPatch` 的语义是出现的字段整体替换，没出现的保持不变（5.3.11）。
- `late_by_ref`：反向索引，键是 `ref.item_id`。
- 快照 ops 的生成与切分：长文本按 `MAX_DELTA_ESCAPED` 拆成 `item.delta`；进行中的回合也要分页；单个 op 不超过帧预算。
- 连续性检查（`from` 等于上一个 seq 加 1）与 epoch 重置规则：`epoch_changed`、`cursor_ahead` 时整体重置。
- 视图的序列化格式，要纳入 schema（测试 #16①：NFR-1 的用例要比较两端的视图 JSON）。

**不做**：
- 存储侧的检查点和回合快照（T026、T027）；
- TS reducer（T072 或 host-client）；
- 方法（T012、T013）。

## 验收标准

- [ ] 新增 PROP-view-coalesce：合并后应用的结果，与逐条应用完全相同（ADR-04 的性质测试要求）。
- [ ] 新增 UNIT-view-window：窗口淘汰；进行中的回合不被淘汰；向前插入历史时按 `turn_no` 排序正确。
- [ ] 新增 UNIT-view-late-by-ref：晚到的输出条目，能经反向索引关联到原条目；原回合不在窗口里时，晚到条目本身信息完整。
- [ ] 补充的性质测试（测试 #22）：窗口快照加上全部 `history/page`，等于从头重放；快照切分后再应用，等于不切分。如果规模放不下，在汇报里说明，交给 T026/T027。
- [ ] 仍能编译到 `aarch64-apple-ios-sim`；在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T014-protocol-views/ci/` 通过，摘要随任务一起提交。
- [ ] 测试名以用例编号的 snake_case 开头（`prop_view_coalesce_`、`unit_view_window_` 等）。

## 交接事项（来自 T017 和 T011，见 `docs/tasks/handoff-notes.md`）

- `apply` 的规则；`late_by_ref` 的键（线上是 `ref.item_id`，Rust 里是 `LateToolOutput.of.item_id`）；工具调用的用时来自 `item.completed.duration_ms`；快照中的长文本按 `MAX_DELTA_ESCAPED` 拆分。
- 惯例：`lib.rs` 只放 `mod` 声明；用 `ids`、`TimestampMs` 这类带注解的类型；不使用超过 2^53−1 的整数；性质测试用 proptest（工作区依赖已有）。
- 时间：视图是纯函数。需要"当前时间"时作为参数传入，不读时钟（`CLAUDE.md` 的 Clock 规则）。

## 规模

计划估为 M（400–900 行），实际可能更大。**开工前先在 `notes.md` 里按模块写行数估计，并定一条停止线**。动手之后，每完成一个模块统计一次行数（守则第 8 条）；预计会明显超出约 1,500 行的上限时，先停下来汇报。

## 参考资料

- 设计：`03-protocol-v1.md` 5.3.3、5.3.5、5.3.11；`02-host.md` 5.2.8；`07-desktop.md` 5.7.6；`09-testing.md` 5.9.1、5.9.4。
- T011、T017 的代码和汇报；SP-4a 的视图与 `Mirror`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。
