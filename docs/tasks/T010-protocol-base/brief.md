# T010 协议基础：分帧、信封、ID、错误码、规范 JSON 与注解

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T010-protocol-base`
- 依赖：T001（已合并）。处在 M1a 第 1 波，属于关键路径：T001 → **T010** → T011 → T014 → …

## 背景

`cupper-protocol` 是宿主、CLI、桌面端、将来 iOS 端共同的契约 crate。它不做 I/O，要能编译到 iOS（ADR-02、ADR-04）。本任务先打基础：分帧、消息信封、ID、错误码、规范 JSON 与摘要、schema 注解的机制、限额。具体的事件、方法和视图分别由 T011–T014 完成。

## 范围

**做**（计划 T010 一行）：
- 从 SP-4a 迁移以下模块（`spikes/SP-4a-host-skeleton/crates/cupper-protocol/`）：
  - `frame`：`u32 长度 | u8 类型 | 负载`，单帧不超过 64,000 字节；
  - `rpc`：`Method` trait 增加关联常量 `SCOPE` 和 `IDEMPOTENCY`；
  - `open_enum`、`text`、`coalesce`、`sync`，以及 `transport` 中不做 I/O 的部分。

  沿用和改动的清单见设计 5.2.15。
- 事件信封与流 ID；带前缀的 ID 类型；错误码与 reason（5.3.8）；限额（5.3.9）。
- 规范 JSON 用 RFC 8785（JCS），`d_` 摘要保留完整的 256 位（5.3.1、5.4.4）。
- 六种 schema 注解的机制：`x-scope`、`x-idempotent`、`x-mergeable`、`x-id`、`x-time`、`x-path`。
- 整数上界：协议里的整数都不超过 2^53−1。

**不做**：具体的事件（T011）、方法（T012、T013）、视图（T014）；schema 导出与 TS 生成（T015）。

## 验收标准

- [ ] UNIT-digest-jcs：JCS 规范化与摘要，包括 Unicode 转义、数字、键排序等边界情况。
- [ ] 新增 UNIT-proto-base：
  - 分帧往返；
  - 超过上限的帧被拒；
  - 开放枚举遇到未知值时落到 `Unknown`；
  - 错误码往返；
  - 整数上界。
- [ ] `cupper-protocol` 能编译到 `aarch64-apple-ios-sim`（`cargo build -p cupper-protocol --target aarch64-apple-ios-sim`）。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T010-protocol-base/ci/` 通过，摘要随任务一起提交。
- [ ] 用例编号按计划 1.6 的约定：测试函数名以编号的 snake_case 开头，例如 `unit_proto_base_...`。

## 注意

- **代码约束**：严格按 `CLAUDE.md`：
  - 不做 I/O；
  - 错误类型用 thiserror（白名单已加入 `Unicode-3.0`，derive 宏可以用）；
  - 导出给其他语言的产物不依赖 serde_json map 的键顺序；
  - SDK 一律锁精确版本。
- **跨任务约束**：以后 T011–T014 会在同一个 crate 里分文件推进，所以模块要分好，`lib.rs` 只放 `mod` 声明。
- **规模**：上限约 1,500 行（含测试）。明显会超出时，先停下来汇报。

## 参考资料

- 设计：`docs/design/03-protocol-v1.md` 5.3.1、5.3.8、5.3.9；`04-data-model.md` 5.4.4；`02-host.md` 5.2.15；`01-repo-build-ci.md` 5.1.3。
- ADR-04；计划 `docs/03-plan.md` 1.3、1.6，以及 T010 一行。
- SP-4a 的代码与报告。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。
