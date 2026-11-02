# T099 汇报

- 执行者：子 agent（测试工程师）
- 分支：`chore/T099-m1a-exit`（第一部分，PR #33，已合并为 7b67605）；`chore/T099-m1a-exit-2`（第二部分，PR #37，最后一个提交 d2c37c4）
- 完成日期：2026-11-02
- PR #37：https://github.com/boris-w/cupper/pull/37 ，CI 运行 https://github.com/boris-w/cupper/actions/runs/41423965306

**状态：除 PR #37 的 CI 外都已完成。** M1a 退出候选是 `ec459fd`。
- 交汇报时 PR #37 有 6 个检查已通过：protocol、testdata-scan、e2e、license、ui、ts。rust 还在跑，compat-framework 在排队等 macOS runner。请合并前用 `gh pr checks 37` 确认这两个。
- 这个 PR 只改了文档、摘要和 `tests/ids.toml` 的 6 行。PR #33 的 8 个检查全部通过（运行 41395546629）。

**结论**
- 在 ec459fd 上，runner 能做的四项都通过了：nfr1 连跑 20 次、降速变体 5 次、`ci pr`、`ci nightly`。
- PERF 闸门所需的本机 nightly 按用户的决定（2026-11-02-perf-local，D1）没有跑。所以 `trace-ids verify --milestone M1a` 不通过：288 个编号里 7 个没完成，都是只有那次本机 nightly 才能产出的。我没有为了让它通过而改登记。
- 这 7 个编号和其余缺口都写进了 record.md，交同步会由用户决定。

## 做了什么

1. **e2e-repeat 加了两样**（第一部分，8024616），实际 diff +40 −10：
   - 带 `@focus` 的用例在其余用例之后一次跑一个，和 `xtask ci e2e` 一样。这样整份 spec 能按 CI 的并行度跑：其余用例同时 2 份，`@focus` 用例一次 1 个。
   - 新增输入 `load`：测试运行期间开几个忙循环进程。
   - spec 写 `src/nfr1`，同时匹配 `nfr1.spec.ts` 和 `nfr1-restart.spec.ts`，共 12 个用例。E2E-NFR1-5 在后一个文件里，任务书只写了前一个，我按 5.12.2 补上。
2. **登记表**（经你批准，3ad4747）：
   - IT-sys-peer-current 的套件由 `pr` 改为 `nightly`：T211 之后托管 runner 上的结果按大版本记作 -macos15/-macos26，PR 又都在托管 runner 上跑，-current 只剩作者机器的 nightly 能产出。
   - IT-ticket-auth 由 `conditional` 改为 `na`：K2 已达标，T079 不触发。
3. **第一个候选 18b3640 的 pr 失败，查明根因**：
   - 失败的是 `it_lifecycle_startup_ends_what_the_last_run_left_running`。测试起了一个 `trap '' TERM` 的子进程，没等 trap 生效就起宿主；宿主启动时的清理先发了 SIGTERM，子进程因此退出，结果是 `[15, 15]`。宿主的行为符合设计。
   - 当天 8f96864 上也这样失败过一次，当时没有记录。
   - 你开了 T216 修测试。之后又出了 T217（MAN-keyboard 时发现）和 T218，三者都已合并。新候选 ec459fd。
4. **在 ec459fd 上的运行**（e2e-repeat 用 `--ref main`）：

| 项 | 运行 | 结果 |
|---|---|---|
| nfr1 连跑 20 次 | https://github.com/boris-w/cupper/actions/runs/41414667873 | 其余用例 180/180，@focus 用例 60/60 |
| 降速 5 次（3 核里 2 个忙循环，load average 约 26） | https://github.com/boris-w/cupper/actions/runs/41414671830 | 其余用例 45/45，@focus 用例 15/15 |
| `ci pr` | https://github.com/boris-w/cupper/actions/runs/41413977102 | 8 个检查全部通过 |
| runner 上的 `ci nightly` | https://github.com/boris-w/cupper/actions/runs/41414675363 | 6 个作业全部通过 |

   - 两次连跑都是 unexpected 0、flaky 0、skipped 0，没有重试。
   - security-peer 在 macOS 15.8.2 上记作 IT-sys-peer-macos15（作业 125175535228），在 macOS 26.6.2 上记作 IT-sys-peer-macos26（作业 125175535565）。
   - 降速下，有几次 @focus 用例要 35–39 s（平时约 6 s），离 90 s 上限还有余量。
5. **核对**：
   - UNIT-unsupported-set：在 pr 的 rust 作业里通过。
   - 契约冻结：pr 的 protocol 作业里，`schema-compat` 报 "0 breaking, 0 additive"，`shell-api-m1a` 通过。
   - COMPAT：按 T202 的报告推导，5 个 M1a 编号全部通过。
   - MAN-keyboard：用户在 18b3640 的 e2e 构建上做，十项全部通过。evidence 绑定 Electron 44.9.0。输入法是豆包。
   - 操作脚本 `man-keyboard.md` 已改成延迟 15 秒发请求的写法，也写明了 T217 之后点击和自动接管的变化。
6. **入库的物料**（都在 `docs/tasks/T099-m1a-exit/` 下）：
   - `ci/pr/`、`ci/nightly/`：14 份 runner 摘要；
   - `record.md`：运行与结果、核对、缺口、18b3640 的旁证、同步会议程草稿；
   - `notes.md`：过程记录；
   - `man-keyboard.md`：操作脚本与结果。

## 关键取舍

- **候选失败后换候选重跑，不重跑失败的作业**：对已知的偶发失败重跑，等于按 1.8 不允许的方式重试。runner 的时间不贵。18b3640 上已经跑完的结果都通过了，写进记录作旁证：20 次 240/240、降速 60/60、nightly 6/6。
- **降速负载取 3 核的一半，向上取整为 2 个忙循环**：比向下取整的 1 个更严。
- **MAN-keyboard 用 e2e 构建加假 agent**：开发构建换不了假 agent，会去找真实的 Claude Code。代价是看不到原生确认框，这一点列为缺口。
- **trace-ids 不通过就照实写**：7 个编号逐条写了原因，没有改登记。

## 验证方式

- `trace-ids check` 一致：437 IDs。
- `node tools/trace-ids/src/main.ts verify --milestone M1a`，读入库的 14 份摘要加兼容报告，结果是 "M1a: 7 of 288 IDs not done"，退出码 1，没有 problem，没有未登记的编号。没完成的 7 个：
  - PERF-host-cold-start、PERF-index-snapshot、PERF-host-memory、PERF-snapshot、PERF-bridge-gate；
  - IT-sys-peer-current，以及等它的 IT-sys-peer。
- 改动的文档过了 prettier；diff 里没有本机路径和个人信息。
- `cargo xtask size`：第一部分 +44 −12，第二部分 +6 −0。停止线 200。
- 对照验收标准：
  - [x] nfr1 连续 20 次、降速 5 次全绿，附了运行链接。
  - [~] 同一提交上 `pr`、runner 上的 `nightly` 各通过一次。PERF 的结果没有：用户决定不在本机跑，已作为缺口写进记录。
  - [~] `trace-ids verify` 不通过，7 个编号逐条列了原因。
  - [x] 记录写好，缺口列清，交你转用户。
  - [ ] PR #37 的 8 个检查：6 个已通过，rust 和 compat-framework 还没出结果，请合并前确认。

## 提交列表

```
d2c37c4 docs(task): T099 record without the local nightly, its seven IDs as a gap
c82a5f4 docs(task): T099 record of the runs on ec459fd, the gaps and the sync agenda
c11885d docs(task): T099 summaries of ec459fd's pr and nightly runs on GitHub
488d833 docs(task): T099 notes on the first candidate, its runs, and T216-T218
bbbc665 test(ids): MAN-keyboard's evidence of 2026-11-02, with its record
（第一部分，已合并）
4877ab4 docs(task): T099 local runs: the nightly command and the MAN-keyboard procedure
3ad4747 test(ids): IT-sys-peer-current is nightly's, IT-ticket-auth does not apply
8024616 ci: e2e-repeat runs @focus tests after the rest and can load the runner
4cb44b6 docs(task): T099 notes with the candidate and the estimate
```

## 遗留问题 / 范围外发现

1. **请用户确认的缺口**（record.md 第 6 节；同步会议程第一项）：
   1. PERF 的 5 个编号和 IT-sys-peer-current 没有结果。替代办法三选：
      - (1) 改用 GitHub 大型 runner，重新标定阈值；
      - (2) PERF 只记录、不卡闸门，等 T160 之后在真机上补测；
      - (3) IT-sys-peer-current 单独跑一次 `security-peer --in nightly`，几分钟，负载低。
   2. e2e 在这次验收里没有在 macOS 26 上跑过；降速负载只有 CPU 一种。
   3. MAN-keyboard 是在 T217 之前做的。
   4. e2e 构建里看不到原生确认框。
   5. 用实体键盘查真实输入，只在 e2e 构建上做过。
   6. 真实 Claude 的兼容报告是 2026-10-28 在 2893c4e 上跑的，早于 T110 和 T218 对驱动层的改动。
2. **rust 作业遇到第一个失败的步骤就停**：后面的 shuttle、golden、native 三步不跑，一次失败会让 PROP-stream-seq 等编号显示为"没跑"。看失败的摘要时要留意。
3. **gh 经本地代理经常超时**，`gh run watch` 会提前退出。等运行结束最好用 `gh api` 轮询。
