# T191 性能趋势补全

- 派发日期：2026-10-26
- 执行者：子 agent（Rust 工程师，宿主安全）。T186、T188 的执行者接着做（PERF-snapshot 是它写的）
- 分支：`test/T191-perf-trend`
- 依赖：T117（已合并）

## 背景

T117 做了夜间的 `ci perf` 作业和历史文件 `testdata/perf/history.jsonl`，退化判定按 p50 对比同一机型历史中位数的 1.5 倍（计划 1.8）。还差两处（计划 v1.3.59）：
- PERF-snapshot 每种情况只有 5 个样本，少于设计 5.9.6 要求的 30 个，p95 只能取最大值；
- PERF-bridge-gate（T078）是 e2e 用例，它的数字还没有进历史。

## 范围

**做**：
1. **PERF-snapshot 补到每种情况 30 个样本**：
   - 改 `git/snapshot/tests.rs` 里的 PERF 用例；
   - 每晚多花的时间写进汇报，T117 估计约 1 分钟；如果多出很多，说明原因并给出取舍。
2. **PERF-bridge-gate 进历史**，做法见 `docs/tasks/T117-host-perf/notes.md`：
   - e2e 夜间跑 `gate.spec.ts` 时设 `CUPPER_PERF_OUT`，用例按 `perf::record` 的行格式写一行事件循环延迟 p99 的样本；
   - `ci/jobs/perf.rs` 里统计、追加、判定的代码抽成共用函数，给 e2e 作业调用；
   - 绝对闸门 p99 < 5 ms 仍由用例自己断言；
   - 指标名、单位与历史里已有的行保持同一种写法。
3. 退化判定照旧只比同一机型；新指标没有历史时不判。

**不做**：改退化规则；改 T078 的闸门本身。

## 验收标准

- [ ] PERF-snapshot 每种情况 30 个样本，`ci perf` 跑通，历史多出的行字段齐全。
- [ ] `ci e2e`（或 `ci nightly` 里的 e2e）跑 `gate.spec.ts` 后，历史里有 PERF-bridge-gate 的行；共用函数有单元测试。
- [ ] 历史里没有机器名、序列号和路径，只有机型泛称。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T191-perf-trend/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T191 的行）

- T117：`perf::record` 与 `CUPPER_PERF_OUT`；PERF-bridge-gate 进历史的做法写在它的 `notes.md`。
- T078：`gate.spec.ts` 的 `throttled` 测量与闸门。
- T093、T178：e2e 一律经 `pnpm test:e2e`、`test:e2e:one` 持锁运行；屏幕没锁、显示器醒着。

## 注意

- 测量期间不要和别的重任务撞车，测量都经机器锁。
- `gate.spec.ts` 要开窗口，会占用作者的桌面。只跑验证必需的次数，一次一个用例（`test:e2e:one`）。
- 历史文件只在 `ci perf`、`ci nightly` 正式运行时追加；你调试时产生的行不要提交，只提交一次正式运行的结果。

## 规模

- 停止线：250；硬停止线：300

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计；每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T054 worktree 里从最新的 main 开新分支：`git checkout -b test/T191-perf-trend main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
