# T023 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，已合并。有一项验收没有做到，已转交后续任务。
- 核对：
  - 在任务 worktree 里重跑 `cargo xtask ci pr`，通过：rust 163 项，UNIT-startup-payload、UNIT-failpoint、UNIT-release-features 都是 pass；
  - 宿主 crate 里没有 `unsafe`；
  - diff 中的路径都是测试里的假路径；
  - 规模约 1,565 行，略超上限，测试占约一半，接受。
- **没做到的一项**：读完载荷后把 stdio 指向 `/dev/null`。标准库没有安全的 `dup2`，`unsafe` 又只能出现在 `cupper-sys`。执行者没有为此改用 rustix 绕开架构约束，而是停下来交给编排者决定，做法正确。
  - 裁定：`cupper-sys` 新增 `redirect_stdio_to_null()`，把 0、1、2 都指到 `/dev/null`，请正在 `cupper-sys` 里工作的 T021 顺带加上；`main` 里的调用由 T037 接入（读载荷的构建在读完之后调用，发布构建一开始就调用）。已记入交接事项。
- 接受的取舍：
  - 自己写故障点框架，用 `process::exit(99)` 退出；
  - 超时用"读线程 + `Clock` 定时器"，不用 `spawn_blocking`；
  - 时长和故障点的登记表一开始为空，由各使用方任务登记；
  - `harness_key` 的格式与 `dev_key` 相同。
- crate 外的改动：`xtask` 的 rust 作业对 cupper-host 增加了特性组合矩阵（无特性、只开 `dev-auth`、只开 `test-harness`）。这符合计划 5.7 中"T023 扩充无特性运行"的要求，接受。
- 合并：`Cargo.lock` 冲突，处理方式与以往相同，合并后核对依赖版本，并在 main 上重跑 CI。
- 转交：给 T025、T030、T037、T045、T056、T161 的事项已记入 `docs/tasks/handoff-notes.md`。
