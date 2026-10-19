# T018 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：修改一处后通过，合并。
- 过程：
  - 本任务由 T012 拆分而来。
  - 执行中发现与 T013 互相等待：T018 等 T013 的阻塞原因枚举，T013 的 `git/revert.input` 要复用 T018 的 `InputEvidence`。编排者裁定把 `session/archive` 移给 T013（计划 v1.3.4），本任务不带 archive 先合并。
  - 编排者把 main 合入分支（`f9b5d24`）。
- 核对：
  - 没有 `unsafe`，没有个人信息，非测试代码里没有 `unwrap`、`expect`；
  - 分支里没有 `session/archive` 的残留；
  - 能编译到 `aarch64-apple-ios-sim`。
- 审阅意见（已改）：`KeyDigit` 的 `TryFrom` 错误类型原来是 `String`，不符合"错误类型用 thiserror"的约束，而且是整个 crate 唯一的例外。执行者改成 `DigitOutOfRange(pub u8)`（`945a031`），重跑 CI：rust 538、ts 64、license 1，全部通过。摘要为 `ci/pr-945a03176cae.json`。
- 代码审阅：
  - 三组方法照 5.3.4 和 5.3.11 的裁定实现：`findings`、`diff_from_trusted`、`fallback_reason`、队列满、补充的错误码。
  - `Sec9` 格子的用例核对两件事：remote_device 确实持有这些 scope；拒绝它们的是策略，不是 scope。
  - `KeyDigit` 在类型层和 schema 层都限制在 1–9。
  - T012 的逐格比对抽成共用的 `check_rows`，T013 合入后会改用它。
- 规模：1,355 行，代码约 825 行、测试约 530 行，在上限以内。每完成一组都统计过行数，执行了守则第 8 条。
- 实现时补的形状已写进 5.3.11 末尾的"T018 实现时的取值"；给 T031、T035 的交接已写进 `handoff-notes.md`。
