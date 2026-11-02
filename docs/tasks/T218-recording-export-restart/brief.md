# T218 录制导出用例：adapter 重启后的回合在负载下 30 秒内没完成

- 派发日期：2026-11-02
- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T218-recording-export-restart`
- 依赖：无（T216 发现）。T099 依赖本任务

## 背景

T216 在 runner 上把整个 lifecycle 二进制 3 份并行连跑时，`it_recording_export_two_instances_read_back_and_sanitize`（`crates/cupper-host/tests/lifecycle.rs` 约第 371 行）900 次里失败了 4 次：

- 运行：41395945555（copy 3 run 10）；41399294080（copy 1 run 6、copy 3 run 49、copy 1 run 184）。
- 4 次形态相同：`common/mod.rs` 的 `until` 等满 PATIENCE（30 s）后报 `SELECT status FROM turns ORDER BY turn_no: not ["completed", "failed", "completed"]`，二进制耗时约 32.9 s。
- 用例流程：第一个回合 `say hi; stderr oops` 完成 → 第二个回合 `exit 0`，adapter 退出，回合 `failed` → 第三个回合 `say again`，宿主要重启 adapter（`adapter_backoff_ms` 设为 1），回合要在 30 s 内完成。失败时第三个回合 30 s 内没有完成。
- 修前那 300 次里它没有失败，但样本小。T216 只改了另一个用例的子进程启动顺序。

M1a 的退出候选要求 `ci` 的 8 个检查一次通过，这个用例在 `rust` 作业里，所以要在定新候选之前查清。

## 范围

**做**：
1. 查清原因，给出证据（runner 上失败那几次的日志、宿主和 adapter 的日志、回合与 adapter 实例的时间线）。可能的方向，由你核实，不要先入为主：
   - 宿主重启 adapter 的路径在负载下真的慢到 30 s 以上（产品问题：退避、重启后的首个回合、队列里的消息怎么被取出）；
   - 第三条消息在 adapter 重启的窗口里丢了、卡住了，或者被拒了（产品问题）；
   - 只是负载下的正常耗时，30 s 的 PATIENCE 不够（测试问题）。
2. 是产品问题就修产品，是测试问题就修测试。修测试不能只是把等待时间加长掩盖问题：要先证明正常路径的耗时在负载下确实接近或超过 30 s，并说明为什么合理。
3. 在 runner 上用 `test-repeat` 对照：`kind=cargo`、`package=cupper-host`、`args=--features test-harness --test lifecycle`、`filter=it_`、`times=200`、`parallel=3`，修前（main）、修后（分支）各一次。要触发时把参数和提交发给 `main`。
4. 修前能用更集中的办法复现（例如只跑这个用例、加负载）更好，写进汇报。

**不做**：放宽 adapter 重启的退避或上限规则，除非查明它们本身就是缺陷，并先报编排者。

## 验收标准

- [ ] 原因有证据。
- [ ] 修后 runner 上整个二进制 600 次里这个用例 0 次失败，附运行链接。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：100；硬停止线：140

## 参考资料

- 设计：5.2.13（进程监护）、ADR-09（adapter 的重启与清理）、5.6（运行时与 agent 驱动）、录制（`diag/recording_export`）。
- 卷宗：T216（发现，report.md）、T205、T206（runner 负载下的停顿：host.db 带 fullfsync 的 WAL 检查点可能让会话流每轮停几秒）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条），高负载复现放到 runner 上。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
