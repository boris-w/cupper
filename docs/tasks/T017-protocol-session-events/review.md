# T017 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，已合并。
- 核对：
  - 在任务 worktree 里重跑 `cargo xtask ci pr`，通过：rust 237 项，UNIT-proto-events-session 和 UNIT-proto-events-index 都是 pass。
  - 能编译到 `aarch64-apple-ios-sim`。
  - 没有个人信息，也没有新增依赖。
- 规模：1,644 行，超出上限约 10%，超出的部分全在测试。执行者开工前先做了模块级估算，定了 1,650 行的停止线，并逐个模块统计，做法符合守则第 8 条，接受。
- 5.12.5 第 2 组：T011 和 T017 的对照表合起来，覆盖了事件与领域类型的全部条目。
- 编排者对三个设计空缺的裁定，已写进 5.3.11：
  1. 权限请求的 `tool.paths` 超出预算时：完整列表放进 `detail`（以 blob 形式），`paths` 只保留前若干项，并设 `content_truncated`。
  2. `user_message` 的 `content` 始终完整，超长的文本以 `text_blob` 引用，不拆成 `item.delta`。拆成 delta 只用于 agent 一侧的条目。
  3. `ItemPatch` 里出现的字段整体替换，不出现的保持不变，没有"清空"的写法。客户端按 `output_state` 等状态字段来判断。
- 执行者补的取值（`Guidance`、`StopReason`、`SnapshotReason`、`ExclusionReason`、`ResolutionReason`、`PlanPriority` 等）也已写进 5.3.11。
- 转交事项已写进 `docs/tasks/handoff-notes.md`：T012、T013、T014、T015，以及提取层。
