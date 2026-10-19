# T042 交互代理纯核心

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主编排）
- 分支：`feat/T042-interaction-broker`
- 依赖：T031（已合并）。M1a 第 5 波，松弛为 1；T043（会话 actor，关键路径）依赖它。

## 背景

agent 发来的权限请求和提问，由宿主的交互代理统一管理（设计 5.2.7、5.3.6）。它负责的事情：
- 入账：生成摘要，算出 `actionable_by`；
- 收到回答时依次检查；
- 处理撤回与失效；
- Claude 的多问题提问需要拆分与合并；
- 遇到不支持的 schema 时降级。

本任务把这些写成纯核心 `InteractionBroker`，套用 T022 的 `Core` 框架，不做 I/O，不读时钟。回答检查链里的资源级规则直接调用 T031 的 `policy::respond`。

## 范围

**做**（计划 T042 一行；设计 5.2.7、5.3.6）：
1. **入账**：
   - 收到 agent 的请求时，生成 `interaction.requested`：摘要用 T017 的 `request_digest`，并算出 `actionable_by`；
   - 记下 `entered`（`MonoTime`），给 400 ms 用。
2. **回答检查链**（5.2.7）：
   - 请求存在且为 `pending`，否则回 `conflict/already_resolved`（带 `resolved_by`），或者 `expired`；
   - 摘要一致；
   - principal 在 `actionable_by` 内；
   - 选项存在、`requires` 满足、档位规则、window 的输入证据、按键与选项相符、400 ms：这几项在本任务里调用 `policy::respond`。
3. **撤回与失效**：
   - Claude 用 `$/cancel_request` 撤回，状态 `withdrawn`；
   - 回合结束时按 `Settle.asks` 结束：`adapter_exited` 时状态为 `expired`，`turn_cancelled` 和 `turn_ended` 时状态为 `cancelled`；
   - 中断时 `TurnMachine` 发 `CancelAsks`；
   - Cancelling 期间新到的请求直接取消；
   - 宿主重启：`host_restarted`。
4. **Claude 多问题提问**：拆成逐题，再合并成一个回答（5.3.6）。
5. **`unsupported_schema`**：遇到不支持的提问 schema 时，按 5.3.6 降级。

**不做**：
- 选项对照表的数据（T053）；
- 事件落盘与 actor（T043）。

## 验收标准

- [ ] 新增 PROP-interaction（proptest，在虚拟时间上用 `CoreDriver` 驱动）：
  - 任何输入序列下，每个请求最终恰好解决一次；
  - 解决后的请求不再接受回答；
  - 撤回、失效、回合结束的原因和状态，与 5.3.11 的裁定一致；
  - 回答一律先过检查链，不会绕过。
- [ ] 新增 UNIT-question-mapping：
  - 多问题的拆分与合并；
  - `unsupported_schema` 的降级。
- [ ] 测试要覆盖拒绝的路径。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T042-interaction-broker/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **结束挂着的请求**（T048）：按 `Action::Settle { end, asks }` 里的 `asks` 结束回合里还挂着的请求：
  - `adapter_exited` 时状态为 `expired`；
  - `turn_cancelled` 和 `turn_ended` 时状态为 `cancelled`。
  - 中断时 `TurnMachine` 发 `CancelAsks`；Cancelling 期间新到的请求直接取消。
- **调用 `policy::respond`**（T031）：
  - 在检查完 pending、摘要和 `actionable_by` 之后调用；
  - 收到请求时用 `Clock` 记下 `entered`（`MonoTime`），并传入会话当前的档位。
- **请你提出方案，由编排者裁定**（T031）：5.3.6 写"所有选项都是 privileged 时 `actionable_by` 只有 `desktop_ui`"，这会把 test_harness 排除在外，而权限矩阵里 test_harness 对特权选项是 ✓。
- **设计里的相关裁定**：5.3.11 末尾"T014 实现时的裁定"中回合外请求的规则，以及"T040 实现时的裁定"中的 `turn_ended`。
- **纯核心框架**（T022）：
  - 适配循环写在 `pure/mod.rs` 的文档里；
  - 核心需要 ID 时用 `IdGen::new_id_at(now.wall_ms)`。
- **惯例**：
  - 用到 `CoreDriver` 的集成测试，声明 `required-features = ["test-harness"]`；
  - 改了协议类型就跑 `pnpm gen` 并提交生成物（T015）。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 并行注意

T027 在改 `store/`，T032 在做 `ledger/`。本任务放在 `orchestrator/`，跟 `TurnMachine` 放在一起；`orchestrator/mod.rs` 只加一行 `mod`。

## 规模

计划估为 M（400–900 行）。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**预计会超出约 1,500 行时，必须先停下来汇报**。

## 参考资料

- 设计：
  - `02-host.md` 5.2.7（交互代理）；
  - `03-protocol-v1.md` 5.3.6（交互请求模型）、5.3.11（T014、T017、T040、T031 的裁定）；
  - `08-security.md` 5.8.3；`07-desktop.md` 5.7.8（RP 规则）。
- T017 的 `interaction.rs`；T022 的 `pure/`；T031 的 `policy/`；T048 的 `orchestrator/turn.rs`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
