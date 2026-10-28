# T206 NFR9 崩溃用例在 runner 负载下超时

- 派发日期：2026-10-28
- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T206-nfr9-crash-wait`
- 依赖：无（T199、T205 发现）。T099 依赖本任务：M1a 退出要求退出候选上的 `pr` 通过一次

## 背景

`it_nfr9_crash_under_load_kill9_again_and_again`（`crates/cupper-host/tests/crash_concurrency.rs`）在 runner 上偶发失败：
- PR #21：run https://github.com/boris-w/cupper/actions/runs/40649145948 ，`crash_concurrency.rs:344:13`，"no load in round 3"，这个测试二进制跑了 84.7 s；
- PR #4：run 40322065826，"no load in round 4"。

用例一共 5 轮：每轮先 kill -9 宿主再重启，之后要在 30 s（`PATIENCE`）内看到 200 条新的 `item.delta`。coalescer 按 40 ms 的节奏写，200 条至少要 8 s，runner 负载高时余量不够。T199 改过这个用例的写法，但没有去掉对墙钟余量的依赖。T205 的执行者核对过，这次失败与 T205 的改动无关。

## 范围

**做**：
1. 查明慢在哪里：是宿主重启、K6 的 resume、假 agent 的输出，还是 coalescer 的节奏，有证据（runner 日志、本机单份运行时的计时）。
2. 按用例要验的东西重新设计等待。NFR-9 要验的是：kill -9 之后能恢复，流不丢、不重，seq 连续。
   - 等待靠事件，或者靠虚拟时钟（`test_config.clock`、`test/clock_advance`），不靠墙钟赌余量；
   - 每轮需要多少负载才能证明"在负载下崩溃"，写明理由。不为了快而削弱验证。
3. 在 runner 上用 `test-repeat` 连跑证明：修复前量一次失败率，修复后连跑 200 次以上没有失败。要触发时把参数和提交发给 `main`。

**不做**：改宿主的崩溃恢复逻辑。真查到产品问题，停下汇报。

## 验收标准

- [ ] 慢的原因有证据。
- [ ] 修复后 runner 上 200 次以上全部通过，附运行链接；NFR-9 的验证没有削弱，汇报里逐条对照。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：150；硬停止线：200

<!-- 执行者开工前在 `notes.md` 里写估计。每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.9.1（测试规则）；5.2.16（时间与可测性）；NFR-9；5.6 的 K6。
- 卷宗：T199（第 3 处，这个用例的上一次修改）；T196；T063（崩溃恢复）；T205（`test-repeat` 的用法）。
- `docs/03-plan.md`：T206 一行。

## 注意

- 不运行真实的 Claude Code、Codex CLI；不读、不改 `~/.claude`、`~/.codex`。
- 本机只单份跑，不循环、不并行重跑（守则第 10 条）。kill -9 的负载场景只在 runner 上连跑。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
