# T099 M1a 退出验收记录（检查点 K4）

- 退出候选：`ec459fd`（`ec459fd9699f0d487aa1af79c5b1e49009157dda`，main）。T216、T217、T218 和 T099 第一部分（PR #33）合并、T217 存档之后的提交。
- 第一个候选 `18b3640` 作废，见第 7 节。它上面的结果只作旁证。
- 日期：2026-11-02
- 在哪里跑：连跑、`ci pr`、runner 上的 `ci nightly` 都在 GitHub 托管的 runner 上，运行期间没人操作。作者机器只跑一次 `ci nightly`，用来判 PERF 闸门、产出 IT-sys-peer-current。
- 摘要：14 份 runner 摘要从构件下载后入库，在 `ci/pr/`、`ci/nightly/` 下；作者机器上的摘要放在 `ci/local/`。目录名就是构件名，例如 `security-peer-macos-26`。

## 结论

| # | 要求 | 运行 | 结果 |
|---|---|---|---|
| 1 | nfr1 连续 20 次全绿 | e2e-repeat [41414667873](https://github.com/boris-w/cupper/actions/runs/41414667873) | 通过：240/240 |
| 2 | 降速变体 5 次全绿 | e2e-repeat [41414671830](https://github.com/boris-w/cupper/actions/runs/41414671830) | 通过：60/60 |
| 3 | `ci pr` 通过一次 | ci [41413977102](https://github.com/boris-w/cupper/actions/runs/41413977102) | 通过：8 个检查全绿 |
| 3 | `ci nightly`（runner） | nightly [41414675363](https://github.com/boris-w/cupper/actions/runs/41414675363) | 通过：6 个作业全绿 |
| 3 | PERF 闸门（作者机器上的 `ci nightly`） | 见第 4 节 | （待填） |
| 4 | UNIT-unsupported-set | pr 的 rust 作业 | 通过 |
| 4 | `trace-ids verify --milestone M1a` | 见第 5 节 | （待填） |
| 4 | 契约已冻结 | pr 的 protocol 作业 | 通过：`schema-compat` 0 breaking、0 additive；ShellAPI 快照（`shell-api-m1a`）通过 |
| 4 | COMPAT 编号 | T202 入库的兼容报告 | 5 个 M1a 编号全部通过 |
| 4 | MAN 编号 | 入库的证据 | MAN-compat-claude、MAN-keyboard 都有证据 |

## 1. nfr1 连跑 20 次

- 运行：[41414667873](https://github.com/boris-w/cupper/actions/runs/41414667873)，`e2e-repeat --ref main`，提交 ec459fd。参数：`spec=src/nfr1 test=E2E-NFR1- times=20 workers=2 load=0`。
- **用例**：`src/nfr1` 同时匹配 `nfr1.spec.ts` 的 10 个和 `nfr1-restart.spec.ts` 的 2 个（E2E-NFR1-5），一共 12 个，就是 5.12.2 退出标准里 E2E-NFR1-1 到 -5 的全部用例。任务书只写了 `nfr1.spec.ts`，不含 -5，这里按退出标准补上。
- **并行度同 CI**：和 `xtask ci e2e` 一样分两轮。
  - 不带 `@focus` 的 9 个用例同时跑 2 份（runner 3 核，Playwright 配置取核数减一、最多 2）；
  - 带 `@focus` 的 3 个之后一次跑一个。
  - `--repeat-each=20`，`retries: 0`，没有跳过和隔离。
- **结果**：
  - 其余用例 180/180，用时 9.2 分钟，单个用例中位数 4.5 s，最长 11.3 s；
  - `@focus` 用例 60/60，用时 4.3 分钟，中位数 5.1 s，最长 5.9 s；
  - 构件里 `repeat-rest.json`、`repeat-focus.json` 的统计：unexpected 0、flaky 0、skipped 0，没有一次重试，12 个用例每个跑满 20 次。

## 2. 降速变体 5 次

- 运行：[41414671830](https://github.com/boris-w/cupper/actions/runs/41414671830)，参数同上，`times=5 load=2`。
- **负载**：测试运行期间开 2 个忙循环（`yes > /dev/null`），runner 3 个核取一半向上取整。输入 `load` 是本任务加的（第一部分，PR #33）。
  - 日志：`2 busy processes on 3 cores`；运行结束时 load average 26.07、26.83、17.58。
- **结果**：其余用例 45/45（3.3 分钟），`@focus` 用例 15/15（3.5 分钟）；unexpected 0、flaky 0、skipped 0，没有重试。
- **负载下明显变慢**：
  - 其余用例中位数 8.2 s，最长 15.7 s（不加负载时中位数 4.5 s）。
  - 有 4 次 `@focus` 用例要 35–39 s，平时约 6 s。
  - 离单个用例 90 s 的上限还有余量。

## 3. pr 与 nightly

**`ci pr`**：main 上 ec459fd 的 push 运行 [41413977102](https://github.com/boris-w/cupper/actions/runs/41413977102)，8 个检查全部通过：

| 作业 | runner | 通过 | 失败 | 用时 |
|---|---|---|---|---|
| rust | macos-15 | 2363 | 0 | 16.0 min |
| protocol | ubuntu-24.04 | 188 | 0 | 0.5 min |
| ts | ubuntu-24.04 | 513 | 0 | 0.7 min |
| license | ubuntu-24.04 | 6 | 0 | 0.1 min |
| testdata-scan | ubuntu-24.04 | 4 | 0 | 0.1 min |
| compat-framework | macos-15 | 15 | 0 | 1.0 min |
| ui | macos-15 | 35 | 0 | 0.9 min |
| e2e（`@smoke`） | macos-15 | 32 | 0 | 7.2 min |

- 用时是摘要记下的 xtask 作业时间，不含 runner 的准备和编译缓存。
- `testdata-scan` 跳过了 `sensitive-words`：托管 runner 上没有身份名单（计划 1.8）。
- 工具链：rustc、cargo 1.96.0；node 24.13.0；pnpm 12.14.0。
- e2e 产物哈希（pr 和 nightly 两次构建逐字节相同）：`Cupper E2E.app` 2551bb3892ae…、`CupperHost.app` 5d5f8f6defcf…、`cupper` 471da69b0124…，全文在摘要里。

**runner 上的 `ci nightly`**：在 main（ec459fd）上手动触发，[41414675363](https://github.com/boris-w/cupper/actions/runs/41414675363)，6 个作业全部通过：

| 作业 | runner | 结果 |
|---|---|---|
| differential | macos-15 | 3 通过 |
| security-peer | macos-15（macOS 15.8.2） | 18 通过；记作 IT-sys-peer-macos15（[作业](https://github.com/boris-w/cupper/actions/runs/41414675363/job/125175535228)） |
| security-peer | macos-26（macOS 26.6.2） | 18 通过；记作 IT-sys-peer-macos26（[作业](https://github.com/boris-w/cupper/actions/runs/41414675363/job/125175535565)） |
| e2e（全集） | macos-15 | 52 通过；`perf` 按托管 runner 的规则跳过 |
| compat-framework | macos-15 | 15 通过 |
| codex-drift | ubuntu-24.04 | 2 通过 |

## 4. PERF 闸门（作者机器）

（待填：编排者在主工作区跑 `cargo xtask ci nightly --record .private/t099-nightly`，结果交来后填写。）

## 5. 核对

- **UNIT-unsupported-set**：pr 的 rust 摘要里结果是 pass，suite 是 pr，提交 ec459fd。
- **契约已冻结**（pr 的 protocol 作业，步骤日志在构件 `protocol-ubuntu-24.04-logs` 里）：
  - `schema-compat`：`tools/schema-compat` 拿 `protocol/released/v1-m1a.json` 比对 `protocol/schema/v1.json`，输出 "0 breaking, 0 additive"；
  - ShellAPI 快照：`shell-api-m1a`（`tsc --build packages/shell-api`，核对 `api-m1a.d.ts`）通过；
  - 同一作业里 UNIT-proto-fixtures、UNIT-ts-fixtures、UNIT-views-parity 也都通过。
- **COMPAT**：按 `testdata/compat/reports/claude-agent-acp-0.91.0-claude-2.1.296.json`（T202，claude-agent-acp 0.91.0，Claude Code 2.1.296）推导：
  - COMPAT-claude-thinking、COMPAT-untrusted-project-claude、COMPAT-claude-six-sessions、COMPAT-claude-close-running、COMPAT-claude-resume-procs 都是 passed in compat；
  - 报告里另有 16 个剧本没有 COMPAT 编号，trace-ids 不判。其中 13 个通过，steer 不适用（M1b），hooks-reload、config-scope 是观察项。
- **MAN**：
  - MAN-compat-claude：证据 2026-10-28（T066）；
  - MAN-keyboard：证据 2026-11-02，Electron 44.9.0，记录在 `man-keyboard.md`。十项全部通过。作者在 18b3640 的 e2e 构建上做的，按登记规则，证据绑定 Electron 版本，不绑定提交。
- **登记表**：本任务改了两处，都经编排者批准（第一部分，3ad4747）：
  - IT-sys-peer-current 的套件由 `pr` 改为 `nightly`：T211 之后 PR 都在托管 runner 上跑，-current 只能由作者机器上的 nightly 产出；
  - IT-ticket-auth 由 `conditional` 改为 `na`：K2 已达标，T079 不触发。
- **`trace-ids verify --milestone M1a`**：只读入库的、同一提交的摘要。命令：

  ```sh
  node tools/trace-ids/src/main.ts verify --milestone M1a \
    $(git ls-files 'docs/tasks/T099-m1a-exit/ci/*.json' | sed 's/^/--summary /') \
    --compat testdata/compat/reports/claude-agent-acp-0.91.0-claude-2.1.296.json
  ```

  - 只用 runner 的 14 份摘要时，288 个编号里还有 7 个没完成：PERF-host-cold-start、PERF-index-snapshot、PERF-host-memory、PERF-snapshot、PERF-bridge-gate、IT-sys-peer-current，以及等它的 IT-sys-peer。这 7 个都只能由作者机器上的 nightly 产出（第 4 节）。没有 problem，也没有未登记的编号。
  - 加上作者机器的摘要之后：（待填）。

## 6. 本机替代不了的缺口（请用户确认）

1. **macOS 15、26 上的对端校验**：已由 runner 覆盖。nightly 41414675363 的 `security-peer` 在 macOS 15.8.2 和 26.6.2 上各跑一次，记作 IT-sys-peer-macos15、-macos26，都通过。作者机器（macOS 26.4）上的那一份记作 -current（第 4 节）。macOS 14 自 2026-11-02 起不再支持。
2. **CI 负载下的稳定性**：已由 runner 上的连跑覆盖。第 1 节 20 次（240 次运行），第 2 节降速 5 次（60 次运行），都没有失败。
   - 局限：runner 只有 macOS 15。e2e 在 macOS 26 上只有作者机器 nightly 里跑的一次全集，没有连跑。
   - 局限：降速负载是 3 核里占 2 核的忙循环，代表不了磁盘或内存紧张的情况。
3. **PERF 只在作者机器上判**：runner 不是参考机型。每个指标只在一台机器、一次运行上判；趋势判定靠 `testdata/perf/history.jsonl` 里同一机型的历史。
4. **MAN-keyboard 是在 18b3640 上做的，T217 之后没有重做**：
   - T217 改了请求面板的焦点行为：点击面板会移焦点；RP-3 改为读页面里的焦点。按键的处理（主进程 `inputGuard`、宿主对真实输入的核对）没有改。
   - T217 的改动由 5 个单元测试和 E2E-SEC-panel-arming 守着，后者在 runner 上连跑了 50 次。
   - 请用户确认这条证据可以继续用，不必在 ec459fd 上重做。
5. **e2e 构建里看不到原生确认框**：主进程的对话框换成了测试替身。实体键盘按特权选项编号会弹出原生确认框，这一步没有亲眼看到，由 E2E-SEC-panel-numbering 自动覆盖（按键是 Playwright 的）。
6. **真实输入只在 e2e 构建上用实体键盘查过**：开发构建的宿主不能换成假 agent；自用构建上的完整流程要到 M1b 的 T160 和 MAN-S1 至 MAN-S4。
7. **真实 Claude 的兼容结果早于候选**：
   - T202 的报告是 2026-10-28 在 2893c4e 上跑的；
   - 之后驱动层有两处改动：T110 把通用的提取代码从 Claude 的提取层里移出；T218 改了 adapter 退出时回合的结束方式。
   - 这两处都由 GOLD-A、GOLD-B（假 agent 回放真实录制）和单元测试覆盖，但真实 CLI 没有在 ec459fd 上重跑。按计划 1.10，真实运行只由维护者手动做，消耗额度。

## 7. 第一个候选 18b3640

- `ci` 运行 [41392847292](https://github.com/boris-w/cupper/actions/runs/41392847292)：rust 作业里 `it_lifecycle_startup_ends_what_the_last_run_left_running` 失败，断言 "SIGTERM, then SIGKILL"，实际是 `[Some(15), Some(15)]`。其余 7 个作业通过。
  - 同一天早些时候，main 上 8f96864 的运行 41377958324 也这样失败过一次，当时没有记录。
- **根因**在测试：测试起了一个 `trap '' TERM` 的子进程后，没等 trap 生效就起宿主。宿主启动时的清理先发了 SIGTERM，子进程那时还是默认处理，被 SIGTERM 结束。宿主的行为（先 TERM、等待、再 KILL）符合设计。
- **处理**：按 1.8 不靠重跑算作通过，另开 T216 修测试（加就绪握手）。之后又发现两处，各开了任务，三者都已合并：
  - **T217**：MAN-keyboard 时发现点击面板不移焦点、自动接管不稳定。产品问题，改在 ui-core。
  - **T218**：T216 在 runner 上连跑时发现录制导出用例在负载下偶发失败。查出是产品缺陷：adapter 退出后约 10 ms 的窗口里，新消息会发给已经死掉的实例并被丢掉。改在宿主的 runtime。
- **18b3640 上已经跑完的结果**（只作旁证；连跑那两次跑在分支提交 8024616 上，产品代码和 18b3640 相同）：
  - nfr1 连跑 20 次（[41393695454](https://github.com/boris-w/cupper/actions/runs/41393695454)）：240/240；
  - 降速 5 次（[41393728018](https://github.com/boris-w/cupper/actions/runs/41393728018)）：60/60；
  - runner 上的 nightly（[41393760316](https://github.com/boris-w/cupper/actions/runs/41393760316)）：6/6。

## 8. M1a 同步会议程草稿

供编排者和用户开会时用，会后另写纪要（`docs/meetings/`）。

1. **M1a 做了什么**
   - 范围：设计 5.12.2 的全部内容，逐项对照见计划 5.1。
     - M1a 的任务已合并 155 个，包括本次验收中加的 T216–T218。
     - 两个条件任务不触发：T057（K1 通过）、T079（K2 达标）。
     - M1b 提前完成了 T101、T102、T110、T114。
   - 检查点：K1（技术验证，2026-10-19）、K2（测量闸门，T078 达标）、K3（契约冻结，T016）、K12（首批真实录制，T066）都已通过；K4 就是本记录。
   - 工程：
     - GitHub Actions：每个 PR 跑 8 个检查，nightly 有 6 个作业，另有 `test-repeat`、`e2e-repeat` 两个手动连跑 workflow（T197）。
     - 最低支持版本提到 macOS 15，对端校验在 macOS 15、26 的 runner 上跑（T211）。
     - T189 起的 fix 任务修掉了 runner 和真实运行里暴露的偶发失败与产品缺陷。
2. **退出验收的结果**：本记录的结论表和第 1–5 节。
3. **请用户确认的缺口**：第 6 节逐条确认。
4. **遗留问题**（交接事项里与 M1b 有关的，按影响排列）
   - **流延迟**：runner 上 3 份并行时，宿主的会话流每轮会停顿几秒（最长 12.4 s，中位数 1.9 s）。推测与 WAL 检查点的 fullfsync 有关，没有验证。这关系到 NFR-1 的流延迟，要在参考机型上量（T103、M1b 的性能任务）。
   - **退出时的应答**：
     - `host/exit` 的应答仍可能被客户端按 5.8.1 拒收（T213、T215）。桌面和 CLI 都已把连接结束当作退出已受理。
     - 和退出撞上的其他请求，应答可能随连接关闭而丢失，能按 `command_id` 重试拿到（T205）。
   - **adapter 退出**（T218 的交接）：
     - T111 接 Codex 时套用同一条规则；
     - "连接关了、进程不退"，以及 attach 途中 adapter 退出，这两种同类窗口还没处理。
   - **请求面板**（T217 的交接）：只能自由输入、没有选项列表的面板，自动接管和点击都没有地方放焦点。
   - **e2e 排查条件**：
     - 失败时没有截图和 trace（T212）。
     - e2e 构建里 App 退出后宿主约 1 s 内按孤儿规则退出，只核查了 `app.spec.ts`（T208）。
     - 关掉 Radix 菜单后立即移焦点的用例，可能有和 T217 一样的竞态。
   - **CI 本身**：
     - `cupper-sys.yml` 还没在 macos-26 上真正跑过（T211），下一个改 cupper-sys 的 PR 要看到它通过。
     - Rust 二进制的 minos 和 `LSMinimumSystemVersion` 15.0 不一致，交 T160 一起定（T211）。
   - **流程上的教训**：
     - MAN-keyboard 原定 W8 做，到退出验收才做，还顺带发现了 T217。人工项要在检查点之前排进日程。
     - 检查点通过、CI 改动之后，登记表没有跟着改：IT-ticket-auth 在 K2 达标后仍是 conditional；T211 之后 IT-sys-peer-current 的套件和 CI 对不上。以后检查点通过、CI 改动时，同一个提交里同步登记表。
     - 同一个偶发失败（T216）当天出现两次，第一次没有留下记录。main 上 CI 失败时，即使重跑通过，也要先记下来。
5. **M1b 第一波（计划 4.2 的 B1）**
   - 任务：T107 Electron 大版本跟进、T111 Codex 接入、**T120** Git 服务 III（关键链）、T124 worktree 创建、T126 文件搜索与内容映射、T127 存储用量与清除快照、T141 配置选择、T143 消息流增强 I、T150 系统通知、T156 防休眠、T160 自用构建、T173 S1–S4 检查表与人工证据格式。
   - 顺序上的要求：T107 要先于 SP-6（T103）；T150 依赖 K5（已有条件通过，T158 不触发）。
   - 需要用户参与的：
     - T107 的 MAN-keyboard：新的 Electron 大版本要照 `man-keyboard.md` 再做一次，本次的证据只对 44 有效；
     - T160 交付自用构建后，读"自用前须知"，开发构建和自用构建会同时运行。
   - 并行度：照常保持约 4 个子 agent。
6. **约下一次的时间**：M1b 中段的检查点（K7、K9、SP-6）。
