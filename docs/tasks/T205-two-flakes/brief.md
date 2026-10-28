# T205 两个偶发失败：安装失败原因与 GOLD-B 收尾

- 派发日期：2026-10-28
- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T205-two-flakes`
- 依赖：无（T201、T066 发现）。T099 依赖本任务：M1a 退出要求同一提交上 `pr`、`nightly` 都通过，偶发失败会让它不可靠

## 背景

两个新近加进来的用例，在 GitHub runner 上各失败过一次，重跑后通过：

1. **`it_runtime_first_prompt_waits_for_the_install`**（IT-runtime-first-prompt-waits，`crates/cupper-host/tests/lifecycle.rs`，T201 新增）
   - main 的 push 运行 https://github.com/boris-w/cupper/actions/runs/40639964185 ，rust 作业的 `test` 步骤；
   - 失败信息：`assertion left == right failed`，`left: ["-"]`，`right: ["hash_mismatch"]`；
   - 也就是第二段"放行不符合 SRI 的字节"时，读到的 `adapter.failure` 还是空的。多半是用例读得太早：回合已经结束，但 `agents.updated` 里的失败原因还没写进去，或者读的是更早的一条。原始日志在这次运行的构件 `rust-macos-15-logs` 里。
2. **`gold_b_claude_approval`**（`crates/cupper-host/tests/gold_b.rs`，T066 新增）
   - PR #20 的运行 https://github.com/boris-w/cupper/actions/runs/40643620891 ，rust 作业的 `golden` 步骤；
   - 失败信息：`gold_b.rs:394:20`，`called Result::unwrap() on an Err value: Closed(Some(Eof))`；
   - 出错的是收尾的 `stop()` 里 `host/exit` 请求的应答：宿主在送出应答之前就关了连接。可能是测试的时序问题，也可能是宿主退出时没有先把应答写出去。后者是产品问题：客户端会看到连接断开，而不是"退出已受理"。

## 范围

**做**：
1. 两处都查明根因，写进 `notes.md`，附证据。
2. 修复：
   - 根因在测试里，就修测试，等的东西要等对，不用 sleep；
   - 根因在宿主里，就修宿主，并补一个能稳定复现的用例。`host/exit` 应当先送出应答、再关连接，照设计 5.2.13 的有序退出；设计没写清的地方，先汇报。
3. 在 runner 上连跑证明：用 `test-repeat` workflow（守则第 10 条：不在本机造负载）。每个用例都要有修复前的失败率和修复后的连跑结果。修复后两个用例各连跑 200 次，没有失败。需要触发 workflow 时，告诉 `main`，由编排者触发。

**不做**：改别的用例；改金样例和录制的内容。

## 验收标准

- [ ] 两处的根因都有证据。
- [ ] 修复后，runner 上两个用例各 200 次全部通过，附运行链接。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：150；硬停止线：200

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计。每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.2.12、5.2.13（运行时安装与有序退出）；5.9.1、5.9.3（测试规则、B 层回放）。
- 卷宗：T201（IT-runtime-first-prompt-waits 的夹具）；T066、T065（GOLD-B 运行器）；T199（`test-repeat` 的用法和连跑记录的写法）。
- `docs/03-plan.md`：T205 一行。

## 注意

- 不运行真实的 Claude Code、Codex CLI；不读、不改 `~/.claude`、`~/.codex`。
- 本机只单份跑与改动相关的测试，不循环、不并行重跑。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
