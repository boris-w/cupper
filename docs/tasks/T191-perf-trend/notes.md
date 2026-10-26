# T191 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）
- 日期：2026-10-26

## 读到的现状（开工前）

- PERF-snapshot（`crates/cupper-host/src/git/snapshot/tests.rs`，`perf_snapshot_ten_thousand_files`）：两种情况各跑 5 次，经 `crate::perf::record` 写进 `CUPPER_PERF_OUT`，断言中位数（`runs[2]`）小于 2 s。历史里的指标名是 `snapshot_10k_no_change`、`snapshot_10k_10_files_changed`，单位 ms。上次正式运行时，两种情况的 p50 分别约 140 ms 和 266 ms。
- `ci/jobs/perf.rs` 的 `run`：先跑测试，然后把读样本、算 n/p50/p95/max、追加到 `testdata/perf/history.jsonl`、按同一机型 1.5 倍历史中位数判定这一整段写在一起。测试步骤失败时直接返回，不写历史。
- `gate.spec.ts`：两个判定用例（窗口在屏幕上、被遮住且停帧），各断言 p99 < 5 ms；另有一个六条流的压力报告，不判定。e2e 作业在 `ci pr` 里只跑 `@smoke`，`gate.spec.ts` 没有标签，只在 `ci e2e` 和 `ci nightly` 里跑。Playwright 的 `retries` 为 0。

## 做法

1. **PERF-snapshot**：每种情况 30 次，断言改成 30 次的中位数（`runs[15]`）。每晚多出的时间按上次的 p50 估算：25 × (140 + 266) ms ≈ 10 s，另加每次改文件的 shell 进程，约 10–15 s，比 T117 估的 1 分钟少。
2. **共用函数**（`ci/jobs/perf.rs`）：
   - `append(root, samples, run)`：读样本、追加历史、返回退化项，不碰机器；单元测试直接调它；
   - `trend(ctx, samples)`：取日期、提交、版本和机型泛称，调用 `append`，打印各指标、计数，有退化就失败。
   - `perf` 和 `e2e` 两个作业都调 `trend`。
3. **e2e 作业**：在 `ci pr`（只跑 `@smoke`）以外，两段 Playwright 都设 `CUPPER_PERF_OUT`；全部通过后才调 `trend`，测试失败不写历史，和 `perf` 作业一样。
4. **`gate.spec.ts`**：两个判定用例在断言都通过之后，按 `perf::record` 的格式追加一行，例如 `{"metric": "bridge_loop_delay_p99", "unit": "ms", "samples": [p99]}`。窗口被遮住的情况记作 `bridge_loop_delay_p99_covered`。压力报告不记，因为它不判定。每晚每种情况 1 个样本，趋势按夜累积。

## 开工前的规模估计

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| PERF-snapshot | 次数、断言、注释 | 4 | — |
| `perf.rs` 共用函数 | `append`、`trend`，`run` 改为调用 | 25 | 35 |
| e2e 作业 | 设 `CUPPER_PERF_OUT`，通过后调 `trend` | 12 | — |
| `gate.spec.ts` | 断言通过后追加一行样本 | 10 | — |
| **合计** | | **约 51** | **约 35** |

直接相加约 86，乘 1.2 约 105，在停止线 250 以内。
