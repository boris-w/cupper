# T008 修复 1：K21 回收用例的偶发失败与残留进程

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师）。T037 修复 1 的执行者接着做（同类问题：测试进程、fd 与进程组的竞态）
- 分支：`fix/T008-k21-reap-flake`
- 依赖：无

## 背景

1. **偶发失败**：T128 合并 main 后的 CI 里，`driver::runtime::tests::it_k21_reap_group_a_stop_reaps_descendants_in_groups_of_their_own` 失败了一次（`tests.rs:329`）：
   ```
   left:  [None, Some(ProcessInfo { pid: 27943, ppid: 1, pgid: 27943, sid: 27938, name: "sleep", .. })]
   right: [None, None]
   ```
   停止适配器之后，自成进程组的孙进程（假 agent 的 `spawn_group_child` 拉起的 `/bin/sleep 3600`）还活着，已被 launchd 收养。当时本机同时跑着另一个 worktree 的 CI。
2. **残留进程**：编排者当时在本机找到 12 个 ppid 为 1 的 `/bin/sleep 3600`，最早的已运行 37 分钟，成对出现：一个和别的进程同组，一个自成一组。说明各 worktree 的测试运行一直在漏这类孙进程，不只是失败的那一次。编排者已向运行超过 1 分钟的发了 TERM。

本任务查明这两件事的原因并修掉。

## 范围

**做**：
1. 复现并查明原因。可能的方向（未核实）：
   - 停止时枚举后代早于孙进程出现（`spawn_group_child` 的回复先到、进程后建，或者相反）；
   - 枚举与发信号之间进程改了组；
   - 用例的清理只在通过时执行，失败或被中断时不清；
   - 别的用例（K21 的其他用例、IT-adapter-restart、`IT-recording-export` 等）本来就不回收这些孙进程。
2. **区分生产缺陷和测试缺陷**。如果关闭顺序（设计 5.6.8：发信号前枚举全部后代，按各自的进程组发信号）在生产里也会漏掉进程，修生产代码，并补一条能稳定复现原问题的用例。如果只是测试的问题，只改测试。
3. **测试不留残留进程**：
   - 每个拉起孙进程的用例，无论通过、失败还是被中断，结束时都确认没有残留，例如用守卫在 drop 时按记下的进程组清理；
   - 在 notes 里写明做法，后面的进程级用例照用。

**不做**：K21 以外的进程级用例改写（T063 的范围）；发现了就记下来，在汇报里提出。

## 验收标准

- [ ] 原因写进本目录的 `notes-fix1.md`：复现的方法和次数、修复前的失败率。
- [ ] 修复后在并行负载下（例如同时跑 `cupper-host` 的整个 lib 测试）把 K21 的用例连跑至少 100 次，没有失败。
- [ ] 跑完整个 `cupper-host` 测试之后，本机没有新增 ppid 为 1 的 `sleep`（或别的假 agent 孙进程）。检查命令写进 notes。
- [ ] 生产代码有改动时，设计 5.6.8 的描述仍然成立；不成立就在汇报里提出，由编排者改设计。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T008-adapter-lifecycle/ci/` 通过，摘要随任务提交。

## 注意

- 发信号的用例只能向测试自己拉起的进程组发。清理残留时，只处理能确认是测试拉起的进程：按记下的 pid、pgid 和启动时间核对，不能按名字批量杀。
- 代码约束：子进程一律经 `cupper_sys::spawn`；`unsafe` 只在 `cupper-sys`；时间只经 `Clock`。

## 规模

S，上限约 250 行。超出就停下汇报。

## 工作方式

T037 修复 1 合并之后，在原来的 T037 worktree 里从最新的 main 开新分支：`git checkout -b fix/T008-k21-reap-flake main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report-fix1.md`。不要自己创建它。汇报里不要写本机的绝对路径。
