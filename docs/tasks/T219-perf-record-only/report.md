# T219 汇报

- 执行者：子 agent（工程师：xtask、workflow、trace-ids）
- 分支：`ci/T219-perf-record-only`（PR #38：https://github.com/boris-w/cupper/pull/38）
- 完成日期：2026-11-02

## 做了什么

**1. nightly 在托管 runner 上跑 PERF，只记录（xtask、workflow、e2e）**
- `nightly.yml` 新增 `{ job: perf, runner: macos-15, timeout: 60 }`，文件头注释也改了。
- 新增 `facts::hosted()`，判断是否在托管 runner 上（`RUNNER_ENVIRONMENT=github-hosted`）。原来有 3 处重复写了这个判断，都改为调用它。
- **`perf` 作业**（`perf::trend`）：
  - 测量照做。托管 runner 上不判退化，也不写 `testdata/perf/history.jsonl`。
  - 作者机器上不变：写历史，p50 超过同机型历史中位数的 1.5 倍判为退化。
  - 不论在哪台机器上，各指标都写进摘要的新列表 `perf`。每项就是一行历史：日期、提交、版本、机型泛称、指标、单位、n、p50、p95、最大值，格式和 `history.jsonl` 相同，可以直接拼成历史看趋势。
- **`e2e` 作业**：
  - 托管 runner 上不再排除 `@perf` 用例，也不再跳过 `perf` 步骤。
  - 给 Playwright 设 `CUPPER_PERF_RECORD_ONLY=1`。`gate.spec.ts` 见到它就不断言 p99 < 5 ms，只把 p99 写进 `CUPPER_PERF_OUT`。
  - 流重置、blob、重新同步这几项断言照旧。
- **摘要**（`summary.rs`）：
  - 字段白名单加了 `perf`。
  - 托管 runner 上，PERF 编号的用例全部通过时，结果记作 **`measured`**，不记 `pass`；失败照旧记 `fail`。作者机器上照旧记 `pass`。

**2. 登记表与 trace-ids**
- 新状态 **`record`**（只记录）：
  - 只能用于 PERF 编号，要写 reason。
  - 在登记的套件里有 `measured` 或 `pass` 就算完成，显示 "measured in nightly"。
  - 有 `fail` 仍算问题。
  - 登记为 `active`、要按预算判定的编号，只有 `measured` 时算问题（"measured, not judged"）。以后恢复闸门时，runner 的结果不会被当成判定结果。
- `readSummary` 接受 `measured` 这个结果。
- 新增用例：
  - registry：`record` 只用于 PERF；
  - evaluate：measured、pass、fail、跑错套件、什么都没跑这几种情况；
  - results：读 `measured`。
- `tests/ids.toml`：
  - 9 个 PERF 编号都登记为 `status = "record"`，reason 为 "M1a 同步会 D1：只记录、不卡闸门"。包括 M1a 的 5 个，以及 M1b 的 PERF-ui-stream、PERF-ui-long-session、PERF-ui-switch、PERF-ui-occluded。
  - IT-sys-peer-current 改为 `status = "na"`，reason 为 "M1a 同步会 D2：用户的 Mac 是 macOS 26，由 -macos26 覆盖"。
  - 文件头加了 `record` 的说明。
- IT-sys-peer 由 -macos15、-macos26 完成，-macos14 和 -current 都是不适用。

**3. 卷宗**
- `docs/tasks/T219-perf-record-only/ci/nightly/` 下放了分支上那次 nightly 的 7 份摘要，目录名就是构件名。
- `notes.md` 记了"只记录"怎么表示，以及为什么 `--milestone` 要等合并后再跑。

## 关键取舍

- **用 `measured` 而不是 `pass`**：摘要里能看出 runner 上的 PERF 没有按预算判定。trace-ids 只对登记为 `record` 的编号把它算作完成。
- **由 xtask 设 `CUPPER_PERF_RECORD_ONLY`，测试本身不看 `RUNNER_ENVIRONMENT`**：要不要判定只在 xtask 里决定，和摘要标记 `measured` 用的是同一个 `facts::hosted()`。
- **状态名用 `record`，没用 `record-only`**：用 `record-only` 时 `STATUSES` 那一行超过 100 列，prettier 会拆成 8 行，超出停止线。
- **`@perf` 标签留着，作标签用**：xtask 已经不按它筛选了。去掉它，prettier 会把整个用例重新缩进，多出约 20 行改动。
- **PERF-snapshot 的"中位数 < 2 s"留着**：这是 1 万文件仓库的健全性上限，不是 5.9.6 的预算（9.6 万文件 ≤ 0.5 s）。runner 上实测 p50 109–144 ms。
- **`verify --milestone M1a` 要等合并后**：它只读同一个提交的入库摘要。
  - T099 的摘要是 ec459fd 的，这次 nightly 的是 b4c7968 的，两者不能混用。
  - PR 的 ci 跑在合并提交上，`ui`、`compat-framework` 还会按路径跳过，所以分支上凑不出一整套同一提交的摘要。
  - 你同意了这个办法：分支上用 `--task` 和 `status` 核对，合并后在 main 上收齐 15 份摘要再跑 `--milestone`。

## 验证方式

- **nightly 在托管 runner 上测出 PERF**：分支上的 nightly [41437271713](https://github.com/boris-w/cupper/actions/runs/41437271713)，提交 b4c7968，7 个作业全部通过。
  - `perf` 作业（[作业](https://github.com/boris-w/cupper/actions/runs/41437271713/job/125250122830)）约 10 分钟，6 个检查通过，4 个编号记作 measured。
  - runner 是 macOS 15.8.2，机型泛称 "Apple Virtual Machine 1 (Apple M1 (Virtual))"。各指标如下：

    | 指标 | 单位 | n | p50 | p95 | 最大值 |
    |---|---|---|---|---|---|
    | host_cold_start | ms | 30 | 21.79 | 61.22 | 68.71 |
    | index_snapshot | ms | 30 | 2.83 | 4.37 | 4.85 |
    | host_memory | MiB | 31 | 36.63 | 37.48 | 37.48 |
    | snapshot_10k_no_change | ms | 30 | 108.66 | 125.43 | 125.82 |
    | snapshot_10k_10_files_changed | ms | 30 | 143.86 | 178.2 | 234.82 |

  - e2e 全集（[作业](https://github.com/boris-w/cupper/actions/runs/41437271713/job/125250122967)）56 个通过，PERF-bridge-gate 记作 measured。p99 在窗口可见时是 7.97 ms，遮挡时是 6.21 ms，超过 5 ms 的预算，按 D1 只记录。T197 时 runner 上是 8.65 ms。
- **`trace-ids verify --milestone M1a`**：分支上按约定的办法核对。
  - 用新摘要跑 `verify --task`，T117、T186、T078、T021 每个都是 "0 of N IDs not done"。T186 的 pr 编号取自 T099 的 rust 摘要。
  - `status` 读 T099 的 14 份摘要、这次的 7 份和兼容报告。M1a 的 288 个编号里 284 个已完成、4 个不适用，没有待办，也没有问题。
    - 5 个 PERF 编号都是 "measured in nightly"；
    - IT-sys-peer 是 "all 4 sub-IDs"；
    - IT-sys-peer-current 是 "n/a: M1a 同步会 D2 …"。
  - 只用 T099 的摘要跑 `verify --milestone M1a`，结果是 "M1a: 5 of 288 IDs not done"，剩下的就是这 5 个 PERF，因为 ec459fd 上没有它们的测量。
  - 把 T099 的摘要和新摘要放在一起跑 `--milestone`，按设计被拒："--milestone reads committed summaries of a single commit"。
- **合并后要做的**（你说会叫我回来）：
  1. 合并提交 push 时的 ci 有 8 份摘要，main 上的 nightly 有 7 份，一共 15 份，下载到 `docs/tasks/T219-perf-record-only/ci/main/<构件名>/`，并且先提交。
  2. 再跑：
     ```sh
     node tools/trace-ids/src/main.ts verify --milestone M1a \
       $(git ls-files 'docs/tasks/T219-perf-record-only/ci/main/*.json' | sed 's/^/--summary /') \
       --compat testdata/compat/reports/claude-agent-acp-0.91.0-claude-2.1.296.json
     ```
  3. 通过后在这个提交上打 `gate/m1a`。
- **PR #38 的 8 个检查都通过**：
  - b4c7968：[41437274153](https://github.com/boris-w/cupper/actions/runs/41437274153)；
  - f35ff8e（最终提交）：[41440688381](https://github.com/boris-w/cupper/actions/runs/41440688381)。
- **本机只跑了普通检查**，没有跑 perf、nightly 或 e2e：
  - `cargo test -p xtask` 61 个加 1 个通过；
  - `cargo clippy -p xtask --all-targets -- -D warnings`、`cargo fmt --all --check`；
  - trace-ids 的 vitest 24 个通过；
  - `trace-ids check`：437 个编号，和文档一致；
  - eslint、`tsc --build tools/trace-ids tools/e2e`、prettier。
- **规模**：`cargo xtask size` 结果是 +149 −78，停止线 150，没有超过。

## 提交列表

```
f35ff8e docs(task): T219 nightly summaries of b4c7968 on GitHub, and notes
b4c7968 test(ids): PERF IDs only record; IT-sys-peer-current does not apply
0e3865a ci: nightly measures PERF on a GitHub-hosted runner, judging no budget
bb568f7 feat(trace-ids): PERF IDs that only record, and measured results
```

## 设计和计划要改的地方（由编排者改）

**计划 `docs/03-plan.md`**
1. **1.8 的"每晚作业"行**：把"`perf` 不上 CI（runner 不是参考机型），由编排者在本机 main 上跑 `xtask ci nightly --record docs/ci/nightly/`，历史与摘要一起提交。"改为：
   > `perf`（`macos-15`）也在 nightly 里跑，只记录：测量照做，各指标的 n、p50、p95、最大值以历史行的格式写进摘要的 `perf` 列表，不按预算判定、不判退化，也不写 `history.jsonl`。e2e 全集同样跑 PERF-bridge-gate，只记录 p99（`CUPPER_PERF_RECORD_ONLY`）。托管 runner 上的 PERF 编号在摘要里记作 `measured`（M1a 同步会 D1，T219）。
2. **1.8 的"性能趋势"行**：改为：
   > `testdata/perf/history.jsonl` 只由作者机器上的 `xtask ci perf` 或 `ci nightly` 追加，并在那里判退化（规则不变）。按 2026-11-02 的决定，目前不安排在作者机器上跑。runner 上的数值只在 nightly 摘要的 `perf` 列表里。PERF 不卡任何里程碑；有了自用构建、用户愿意时在真机上补测，届时再定是否恢复闸门。
3. **1.8 的"M1a 退出"行**：把"PERF 闸门在作者机器上判（T099、T197）"改为"PERF 不作为退出条件，只记录（M1a 同步会 D1，T219）"。
4. **1.8 的"本机替代不了的项"行**：补一句：
   > 作者真机上的对端校验（IT-sys-peer-current）自 2026-11-02 不适用：用户的 Mac 是 macOS 26，由 IT-sys-peer-macos26 覆盖（M1a 同步会 D2）。
5. **1.6 第 143 行**：把"IT-sys-peer = `-current`（T021，作者机器上的系统）+ …；`-macos14` 自 2026-11-02 不适用"改为：
   > …；`-macos14`、`-current` 自 2026-11-02 不适用（最低支持版本提到 15；用户的 Mac 是 macOS 26，由 `-macos26` 覆盖，D2）
6. **1.6 第 149 行**："本机跑的 `security-peer` 只记作 `IT-sys-peer-current`"后加"（该编号自 2026-11-02 不适用）"。
7. **1.6 新增一条**：
   > PERF 编号一律登记为 `status = "record"`（只记录）：在登记的套件里有 `measured` 或 `pass` 就算完成，不按预算判定；托管 runner 的摘要把 PERF 结果记作 `measured`；登记为 `active` 的编号只有 `measured` 时算问题（M1a 同步会 D1，T219）。
8. **5.7 CI 作业表**：
   - 加一行"`perf` | T117 | T191；T219 起每晚在 runner 上只记录"；
   - "每晚的其他检查（差分测试、性能趋势）"改为"…（差分测试、性能测量，只记录）"，扩充一栏加上 T219；
   - `security-peer` 行的"本机运行时只记作 IT-sys-peer-current"后加"（不适用，D2）"。
9. **5.4 的 SEC-8 行（第 1436 行）和 5.5 的 Q24 行（第 1499 行）**：在 IT-sys-peer-current 后注"（2026-11-02 起不适用，D2）"。
10. **T155 行（第 468 行）**："性能回归与趋势阈值"改为"性能测量（只记录，D1）"。**4.4 的 K8**（"5.9.6 与 5.9.7 的预算"）见遗留问题第 1 条。
11. **版本记录**：新增 v1.3.85，T219 合并：PERF 只记录、登记状态 `record`、摘要的 `measured` 和 `perf` 列表、IT-sys-peer-current 不适用。

**设计**
1. **09-testing 5.9.1 表的 PERF 行**：把"每晚看趋势；判定在参考硬件上做"改为：
   > 每晚在 runner 上测量，只记录（摘要的 `perf` 列表），不卡闸门（M1a 同步会 D1）
2. **09-testing 5.9.6"判定口径"第一条**：改为：
   > PERF 只记录、不卡闸门（M1a 同步会 D1）：每晚在 GitHub 托管 runner（`macos-15`）上测量，n、p50、p95、最大值写进摘要，编号记作 `measured`，不按预算判定，也不判退化。runner 是共享虚拟机，数值只用于看趋势。绝对预算留给参考硬件：有了自用构建、用户愿意时在真机上补测，届时再定是否恢复闸门。在作者机器上运行 `xtask ci perf` 时，仍按历史中位数的 1.5 倍判退化，写 `testdata/perf/history.jsonl`，报告里记下机型泛称。

   另外，在"主进程事件循环延迟"一行注明：runner 上实测 6–8 ms（T197、T219），只记录。
3. **09-testing 5.9.10**：把"支持子编号、MAN 证据和'不适用'状态"改为"…、'不适用'和'只记录'（PERF：有测量结果就算完成）状态"。
4. **01-repo-build-ci 5.1.6**：
   - 开头一段：把"`perf` 不上 CI，在作者机器上跑。"改为"`perf` 每晚在 `macos-15` 上跑，只记录、不卡闸门（M1a 同步会 D1，T219）。"
   - 作业表加一行：
     > `perf` | `macos-15` | 每晚 | release 构建下宿主的 PERF 用例（PERF-host-cold-start、PERF-index-snapshot、PERF-host-memory）和 PERF-snapshot，一次一个；托管 runner 上只把各指标写进摘要，编号记作 `measured`，不判退化、不写历史；作者机器上按历史中位数的 1.5 倍判退化并追加 `history.jsonl`（5.9.6）| 否（只记录）
   - `e2e` 行补一句："每晚的全集含 PERF-bridge-gate，托管 runner 上只记录 p99"。
   - mermaid 图的 NIGHT 下加 `perf（只记录）`。
5. **12-milestones**：
   - 5.12.4 的 NFR-6 行：把"PERF（5.9.6）〔晚〕"改为"PERF（5.9.6）〔晚，只记录〕"；
   - 5.12.2 的"工程"行，CI 作业列表里可以补上 `perf`（每晚，只记录）；
   - 5.12.2 的退出标准没写 PERF，不用改。
6. **07-desktop 5.7.1 的测量闸门**（可选）：在"结论"后补一句"K2 之后每晚在 runner 上只记录（runner 上 p99 6–8 ms，T197、T219）"。

## 遗留问题 / 范围外发现

1. **SP-6 安排在作者机器上**：T103（场景 1–4、7、8）、T108（8 小时浸泡）、K8（按 5.9.6 和 5.9.7 的预算判定）、设计 5.9.7 的"判定以作者的机器为准"、第 6 节用户参与表第 15、16 行，都要在作者机器上压测。这和 perf-local 会议的 D2"以后不安排在用户机器上跑 PERF 或本机 nightly"冲突。M1b B2 之前需要和用户再定。R8 的描述也随之要改。
2. **runner 上的趋势只存在构件里**：nightly 的摘要构件按仓库的保留期过期（日志 7 天，摘要用默认保留期），不入库。要长期看趋势的话，需要定一个存放办法，比如定期把摘要的 `perf` 行汇总入库。
3. **nightly 有 6 个 macOS 作业**：标准 runner 最多同时跑 5 个，其中一个要排队，墙钟时间略长。这次 7 个作业都在约 13 分钟内跑完。
4. **计划 5.7 的表头还写着"全部经 `cargo xtask ci <作业>` 在本机执行"**：T197 之后已经不对，本任务没有改。
5. **worktree 和分支请保留**：合并后还要叫我回来下载 15 份摘要、跑 `verify --milestone M1a`。
