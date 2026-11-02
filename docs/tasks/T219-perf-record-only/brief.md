# T219 PERF 改为只记录、不卡闸门；真机对端校验改为不适用

- 派发日期：2026-11-02
- 执行者：子 agent（工程师，xtask、workflow、trace-ids）
- 分支：`ci/T219-perf-record-only`
- 依赖：T099（M1a 同步会的决定 D1、D2）。合并后编排者打 tag `gate/m1a`

## 背景

M1a 同步会（`docs/meetings/2026-11-02-m1a-sync/minutes.md`）上，用户决定：

- **D1**：PERF 只记录、不卡闸门。PERF 照常在 GitHub 的 nightly 里跑，写进历史，只看趋势，不作为里程碑退出条件。有了自用构建、用户愿意时再在真机上补测。用户不在自己的电脑上跑压测（`docs/meetings/2026-11-02-perf-local/`）。
- **D2**：IT-sys-peer-current 改为不适用。用户的 Mac 是 macOS 26，nightly 的 `security-peer` 已在 macOS 26 的 runner 上覆盖（IT-sys-peer-macos26）。

现状：
- 设计 5.9.6 写着"CI 的 runner 是虚拟机，只看趋势：取历史中位数的 1.5 倍作为阈值"。但 T197 定了 `perf` 不上 CI，托管 runner 上 `perf` 作业按规则跳过（T099 的 nightly 摘要里可见）。
- PERF-bridge-gate 是 e2e 里的 `@perf` 用例，样本交给 `CUPPER_PERF_OUT`。
- M1a 的 `trace-ids verify` 现在因为 PERF-host-cold-start、PERF-index-snapshot、PERF-host-memory、PERF-snapshot、PERF-bridge-gate、IT-sys-peer-current（以及 IT-sys-peer）没有结果而不通过。

## 范围

**做**：
1. **nightly 在托管 runner 上跑 PERF，只记录**：
   - `nightly.yml` 加上 `perf` 作业（macos-15）。`xtask ci perf` 在托管 runner 上不再跳过，改为只记录：测量照做，样本和 p50、p95、最大值写进摘要，绝对预算不作为失败条件。
   - 作者机器上的行为不变（照旧按预算判定、写 `testdata/perf/history.jsonl`）。
   - 托管 runner 上的 e2e 作业里，PERF-bridge-gate 也一样只记录。
   - 摘要里这些编号记作"已测量"，能被 trace-ids 认作有结果。具体怎么表示由你设计，汇报里说明。
2. **登记表与 trace-ids**：
   - 让登记表能表达"只记录、不卡闸门"：PERF 编号有了测量结果就算完成，结果不按预算判定。M1a 的 5 个 PERF 编号，以及登记表里其他 PERF 编号，按 D1 一律这样登记。
   - `tools/trace-ids` 支持这种登记，有用例覆盖。
   - IT-sys-peer-current 改为 `status = "na"`，reason 引用 D2；IT-sys-peer 由其余部分覆盖。
   - 改完用 T099 入库的摘要跑 `node tools/trace-ids/src/main.ts verify --milestone M1a`：PERF 那 5 个还没有 runner 上的测量结果，所以需要先在你的分支上触发一次 nightly（找 `main` 触发），把这次 nightly 的摘要放进 T099 卷宗的 `ci/nightly/` 下，或者另放一处，由你提议。目标是 verify 通过。
3. **设计和计划要改的口径**：列进汇报，由编排者改。包括计划 1.8（"`perf` 不上 CI"）、设计 5.9.6 的判定口径、01-repo-build-ci 的作业表、12-milestones 的退出标准里涉及 PERF 的地方，以及你找到的其他地方。

**不做**：改测量本身、预算数值；在作者机器上跑任何东西（用户的决定）。

## 验收标准

- [ ] nightly 在托管 runner 上跑出 PERF 的测量结果，附运行链接。
- [ ] `trace-ids verify --milestone M1a` 通过，附输出。
- [ ] 设计和计划要改的地方逐条列出。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：150；硬停止线：200

## 参考资料

- 会议记录：`docs/meetings/2026-11-02-m1a-sync/`、`docs/meetings/2026-11-02-perf-local/`。
- 设计：5.9.6（性能预算）、5.9.10（编号与登记）、5.1.6（CI 作业）。
- 计划：1.6（编号与登记）、1.8（CI 的执行方式）。
- 卷宗：T099（record.md、ci/ 下的摘要）、T197、T211（托管 runner 的判断 `RUNNER_ENVIRONMENT`）、T002（trace-ids）。

## 注意

- 不在本机跑 `perf` 或整套 nightly，也不跑别的高负载测试（用户的决定和守则第 10 条）；本机只跑单元测试、lint 这类普通检查。
- 不运行真实的 Claude Code、Codex CLI。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
