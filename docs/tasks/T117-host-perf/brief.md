# T117 宿主性能测量与趋势

- 派发日期：2026-10-26
- 执行者：子 agent（Rust 工程师，宿主核心）。T037、T047、T100、T138、T163、T068、T062、T065、T069 的执行者接着做（测试接口 T047 是它做的）
- 分支：`test/T117-host-perf`
- 依赖：T047（已合并）。T099 依赖本任务

## 背景

性能要有趋势数据：每晚跑，和历史比较，退化时报警（设计 5.9.6）。SP-4a 原型有一个 `measure.rs`，计划里由本任务迁移（原 T067 的一部分）。`xtask ci perf` 作业现在是 `run: None`。T186 已经写了一个每晚运行的 PERF-snapshot，T078 有每晚的 PERF-bridge-gate，都还没有接进 `perf` 作业。

## 范围

**做**（计划 T117 一行；设计 5.9.6）：
1. **三个 PERF 用例**，迁移 SP-4a 的 `measure.rs`，只参考、不依赖原型代码：
   - PERF-host-cold-start：宿主冷启动；
   - PERF-index-snapshot：索引快照（用 `test/seed` 灌数据，T047 的交接：先要有一个项目）；
   - PERF-host-memory：宿主内存。
2. **`xtask ci perf` 作业**（nightly）：
   - 跑上面三个用例，加上已有的 PERF-snapshot（`cargo test -p cupper-host --lib -- --ignored perf_snapshot`，T186 的交接）；
   - PERF-bridge-gate 是 e2e，要开窗口，放在 `ci e2e`/`ci nightly` 里跑。要不要也把它的数字写进历史，在汇报里给出建议；
   - 作业持机器锁。
3. **历史数值序列** `testdata/perf/history.jsonl`：
   - 每行包括日期、版本、指标、p50/p95/max、样本数；
   - **不含机器名和路径**，只写机型泛称；
   - 按历史中位数的 1.5 倍判定退化。
4. **顺带评估**（交接里点名给本任务）：
   - 假 agent 的 `flood_mixed` 每个条目约 3.5 ms、线性增长：是否达标，若不达标，原因在假 agent 还是宿主（T062 的交接）；
   - 回合指纹每次快照遍历整个工作区找 `.gitattributes`，10 万文件的仓库是否超预算（T186 的交接）。

**不做**：界面侧的性能（SP-6、M1b）；e2e 的测量闸门（T078 已做）。

## 验收标准

- [ ] 三个 PERF 编号在 `ci perf` 里跑，数字写进 `history.jsonl`，退化判定有单元测试。
- [ ] `cargo xtask ci perf` 能跑通，并持机器锁。
- [ ] `flood_mixed` 与指纹遍历的评估结论写进汇报。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T117 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T117-host-perf/ci/` 通过，摘要随任务提交（`perf` 是 nightly，不在 `ci pr` 里）。

## 交接事项（`handoff-notes.md` 第一列含 T117 的行）

- T047：`test/seed` 要先有一个项目；seed 返回时索引快照里已经有这些会话。
- T062：`flood_mixed` 的速度。
- T186：PERF-snapshot 怎么跑；指纹遍历的代价。
- T078：PERF-bridge-gate 在 e2e 里。

## 注意

- 测量期间别和别的重任务撞车（经机器锁）；数字只写机型泛称，不写机器名、序列号和路径。
- 不运行真实的 Claude Code、Codex CLI。

## 规模

- 停止线：850；硬停止线：950

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计；每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

T069 合并之后，在原来的 T037 worktree 里从最新的 main 开新分支：`git checkout -b test/T117-host-perf main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
