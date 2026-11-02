# T219 笔记

## 怎么表示"只记录"

- **摘要**：
  - 托管 runner 上（`RUNNER_ENVIRONMENT=github-hosted`，`facts::hosted()`），PERF 编号的用例全部通过时，结果记作 `measured`，不记 `pass`；没通过照旧记 `fail`。作者机器上照旧记 `pass`。
  - 摘要新增 `perf` 列表，每项就是一行历史（日期、提交、版本、机型泛称、指标、单位、n、p50、p95、最大值），和 `testdata/perf/history.jsonl` 的格式相同，可以直接拼成历史看趋势。作者机器上的运行也写这个列表。
  - 字段照旧走白名单（`summary.rs` 的 `SUMMARY`）。
- **`perf` 作业**：托管 runner 上测量照做，不判退化，也不写 `history.jsonl`。作者机器上不变：写历史，并按历史中位数的 1.5 倍判退化。
- **`e2e` 作业**：托管 runner 上不再排除 `@perf` 用例，给 Playwright 设 `CUPPER_PERF_RECORD_ONLY=1`。`gate.spec.ts` 见到它就不断言 p99 < 5 ms，只把 p99 写进 `CUPPER_PERF_OUT`。流重置、blob、重新同步这些断言照旧，它们判的是对不对，不是快不快。`@perf` 标签留着，作标签用。去掉它，prettier 会把整个用例重新缩进，多出约 20 行改动。
- **登记表**：新状态 `record`（只记录），只用于 PERF 编号，要写 reason。
  - `record` 编号在登记的套件里有 `measured` 或 `pass` 就算完成，显示 "measured in nightly"；有 `fail` 仍算问题。
  - 要按预算判定的编号（`active`），只有 `measured` 时算问题："measured, not judged"。恢复闸门时就不会被 runner 的结果冒充。
  - 名字原想用 `record-only`，但那样 `STATUSES` 一行超过 100 列，prettier 会把它拆成 8 行；为守停止线改成 `record`。
- PERF-snapshot 用例里的"中位数 < 2 s"是对 1 万文件的健全性上限，不是 5.9.6 的预算（9.6 万文件 ≤ 0.5 s），留着。

## verify 为什么在分支上过不了 `--milestone`

- `trace-ids verify --milestone` 只读同一个提交的入库摘要。
- T099 的摘要来自 ec459fd，本任务的 nightly 摘要来自分支提交，两者不能混用。
- 分支上凑不出同一个提交的一整套摘要：
  - PR 的 ci 跑在 GitHub 的合并提交上，`ui`、`compat-framework` 还会按路径跳过；
  - nightly 跑在分支提交上。
- 办法（编排者同意）：
  - 分支上：本次 nightly 的摘要放在 `ci/nightly/`，用 `verify --task` 和 `status` 核对；
  - 合并后：在 main 的合并提交上收齐 push 的 ci 和 nightly 共 15 份摘要，放在 `ci/main/`，再跑 `verify --milestone M1a`。
