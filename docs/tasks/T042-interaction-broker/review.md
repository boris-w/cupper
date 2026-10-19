# T042 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：编排者把 main 合入分支（`8d509e5`），没有冲突。在任务 worktree 里跑 `cargo xtask ci pr --record`：rust 947、protocol 176、ts 232、license 1，全部通过，摘要为 `ci/pr-8d509e5d3848.json`。
- 核对：
  - 没有 `unsafe`，没有个人信息；
  - 纯核心不做 I/O、不 await、不读时钟，也不设定时器。
- 审阅：
  - **检查链**：按 5.2.7 的顺序，摘要、`actionable_by`、policy、答案形状逐项检查。PROP-interaction 用测试里独立实现的检查链做对照，逐个动作比较，拒绝的原因和顺序也必须一致。
  - **变异检查**：13 处变异全部被测出，包括调换检查顺序，以及跳过摘要、`actionable_by` 或 policy。
  - **`DriverEvent::Ask` 改为带 `AskRequest`**：交互代理需要原始表单，才能把回答拆回 `question_N_custom`。接受，符合"接口由使用方定义"。
  - **`Ask.turn_id` 改为必填**，落实 T014 的裁定。
  - **应答约定**：每个 ask 恰好收到一次 `Answer`，除非它的实例已经退出。撤回之后也回 `Cancelled`，运行时据此放掉应答句柄。
- 规模：1,494 行（代码约 650 行，测试约 840 行）。执行者在 1,536 行时发现超限，只改写法、没有删检查，压回上限以内，并在 `notes.md` 里记下了过程。
- 编排者裁定：
  - **`actionable_by` 总带上 `test_harness`**：全是特权选项时为 `[desktop_ui, test_harness]`，否则为 `[desktop_ui, cli, test_harness]`。理由：与 5.8.3 的矩阵一致，检查链里没有隐藏的例外，取值不随构建变化。5.3.6 已相应改写。
  - **5.2.7 撤回的原因**：设计里写的是 `agent_withdrew`，改为协议里的 `withdrawn`。
  - **`interaction/respond` 的 `x-errors` 缺 `not_found`**：会话里不存在的请求 ID 目前回 `invalid_params`，而 `interaction/get` 回 `not_found`。交给 T016 在冻结前补上，补的时候跑 `pnpm gen`。
  - **Codex 的"其他"和备注**（`_meta.codex.isOther`、`<id>_note`）要不要合成一题，交给 T110。
- 转交事项已写进 `handoff-notes.md`：T043、T053/T055、T110、T016。
