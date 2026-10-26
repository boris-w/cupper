# T117 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前读到的现状

- SP-4a 的 `measure.rs` 有两条：冷启动（spawn 到索引快照，20 个会话、30 次）和内存（空闲、6 个会话）。原型只作参考，不依赖。
- `xtask ci perf` 已登记为持机器锁的作业，`run: None`，在 `nightly` 组合里。
- PERF-snapshot（T186）是库里的 `#[ignore]` 用例，只往 stderr 打印中位数（每种情况 5 个样本）。
- 设计 5.9.6 的口径：
  - 每项至少 30 个样本，报 p50、p95、最大值；
  - CI 机器只看趋势，阈值是历史中位数的 1.5 倍；
  - 绝对预算只在参考硬件上判定：宿主冷启动 p95 ≤ 400 ms，索引快照（200 个会话）≤ 50 ms，宿主内存（6 个活跃会话）≤ 64 MiB。
- CI 摘要已有一个只写机型泛称的 `facts::machine`（例如 "MacBook Pro (Apple M5 Pro)"），历史数据沿用它。

## 方案

- **测量值怎么交出来**：PERF 用例不往 stdout 打印给人看的行（`--nocapture` 会打乱 libtest 的输出），而是经宿主库里一个只在 `test-harness` 构建中存在的 `perf::record`，把原始样本写到环境变量 `CUPPER_PERF_OUT` 指向的 JSONL 文件。PERF-snapshot 改为同样调用它。
- **`ci perf` 作业**：
  - 用 release 构建跑 `tests/perf.rs` 的三个 `#[ignore]` 用例，单线程；再跑 PERF-snapshot；
  - 读样本，算 n、p50、p95、max；
  - 与 `testdata/perf/history.jsonl` 里同一指标、同一机型的历史 p50 中位数比较，超过 1.5 倍判为退化；
  - 每次运行追加一行到历史，字段为日期、提交、版本、机型泛称、指标、单位、n、p50、p95、max。
- **三个用例**：
  - PERF-host-cold-start：经 `test/seed` 灌 200 个会话（其中一个有 1,000 条的回合），30 次"拉起宿主到拿到完整的索引快照"；
  - PERF-index-snapshot：同一份数据，宿主运行中 30 次"连接、订阅索引到最后一页"；
  - PERF-host-memory：6 个会话同时跑带输出的回合，期间和之后按 100 ms 采样宿主 RSS，至少 30 个样本。
- **顺带评估**：`flood_mixed` 分别测"只有假 agent"和"经宿主"两种情况；指纹遍历用一个合成的 10 万文件工作区计时。都是一次性测量，结论写进汇报。

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行，按净增计；以往偏低，乘 1.3。

| 模块 | 估计 |
|---|---|
| 宿主库的 `perf::record`（`test-harness`） | 25 |
| PERF-snapshot 改用它 | 5 |
| `tests/perf.rs`：三个用例与辅助 | 160 |
| `xtask` 的 `perf` 作业：运行、读样本、统计、历史、判定 | 150 |
| `xtask` 单元测试：统计、退化判定、历史行不含机器名 | 70 |
| `Cargo.toml` 测试目标 | 5 |
| **合计** | **约 415，乘 1.3 约 540** |

在停止线 850 之内，开工。每次提交后跑 `cargo xtask size`。
