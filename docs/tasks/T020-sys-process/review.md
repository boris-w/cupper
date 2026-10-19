# T020 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，已合并
- 核对：
  - 在任务的 worktree 里重跑 `cargo xtask ci pr`，结果为通过，rust 共 36 项；
  - `unsafe` 只出现在 `cupper-sys`，每个 unsafe 块都有 `SAFETY:` 注释，并开启了 `undocumented_unsafe_blocks`；
  - 没有发现个人信息；
  - 规模 1,625 行，略超上限，接受。
- 接受的取舍：
  - 进程信息改用 `PROC_PIDT_SHORTBSDINFO` 加 `sysctl`，这样父进程链能经过 root 进程走到 launchd；
  - 进程组的清理只提供记录、核对、发信号三个原语，等待和升级由 Supervisor 用 `Clock` 编排；
  - 用 `set -m` 复现 T003 里"后代进程在另一个进程组"的场景。
- 合并：`Cargo.lock` 与 main 冲突。处理方法是先取 main 的版本，再由 cargo 重新解析，新增的是 `cupper-sys`、`libc`、`thiserror`。合并后在 main 上重跑 CI。
- 转交：给 T021、T037、T038、T054、T055 的提醒已记入 `docs/tasks/handoff-notes.md`。
