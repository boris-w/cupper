# T216 IT-lifecycle 的清理用例在 trap 生效之前就发了 SIGTERM

- 派发日期：2026-11-02
- 执行者：子 agent（Rust 测试工程师）
- 分支：`fix/T216-lifecycle-reap-trap`
- 依赖：无（T099 发现）。T099 依赖本任务

## 背景

M1a 退出候选 18b3640 的 `ci` 运行（https://github.com/boris-w/cupper/actions/runs/41392847292 ）里，`rust` 作业有一个用例失败：

- `it_lifecycle_startup_ends_what_the_last_run_left_running`（`crates/cupper-host/tests/lifecycle.rs` 约第 119 行）；
- 断言 "SIGTERM, then SIGKILL"：期望 `[Some(15), Some(9)]`，实际 `[Some(15), Some(15)]`。
- 同一天 main 上 8f96864 的运行 41377958324 也这样失败过一次。

T099 的分析（读代码和日志得出，没有在本机复现）：

- 测试用 `sh -c "trap '' TERM; /bin/sleep 60; :"` 起一个忽略 SIGTERM 的后代，登记进 adapters 表，紧接着就起宿主。
- 宿主启动时的清理（`supervisor/reap.rs`）先发 SIGTERM，等到进程退出或 `reap_term_wait` 到期，再发 SIGKILL。这是对的。
- runner 负载高时，sh 还没执行到 `trap` 就收到了 SIGTERM，于是以 SIGTERM 退出。
- cupper-sys 的 `proc.rs` 里同类用例的写法是先 trap、再 `echo ready`，测试等读到 ready 才继续。

## 范围

**做**：
1. 核实原因：能在本机单份复现的话（例如在 trap 之前人为加延迟），做修前修后对照；复现不了就在 runner 上对照。
2. 只改测试：忽略信号的后代先输出就绪，测试读到就绪之后再起宿主，照 `proc.rs` 的写法。
3. 查一遍仓库里还有没有同样的写法（忽略信号、没有就绪握手），有的话一并改，列进汇报。
4. 在 runner 上用 `test-repeat` 连跑对照：`kind=cargo`、`package=cupper-host`、`args=--features test-harness --test lifecycle`、filter 写这个测试名，修前（main）、修后（分支）各 200 次 × 3 份并行。要触发时把参数和提交发给 `main`。

**不做**：改宿主的清理逻辑。

## 验收标准

- [ ] 原因有证据（复现或 runner 上的修前失败）。
- [ ] 修后 runner 上 600 次全部通过，附运行链接。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：40；硬停止线：60

## 参考资料

- 设计：5.2.13（启动时清理上次留下的进程组）、ADR-09。
- 卷宗：T099（发现，notes.md）、T055（后代进程组的表）、T059。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。用户正在机器前做手动检查时，不在本机编译大的目标；先写代码，编译和测试等编排者告诉你可以了再做，或者直接交给 runner。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
