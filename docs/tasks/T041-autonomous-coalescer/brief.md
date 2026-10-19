# T041 会话纯核心 II：自发回合、合并器与最终回答

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主编排）
- 分支：`feat/T041-autonomous-coalescer`
- 依赖：T048（已合并）。T043（会话 actor，关键路径）依赖它；本任务的松弛约为 2。

## 背景

回合由宿主推导（ADR-06）：
- 以 `session/prompt` 的请求和响应为界；
- 回合外的更新，开一个 `origin: autonomous` 的回合（K9）。

T048 已经做好 `TurnMachine`，自发回合在那里只留了进入点和退出点（`Input::AutonomousStarted`、`AutonomousEnded`）。

本任务做其余几个纯核心：
- 判定自发回合何时开始、何时结束、能否出队（`AutonomousJudge`，Q10）；
- 写入时合并文本片段（`Coalescer`，从 SP-4a 的 `normalize.rs` 迁移）；
- `group_id` 与 `final_item_ids` 的判定；
- 晚到输出与回合快照不可变的模型检查。

actor 的接线归 T043。

## 范围

**做**（计划 T041 一行；设计 5.2.6、5.3.5、5.2.15、5.2.16）：
1. **`AutonomousJudge`**（5.2.6 自发回合的结束与出队，Q10）：
   - Codex 以 `threadStatus` 变为 idle 为准；
   - Claude 收到带 `cost` 的 `usage_update`，或者 10 s 没有更新且没有运行中的工具，就结束自发回合；
   - 由静默判定结束的自发回合，结束之后的出队保护按 5.2.6 处理（PROP-autonomous-dequeue-guard）；
   - 时长登记在 `durations.rs`，经 `Clock` 定时；
   - 回合外的交互请求一律放进自发回合，见 5.3.11 末尾"T014 实现时的裁定"。
2. **`Coalescer`**：
   - 写入时合并文本片段：40 ms 的窗口，其他事件到达时先刷出，定时器到点也刷出（5.2.16 的表）；
   - 从 SP-4a 的 `normalize.rs` 迁移，改成纯核心，输出用新的条目模型。
3. **`group_id` 与 `final_item_ids`**（5.3.5）：判定哪些条目属于同一组、哪些是回合的最终回答。
4. **晚到输出与回合快照不可变**：
   - 晚到输出：回合取消后命令仍在运行，产生 `late_tool_output` 条目，`ref` 指向原条目，信息自足；没有进行中的回合时，按 K9 开自发回合来承载；
   - 回合快照写下后不再改变；
   - 用模型检查验证。
5. **与 `TurnMachine` 的衔接**：
   - 用 T048 留下的 `AutonomousStarted`、`AutonomousEnded`；
   - Ending 期间到达的更新归属哪个回合，由本任务定下，在汇报里写明规则。

**不做**：
- actor 接线（T043）；
- 提取层（T053）。

## 验收标准

- [ ] PROP-autonomous-dequeue-guard（计划已登记）。
- [ ] PROP-turn-snapshot-immutable（计划已登记）。
- [ ] UNIT-final-items、UNIT-grouping（计划已登记）。
- [ ] 新增 PROP-coalescer：合并后应用等于逐条应用；其他事件到达时先刷出；定时器到点刷出；不跨条目合并。
- [ ] 全部用 `CoreDriver` 在虚拟时间上驱动，性质测试用 proptest，不睡眠。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T041-autonomous-coalescer/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md` 和设计裁定）

- **自发回合的进入与退出**（T048）：进入点和退出点是 `Input::AutonomousStarted`、`AutonomousEnded`；Ending 期间到达的更新归属哪个回合，由本任务处理。
- **回合外的交互请求**（5.3.11 末尾"T014 实现时的裁定"）：一律属于某个回合，没有进行中的回合时开自发回合；回合在它的请求全部解决之前不结束。
- **视图与合并**（T014、T011）：
  - `cupper-protocol` 的 `coalesce` 有合并规则，`Mergeable`；
  - PROP-view-coalesce 已经验证了视图层"合并后应用等于逐条应用"。本任务的 `Coalescer` 是宿主侧写入前的合并，两者要一致。
- **纯核心框架**（T022）：适配循环在 `pure/mod.rs` 的文档里；核心需要 ID 时用 `IdGen::new_id_at(now.wall_ms)`。
- **惯例**：
  - 用到 `CoreDriver` 的集成测试，声明 `required-features = ["test-harness"]`；
  - 改了协议类型就跑 `pnpm gen` 并提交生成物（T015）。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 并行注意

T042（交互代理）同时在 `orchestrator/` 里做 `InteractionBroker`。本任务的文件单独放，`orchestrator/mod.rs` 只加 `mod` 行。

## 规模

计划估为 M（400–900 行）。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**预计会超出约 1,500 行时，必须先停下来汇报，不要继续写**。

## 参考资料

- 设计：
  - `02-host.md` 5.2.6（回合推导、自发回合、晚到输出）、5.2.15、5.2.16（合并窗口和纯核心表）、迁移对照表中的 `normalize.rs`；
  - `03-protocol-v1.md` 5.3.5（`group_id`、`final_item_ids`、`late_tool_output`）、5.3.11；
  - `06-agent-driver.md` 5.6.9（K3、K9、K12）；
  - 设计 13 的 Q10。
- SP-4a 的 `normalize.rs`；T048 的 `orchestrator/turn.rs`；T022 的 `pure/`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-19，编排者）

执行者写到 751 行时推算，全部做完约 1,600–1,670 行，于是按守则第 8 条停工汇报。

**裁定：拆分（计划 v1.3.8）。** 本任务收窄为三部分：
- 与回合机的衔接（`HoldQueue`、`TurnSignal`）；
- `Coalescer`；
- 分组与最终回答。

验收是 PROP-coalescer、UNIT-grouping、UNIT-final-items。

`AutonomousJudge` 和组合测试环境另立为 T058，同一位执行者在本任务合并后接着做，验收是 PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable。T043 改为也依赖 T058。

执行者已定的设计决定全部认可，写进了 `03-protocol-v1.md` 5.3.11 末尾"T041 实现时的裁定"。出队保护的范围维持设计原意：只保护自动出队，用户的明确操作优先。

