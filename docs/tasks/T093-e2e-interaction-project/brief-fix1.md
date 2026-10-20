# T093 修复 1：机器锁用例的偶发失败

- 派发日期：2026-10-20
- 执行者：子 agent（前端工程师）。T093 的执行者（这条用例的作者）
- 分支：`fix/T093-lock-test-flake`
- 依赖：无

## 背景

`crates/xtask/src/ci/lock.rs` 的 `unit_ci_lock_command_passes_its_exit_code_and_is_told_it_holds_the_lock` 偶发失败，报"the lock outlived its command"。T176 跑 `ci pr` 时撞上一次，当时 xtask 的测试花了 12 s，平时约 3 s。

推测原因与 T037 修复 1 是同一类（T021 查明的窗口）：同一个测试进程里，别的测试线程在 `posix_spawn` 或 fork 时会短暂复制 fd 表。子进程 exec 之前也持有这把 `flock`，用例在命令结束后立刻检查"锁已释放"，就可能看到锁还被占着。生产环境里 `xtask lock` 一次只拉起一个命令，不受影响，所以这只是测试的问题。动手之前先确认这个推测；如果原因不同，先停下汇报。

## 范围

**做**：
1. **先复现**：例如在并行跑 xtask 的单元测试时循环运行这条用例，记下失败率。
2. **修复**：首选把依赖"命令结束后锁立刻释放"的用例放进单独的进程运行，做法照 T037 修复 1 用的 `testutil::alone`。xtask 用不到那个辅助时，在 xtask 的测试里加一个同样做法的辅助，并说明理由。不要用"短时限内重试"掩盖问题，除非能说明为什么单独进程不可行。
3. **同类排查**：在 `lock.rs` 的其他用例里搜一遍类似的依赖，同样处理。只改测试，不改生产代码。

**不做**：生产代码的改动；其他偶发失败（发现了就记下来，在汇报里提出）。

## 验收标准

- [ ] 修复前的失败率和修复后的连跑结果写进本目录的 `notes-fix1.md`：修复后在同样的并行负载下连跑至少 200 次，没有失败。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T093-e2e-interaction-project/ci/` 通过，摘要随任务提交。

## 规模

S，上限约 150 行。超出就停下汇报。

## 工作方式

在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b fix/T093-lock-test-flake main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report-fix1.md`。汇报里不要写本机的绝对路径。
