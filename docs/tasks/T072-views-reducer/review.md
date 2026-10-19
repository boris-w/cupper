# T072 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：执行者与 T071、T087 是同一位。分支从最新的 main 开出，合并前 main 没有新提交。执行者在干净的 worktree 里跑了两次 CI，都通过：
  - `cargo xtask ci pr --record`：rust 1074、protocol 185（含 `views-ts`）、ts 314、license 1、testdata-scan 2，摘要在 `ci/` 下；
  - `cargo xtask ci differential --record`：UNIT-views-differential 为 pass。
- 核对：没有新依赖，没有个人信息。`views.json` 约 128 KB，是 Rust 写出的数据，不计入规模。
- 审阅：
  - **TS 视图**：规则与 `cupper-protocol::view` 逐条对应，比较的是"按线上发送时的样子"序列化之后的 JSON。
  - **未知或残缺的事件**：放进 `unknown` 列表，不进视图 JSON，与 Rust 一致。占位显示在消息流里的位置，交给 T082 定。
  - **交叉验证**：样例由 Rust 写出（`UPDATE_GOLDEN=1`），TS 读同一份文件比较。
  - **差分测试**：Rust 的 proptest 生成 300 条随机流，每条按三种方式 fold；TS 读文件比较。只在 nightly 跑。执行者另外本机换种子跑了 4 次，全部相等。
  - **变异检验**：改坏 5 处，每处都有 parity 用例失败。
- 编排者的后续处理：
  - 设计 5.1.6 的作业表缺 `differential`，已补；
  - 单独跑 `xtask ci differential` 时，摘要的套件记为 `differential`；`tests/ids.toml` 登记的是 `nightly`，以 nightly 组合作业的运行为准。
- 规模：971 行（代码约 520 行，测试约 450 行），在上限以内。
- 转交事项已写进 `handoff-notes.md`：T080、T082，以及改动视图时重新生成 `views.json` 的方法。
