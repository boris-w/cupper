# T013 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：
  - 与 T018 并行，两边互相依赖，编排者把 `session/archive` 移给本任务（计划 v1.3.4）。
  - T018 合并后，编排者把 main 合入分支（`305f058`）。`catalog.rs` 有 7 处附加对附加的冲突，两边都保留；执行者随后改用 `InputEvidence` 和 `check_rows`，补上了 archive。
  - 完成后，编排者又把 main 合入一次（`a32efac`，带来 T048 等），在任务 worktree 里跑 `cargo xtask ci pr --record`：rust 575、ts 64、license 1，全部通过，摘要为 `ci/pr-a32efac3b10b.json`。另外单独确认了 `aarch64-apple-ios-sim` 构建通过。
- 核对：没有 `unsafe`，没有个人信息；`catalog.rs` 只做了附加。
- 代码审阅：
  - 构建门控：`BUILDS` 有默认值（空），漏写的风险由测试兜住——`Build` 格子必须恰好对应带 `x-builds` 的方法。接受。
  - 三种新格子 `Input`、`Build`、`CliOnly` 各有文档，写明交给谁处理；remote_device 的 ✗ 统一改用 `Sec9`。
  - `WorktreeBlocker` 正好对应 ADR-10 的 7 条清理规则，并有用例。
  - `git/diff.scope` 和 `git/*.target` 是封闭的外部标签枚举，只由客户端发送，宿主拒绝不认识的取值（5.3.10）。接受。
  - 设置用点号扁平键，`Settings` 和 `SettingsPatch` 由同一张键表生成。
- 规模：1,411 行（代码 973 行，测试 438 行），在上限以内。每完成一组统计一次行数，接近停止线时主动压缩了测试。
- 编排者裁定：
  - 执行者补的取值全部认可，写进 5.3.11 末尾的"T013 实现时的取值"，包括对 5.3.4 的三处更正：
    - `git/status.branch` 改为可选；
    - `diag/export.include_sessions` 是会话 ID 的列表；
    - `diag/recording_export` 的 scope 为 `ui_local`。
  - 两处 schema 不够精确（`Settings.cli_path` 的必填，`SettingsPatch` 的 `null`）都在客户端→宿主方向，宿主会拒绝，暂不处理，记给 T015。
  - CI 缺 `aarch64-apple-ios-sim` 的构建步骤：这是 T001 时漏掉的，5.1.6 要求每个 PR 都跑。交给 T002 补上。
- 转交事项已写进 `docs/tasks/handoff-notes.md`：T031、T030/T033、T015、T036/T126、T047、T002。
