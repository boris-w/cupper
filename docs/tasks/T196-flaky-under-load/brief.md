# T196 高负载下的两处偶发失败

- 派发日期：2026-10-26
- 执行者：子 agent（Rust 工程师，宿主核心）。T037、T117 等的执行者接着做（`tests/lifecycle.rs` 是它在 T037 写的）
- 分支：`fix/T196-flaky-under-load`
- 依赖：无。T099 依赖本任务

## 背景

M1a 的退出验收（T099）要求在同一个提交上**连续 20 次全绿**，不允许重试、跳过或隔离用例。T190 跑带记录的 CI 时碰上两处与它无关的偶发失败，单独重跑都通过（计划 v1.3.64）：

1. **`trust::tests::unit_trust_scan_forking_directory_links_hit_the_entry_limit`**：用墙钟断言扫描在 10 s 内结束，那次用了 23.4 s，机器平均负载约 16。
2. **`it_lifecycle_host_exit_and_sigterm_exit_in_order`**（`tests/lifecycle.rs`）：报 "Sigterm: no host/shutdown"，负载约 27。

## 范围

**做**：
1. **先复现**：在高负载下循环运行，记下失败率。负载的造法照 T093 修复 1、T189：后台跑几份整套测试。
2. **查明根因**：
   - 第 1 处：墙钟断言想证明的是"扫描在第 20,001 个条目处停下，而不是走完 2^20 次"。如果断言的是这件事本身，例如走过的条目数，就不受负载影响；
   - 第 2 处：分清是用例的等待在负载下偏紧，还是宿主在 SIGTERM 下真的跳过了 `host/shutdown`，例如退出路径上的竞态。后者是产品问题。
3. **修复**：
   - 原因在产品里、并且在计划 1.2 的上限内（单处不超过 100 行、不改接口）的，就地修，单独提交；超出的停下汇报；
   - 只放宽等待时，要说明为什么放宽是对的，而不是在掩盖问题；
   - 不要用"失败就重试"。
4. **同类排查**：两个文件里同样写法的用例（墙钟上限、等关闭通知）一并检查。

**不做**：其他偶发失败。发现了记下来，在汇报里提出。

## 验收标准

- [ ] 两处的复现数据、根因、修法写进本目录的 `notes.md`。
- [ ] 修复后在同样的负载下，两条用例各连跑至少 200 次无失败。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T196-flaky-under-load/ci/` 通过，摘要随任务提交。

## 交接事项

- T093 修复 1、T189：复现偶发失败的做法，`testutil::alone`；T189 修假 agent 退出等待时的分析方法：先量出实际等了多久。
- T035：信任扫描的条目上限（`notes.md`）。
- T037：IT-lifecycle 的写法。

## 规模

- 停止线：200；硬停止线：240

<!-- 执行者开工前在 `notes.md` 里写估计。每写完一个模块就跑一次 `cargo xtask size`，不要等全部写完；推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T037 worktree 里从最新的 main 开新分支：`git checkout -b fix/T196-flaky-under-load main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充（2026-10-26，编排者）

T191 跑带记录的 CI 时（负载约 45）又碰上两处，一并纳入：
3. **`it_cli_commands_read_elevate_write_and_answer`**（`tests/lifecycle.rs`）：第三条 `send` 得到 `queued`，断言期望 `started`。看起来是用例在上一个回合结束前就发了下一条；单独跑 5 次都通过，耗时 3.5–13.6 s。
4. **fake-agent 的 `unit_replay_*`**：三个用例报 "no response 1" 和 "the agent did not exit"。T191 的分支开在 T189 合并之前；T189 修好的 `Fake::exit` 也覆盖 `replay.rs` 的五处，先在最新的 main 上确认是不是已经修好。没修好的照前两处处理。

验收：第 3 处同样连跑 200 次无失败；第 4 处给出在最新 main 上的复现结论。

- 停止线：260；硬停止线：300
