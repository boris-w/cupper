# T191 汇报

- 执行者：子 agent（Rust 工程师，宿主安全）
- 分支：`test/T191-perf-trend`
- 完成日期：2026-10-26

**结论**：PERF-snapshot 每种情况补到了 30 个样本，正式的 `ci perf` 已跑通，它写进历史的 5 行已提交。PERF-bridge-gate 写历史的整条链路已接好，但**历史里还没有 PERF-bridge-gate 的行**：要跑一次完整的 `ci e2e` 或 `ci nightly` 才会写入，这次没跑（见"需要你定的事"）。改动 **+84 −26**，停止线是 250。记录 CI 第三次才通过，前两次失败的都是本任务没碰过的用例，原因是机器负载太高。工作区干净。

### 需要你定的事

- **要不要现在跑一次完整的 `ci e2e`**，让历史里先有第一行 PERF-bridge-gate。
  - 不跑的话，第一行等下次夜间运行写入。
  - 跑的话，整个 e2e 套件都会开窗口、占用桌面。我是按"只跑验证必需的次数、一次一个用例"才没有跑。你同意的话我在这个 worktree 里跑一次，并只提交那次的历史行。

### 做了什么

- **PERF-snapshot**（`crates/cupper-host/src/git/snapshot/tests.rs`）：两种情况各跑 30 次，断言改为 30 次的中位数。
- **共用函数**（`crates/xtask/src/ci/jobs/perf.rs`）：
  - `trend(ctx, samples)`：取日期、提交、版本和机型泛称，打印各指标，有退化就失败；
  - `append(root, samples, run)`：读样本，把本次运行的行追加到历史，返回退化项；它不读机器信息，所以单元测试可以直接调用。
  - `perf` 作业改成调用 `trend`，判定规则没变。
- **e2e 作业**（`crates/xtask/src/ci/jobs/e2e.rs`）：
  - `ci pr` 只跑 `@smoke`，这时不设 `CUPPER_PERF_OUT`；其余情况（`ci e2e`、`ci nightly`）给两段 Playwright 都设上；
  - 全部用例通过后才调用 `trend`；测试失败时不写历史，和 `perf` 作业一致。
- **`gate.spec.ts`**：两个判定用例在断言都通过之后，按 `perf::record` 的行格式追加一行样本。
  - 指标名：窗口在屏幕上时为 `bridge_loop_delay_p99`，被遮住时为 `bridge_loop_delay_p99_covered`；单位 ms，与历史里已有的写法一致。
  - 绝对闸门 p99 < 5 ms 仍由用例自己断言。
  - 六条流的压力报告不判定，所以不写历史。

### 关键取舍

- **每个判定用例每晚只有 1 个样本**，趋势靠一夜一夜累积。新指标没有历史时不判定退化。
- **样本在断言通过之后才写**：失败的那次测量不会进入趋势。Playwright 的 `retries` 为 0，同一晚不会写出重复的行。

### 验证方式

- **每个模块写完都跑了 `cargo xtask size`**：PERF-snapshot +4 → 共用函数及用例 +61 → e2e 作业 +75 → `gate.spec.ts` +84。
- **共用函数的单元测试**（`unit_ci_perf_trend_appends_the_run_and_judges_it`）：
  - 第一次追加时没有历史，不判定，历史里正好多一行；
  - 第二次的样本超过 1.5 倍，判为退化，历史里有两行；
  - 没有样本文件时报错。
  - 原有的 perf 单元测试照常通过。
- **正式的 `ci perf`**（提交 c0a7eda）：通过，5 个指标都没有退化。历史多出的 5 行字段齐全，只有机型泛称，没有机器名、序列号或路径。
  - PERF-snapshot 两种情况 n = 30：no change p50 83 ms、p95 101 ms；10 files changed p50 94 ms、p95 99 ms。
  - `perf-snapshot` 这一步共 34.0 s，其中 release 编译 22.0 s，用例本身 11.4 s（含造 1 万个文件的仓库）。
  - **每晚多花的时间**：比 5 个样本时多约 25 × (83 + 94) ms ≈ 4.4 s，加上每次改文件的 shell 进程，约 5–7 s，比 T117 估的 1 分钟少。
- **`gate.spec.ts` 写样本**：经 `test:e2e:one` 持锁跑了一次"窗口在屏幕上"的用例，设了 `CUPPER_PERF_OUT`。
  - 结果：通过，p99 2.76 ms，写出一行 `{"metric":"bridge_loop_delay_p99","unit":"ms","samples":[2.76]}`。
  - 被遮住的用例只差指标名后缀，为了少占桌面没有再跑。
  - 这次的样本写在临时文件里，没有进历史。
  - 跑完没有残留进程。
- **记录 CI**（`cargo xtask ci pr --record docs/tasks/T191-perf-trend/ci/`）：第三次全部通过，摘要已提交，运行期间没有写工作区。

  | 作业 | 结果 |
  |---|---|
  | rust | 2172 通过 |
  | protocol | 188 通过 |
  | ts | 483 通过 |
  | license | 通过 |
  | testdata-scan | 4 通过 |
  | e2e（冒烟） | 30 通过 |

  - compat-framework 和 ui 按改动路径跳过。
  - 前两次失败的摘要没有提交。

### 提交列表

- `1eb6949` docs(task): T191 estimate and the planned approach
- `29d31d1` test(host): PERF-snapshot takes 30 samples of each case
- `27ce815` refactor(xtask): the perf job's trend becomes a shared function
- `c0a7eda` test(e2e): PERF-bridge-gate's p99 joins the PERF history
- `063c0a5` test(perf): the history of a ci perf run with 30 snapshot samples
- `bdcdd1f` docs(task): T191 results of the perf run and the gate sample
- `d298329` docs(task): T191 recorded PR CI summary

### 遗留问题·范围外发现

- **记录 CI 前两次失败的用例都与本任务无关**，失败时另一个 worktree（T037）的宿主测试占满了 CPU，负载约 45。我等负载降到 12 以下后重跑，第三次通过。
  - **第一次**：`it_cli_commands_read_elevate_write_and_answer`（`crates/cupper-host/tests/lifecycle.rs:985`）。
    - 现象：第三条 `send` 得到的是 `queued`，断言期望 `started`。
    - 原因：用例在上一个回合结束前就发了下一条。
    - 单独跑 5 次都通过，耗时从 3.5 s 到 13.6 s 不等，说明它对负载敏感。建议用例先等上一个回合结束再发。
  - **第二次**：fake-agent 的三个 `unit_replay_*` 用例（"no response 1"、"the agent did not exit"），以及 `unit_trust_scan_forking_directory_links_hit_the_entry_limit`（耗时 17.6 s，超过它的时间上限）。都是超时类失败，负载下容易复现。
- **PERF-bridge-gate 的测量环境**：在 `ci e2e` 里，`gate.spec.ts` 和另一个 worker 的用例并行跑（`workers: 2`），测得的 p99 会受旁边用例影响，趋势会比单独跑时噪。测法属于 T078 的闸门，本任务没改，留给编排者判断。
