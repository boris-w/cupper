# T207 原生扩展的中继用例在 runner 上超时

- 派发日期：2026-10-30
- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T207-native-relay-flake`
- 依赖：无（T206 发现）。T099 依赖本任务：M1a 退出要求退出候选上的 `pr` 和 `nightly` 各通过一次

## 背景

`relay::tests::unit_native_relay_forwards_both_ways_until_the_host_closes`（`crates/cupper-desktop-native/src/relay.rs`）在 PR #22 的运行 https://github.com/boris-w/cupper/actions/runs/40661950733 里失败过一次，在 rust 作业的 `test-native-release` 步骤，重跑后通过：
- `relay.rs:182` 的 `read_to_end` 等满了测试设的 10 s 读超时，报 `WouldBlock`（os error 35）；
- 也就是宿主一端关闭之后，中继没有在 10 s 内把主进程那一端也关掉，交出 EOF。

两种可能：
- 测试的等待太紧；
- 中继在负载下关得慢。后者是产品问题：窗口会晚发现连接已经断了（设计 5.7.1）。

## 范围

**做**：
1. 查明根因，写进 `notes.md`，附证据。可以用临时的延迟把窗口拉大来复现，不提交。
2. 修复：根因在测试就修测试，等的东西要等对，不用 sleep；根因在中继就修中继，并补能稳定复现的用例。
3. 在 runner 上用 `test-repeat` 连跑证明：修复前量一次失败率，修复后 200 次以上没有失败。
   - 注意这个用例在 release 特性下跑（`test-native-release` 步骤），`test-repeat` 的参数要和它一致；
   - 要触发时，把参数和提交发给 `main`。

**不做**：中继的其他行为。

## 验收标准

- [ ] 根因有证据。
- [ ] 修复后 runner 上 200 次以上全部通过，附运行链接。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：150；硬停止线：200

<!-- 执行者开工前在 `notes.md` 里写估计。每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.7.1（进程与连接，窗口的数据连接）；5.9.1（测试规则，屏障要来自被断言的那条流）。
- 卷宗：T073（原生扩展）；T190；T205、T206（偶发失败的查法和 `test-repeat` 的用法）。
- `docs/03-plan.md`：T207 一行。

## 注意

- 不运行真实的 Claude Code、Codex CLI；不读、不改 `~/.claude`、`~/.codex`。
- 本机只单份跑，不循环、不并行重跑（守则第 10 条）。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
