# T047 test-harness 测试接口、录制导出与宿主性能

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主核心）。T037、T035、T096、T034 的执行者接手，因为宿主的组装、`Part`、静止屏障的登记大多是它写的
- 分支：`feat/T047-test-harness`
- 依赖：T034、T039、T043、T055（T034 合并中，其余已合并）。**本任务在剩余关键路径上，没有松弛**：T047 → T064 → T066 → T099

## 背景

进程级的集成测试、e2e 和兼容套件都要靠 test-harness 构建的测试接口驱动宿主：推进虚拟时钟到全宿主静止、手动跑维护任务、灌入种子数据，以及把宿主自己驱动产生的原始帧导出成录制（设计 5.2.16、5.9.9、5.9.3）。性能测量也从这里起步（5.9.6）。

## 范围

**做**（计划 T047 一行；设计 5.2.16、5.9.9、5.9.3、5.9.6）：
1. **`test/clock_advance`**：用 T022 的登记实现全宿主静止屏障。actor 在 poll 自己的等待点之前先标记为忙，只有等待的 future 返回 Pending 时才算空闲；统计在途的消息和已触发、尚未被消费的定时器；逐个到期时间推进（`ManualClock::next_deadline` 加 `advance_to`）（交接，T022）。要算进屏障的参与者：存储写线程（阻塞在 `inbox.recv()` 且手里没有批次才算空闲，T024）、原始帧写线程和 Maintenance（T027）、`spawn_blocking`、网关的连接任务（关闭宽限和握手期限的定时器，T030、T039）、账本清理任务和各连接等待回复的 task（T033）、索引（T034 已登记 `INDEX`）、会话 actor、`AgentRuntime`。`Quiescence` 和 `ManualClock` 留在 `Host` 上（交接，T037）。
2. **`test/run_maintenance`**：`raw_frames`、`audit`、`orphan_blobs` 对应 `Maintenance::run(Cleanup::…)`，`ledger` 对应 `Ledger::sweep()`；给 `Routes` 加一个公开的账本访问函数（交接，T027、T033）。参数形状按 T013（`run_maintenance {task}`），实现时可以细化。
3. **`test/seed`**：`seed {profile}`，含 SP-1 分布和单回合 2,000 条目（交接，T013）。
4. **`host/status.pid`**（test-harness 构建）。
5. **`diag/recording_export`**：从 `raw.db` 导出录制格式 v1，逐行写出用 `Header::to_line`、`Frame::to_line`（`cupper-protocol::recording`），导出前过 `tools/sanitize`（交接，T052）；`raw.db` 的 `ts` 换算成段内相对时间；`stderr` 和 `note` 方向由本任务补上（交接，T050）。
6. **其余注入点接线**（5.9.9）。
7. **迁移 SP-4a 的 `measure.rs`**（原 T067），性能趋势的历史数值序列入库（脱敏后只含数值和版本），"历史中位数"从它算出。

**不做**：通知的记录模式（T150）。

## 验收标准

- [ ] 新增 IT-clock-advance（含"全部 actor 类型都已登记"的枚举用例：新增一种 actor 却没登记时用例失败）、IT-seed、IT-recording-export（真实宿主进程加假 agent 产生原始帧，导出后能被格式库读回、过脱敏扫描）。
- [ ] PERF-host-cold-start、PERF-index-snapshot、PERF-host-memory（`nightly` 套件），历史数值序列入库。如果 PERF-index-snapshot 需要的会话摘要要等 T100 的状态计算，先停下汇报（计划 v1.3.19 的裁定）。
- [ ] 不睡眠：静止屏障本身就是为此服务的。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T047 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T047-test-harness/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T047 的行，开工前逐行读一遍）

T013、T022、T024、T027、T030、T033、T037、T039、T050、T052，以及 T034 的（`INDEX` 已登记、`IndexHandle` 是第三个 `Part`）。

## 注意

- `test/*` 方法只在 test-harness 构建里登记，发布构建永不开启（`--build-info` 与 UNIT-release-features 会核对）。
- 性能数值入库前去掉机器名、路径等，只留数值和版本。
- 不运行真实的 Claude Code CLI；用例用假 agent。

## 规模

计划估为 L，上限约 1,500 行。**范围宽，开工前先在 `notes.md` 里按模块写行数估计（测试单独列），推算超线（包括贴线）就开工前停下汇报，并给出拆分建议**，例如：
- 第一部分：静止屏障与 `test/clock_advance`、`test/run_maintenance`、`test/seed`、`host/status.pid`、注入点（T064 依赖它）；
- 第二部分：`diag/recording_export`；
- 第三部分：性能测量的迁移与趋势序列。

**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

## 参考资料

- 设计：`02-host.md` 5.2.16；`09-testing.md` 5.9.3、5.9.6、5.9.9；`03-protocol-v1.md`（`test/*`、`diag/recording_export`）。
- `docs/tasks/` 下 T022、T024、T027、T033、T034、T037、T052 的 `notes.md`；`spikes/SP-4a-host-skeleton/`（`measure.rs`）。

## 工作方式

T034 合并之后，在原来的 T037 worktree 里从最新的 main 开新分支：`git checkout -b feat/T047-test-harness main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
