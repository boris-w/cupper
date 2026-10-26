# T117 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`test/T117-host-perf`（T037 的 worktree，从 main ea80560 开出）
- 完成日期：2026-10-26

三个 PERF 用例和夜间 `ci perf` 作业都做完了，第一次正式测量的数字已写进历史。两项顺带评估的结论：`flood_mixed` 慢在宿主、但够用；指纹遍历不超预算。带记录的 `ci pr` 一次通过。

## 做了什么

1. **三个 PERF 用例**：新测试文件 `tests/perf.rs`，用例都标了 `#[ignore]`，只在夜间跑。原型 SP-4a 的 `measure.rs` 只作参考，没有依赖。
   - PERF-host-cold-start：从拉起宿主进程，到客户端拿到完整的索引快照，测 30 次。为了不让计时受轮询间隔影响，连接每 1 ms 重试一次。
   - PERF-index-snapshot：宿主运行中，从连接、订阅索引到快照最后一页，测 30 次。
   - PERF-host-memory：6 个会话同时跑带输出的回合；回合进行中和结束后每 100 ms 采一次宿主 RSS，至少 30 个样本。
   - 前两个用例的数据：先建项目，再用 `test/seed` 灌 200 个会话，其中一个带 1,000 条的回合。
   - 按设计"绝对预算只在参考硬件上判定"，用例不断言预算，只交出样本。
2. **样本怎么交出来**：宿主库里加了 `perf::record`，只在测试和 `test-harness` 构建里有。它把样本写进环境变量 `CUPPER_PERF_OUT` 指向的 JSONL 文件。不往 stdout 打印，因为 `--nocapture` 会打乱 libtest 的输出。T186 的 PERF-snapshot 也改用它。
3. **`xtask ci perf` 作业**：
   - 持机器锁，在 nightly 组合里；
   - 用 release 构建单线程跑三个用例，再跑 PERF-snapshot；
   - 算出每个指标的 n、p50、p95、max（样本少于 20 个时 p95 就是最大值）；
   - 追加到 `testdata/perf/history.jsonl`，每行写日期、提交、版本、机型泛称、指标、单位、n、p50、p95、max，不写机器名和路径；
   - 某项 p50 超过同一指标、同一机型历史 p50 中位数的 1.5 倍，就判为退化，作业失败；没有历史的指标不判。
   - 单元测试两条：
     - 统计值和历史行的字段（字段名逐个核对，没有机器名），以及日期换算；
     - 退化判定（奇数、偶数个历史值；别的机型、别的指标不计入）。
   - `ci/mod.rs` 原来拿 `perf` 当"未实现的作业"做测试，改为 `compat-framework`。
4. **第一次 `cargo xtask ci perf`**：作业先等到了别的运行放下的机器锁，再跑完，6 项通过，结果写进了历史（提交 29e08c07dcf4）。

| 指标 | n | p50 | p95 | max | 设计预算 |
|---|---|---|---|---|---|
| 宿主冷启动 | 30 | 20.08 ms | 26.39 ms | 28.09 ms | p95 ≤ 400 ms |
| 索引快照（200 个会话） | 30 | 2.5 ms | 2.93 ms | 3.32 ms | ≤ 50 ms |
| 宿主内存（6 个活跃会话） | 30 | 30.8 MiB | 43.14 MiB | 43.14 MiB | ≤ 64 MiB |
| 快照，1 万文件、无改动 | 5 | 139.5 ms | 162.3 ms | 162.3 ms | 9.6 万文件 ≤ 0.5 s |
| 快照，1 万文件、改 10 个 | 5 | 265.55 ms | 428.32 ms | 428.32 ms | — |

冷启动没有清系统的文件缓存，清缓存要管理员权限。

## 顺带评估的结论

两项都是一次性测量：release 构建，持机器锁，测量代码没有提交。

### `flood_mixed` 的速度（T062 的交接）：慢在宿主，但达标

| 情况 | 时间 | 会话流事件数 |
|---|---|---|
| 只有假 agent，`flood_mixed 2000 10` | 53 ms | — |
| 经宿主，`flood_mixed 2000 10` | 5,202 ms | 6,011 |
| 经宿主，`flood 2000 10` | 93 ms | 14 |

- **原因**：每个交替条目都产生 `item.started`、`item.delta`、`item.completed` 三个事件。写入时合并只能合并同一条目的连续 delta，所以这些事件逐个提交，每个条目约 2.6 ms（每个事件约 0.87 ms）。
- **为什么算达标**：设计 5.9.6 没有"条目吞吐"的预算。真实 agent 每秒出个位数到几十个条目，离每秒约 380 个还远。
- **改进方向**：把会话 actor 同一步里不能合并的事件攒成一批再追加。这里只作建议，没有做。

### 回合指纹的遍历代价（T186 的交接）：不超预算

测量用的合成工作区共 10 万个文件：1 万个已跟踪，9 万个在被忽略的 `node_modules/` 里。

| 做法 | 时间 |
|---|---|
| 现在的 `Fingerprint::take`（遍历整个工作区） | 中位数 42.9 ms |
| 对照：`git ls-files -co --exclude-standard` 只找 `.gitattributes` | 19.1 ms |

- **结论**：每个回合取两次指纹，合计约 86 ms，在 0.5 s 的快照预算里占不到两成。暂不改成"只看已跟踪和未忽略的"。
- **不改的理由**：每次只能省约 25 ms，却要多拉起一次 git，而且结果会受仓库配置影响。
- **何时再改**：遍历时间随被忽略的文件数线性增长，到百万级时约 0.4 s，届时再改。

## 关键取舍

- **样本放文件，不靠解析 stdout**：既不打乱 libtest 的用例记录，PERF-snapshot 也能用同一个出口。
- **退化只比 p50，只比同一机型**：p50 最稳；不同机型的绝对数字不可比。
- **历史文件留在仓库里**：夜间运行之后要连同历史一起提交，趋势才能累积。
- **PERF-bridge-gate 建议也进历史**：做法写在 `notes.md`。e2e 夜间跑 `gate.spec.ts` 时也设 `CUPPER_PERF_OUT`，写一行事件循环延迟 p99；`perf` 作业里统计、追加、判定的代码抽成共用函数给 e2e 作业调用。绝对闸门 p99 < 5 ms 仍由用例自己断言。要改 TS 用例和 e2e 作业，建议并进 T078 的后续或单开一个小任务。

## 验证方式

| 验收标准 | 结果 |
|---|---|
| 三个 PERF 编号在 `ci perf` 里跑，数字写进 `history.jsonl`，退化判定有单元测试 | 已做，见上表；退化判定的单元测试是 `unit_ci_perf_regression_is_over_one_and_a_half_times_the_median` |
| `cargo xtask ci perf` 能跑通，并持机器锁 | 能跑通，共 782 s（含等锁）；日志里有"waiting for the machine lock held by another run" |
| `flood_mixed` 与指纹遍历的评估结论 | 见上，数字也写在 `notes.md` |
| 测试名以用例编号开头 | `perf_host_cold_start_*`、`perf_index_snapshot_*`、`perf_host_memory_*`，对应 T117 段的三个编号 |
| 干净 worktree 里 `cargo xtask ci pr --record docs/tasks/T117-host-perf/ci/` 通过，摘要随任务提交 | 一次通过，摘要已提交 |

`ci pr` 各作业：

| 作业 | 结果 |
|---|---|
| rust | 2152 项通过 |
| protocol | 185 项通过 |
| ts | 477 项通过 |
| license | 5 项通过 |
| testdata-scan | 3 项通过 |
| ui | 通过 |
| e2e | 30 项通过 |

另外：
- `perf` 是 nightly 作业，不在 `ci pr` 里；
- `cupper-host` 的 debug 构建里，三个 PERF 用例也跑通过一次；
- 摘要和历史里都没有本机路径。

`cargo xtask size`：+490 −3，净 +487。停止线 850，硬停止线 950。

## 提交列表

```
b3fbfcb docs(task): T117 CI summary
5965321 docs(task): T117 first perf run, the flood_mixed and fingerprint findings
5aabf32 test(perf): the first history, from a perf run on the reference kind of machine
29e08c0 ci(xtask): the perf job, with its history and the regression rule
714ebef test(host): PERF-host-cold-start, PERF-index-snapshot and PERF-host-memory
8a95df9 docs(task): estimate T117 by module
```

## 遗留问题 / 范围外发现

1. **PERF-snapshot 样本不足**：每种情况仍是 T186 的 5 个样本，少于设计要求的 30 个，所以它的 p95 就是最大值。要补足就改 T186 的用例，每晚多花约 1 分钟。
2. **`flood_mixed` 慢在宿主**：会话 actor 把不能合并的事件逐个提交，是吞吐的上限。现在达标；以后要提速，就在同一步里攒批追加。
3. **PERF-bridge-gate 进历史**：建议的做法见上，本任务没有做。
4. **夜间运行之后要提交 `history.jsonl`**：否则历史不会累积。由谁、怎样提交，需要在夜间流程里定下来。
