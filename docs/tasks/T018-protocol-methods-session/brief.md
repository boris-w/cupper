# T018 协议方法 I-b：项目、agent、会话与交互

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T018-protocol-methods-session`
- 依赖：T012（已合并）。本任务由 T012 拆分而来（计划 v1.3.2），M1a 第 3 波。桌面链 T018 → T015 → T071 → … 是零松弛；T031 也依赖它。

## 背景

T012 开工前估计约 2,250 行，于是拆成两个任务：
- T012：方法基础、握手与流，已合并；
- T018：本任务，即原计划中 T012 的第 3–5 组。

T012 搭好了这些东西：
- `method!` 宏，`MILESTONE`、`ERRORS` 常量；
- 方法目录；
- 跨组测试的框架。

T013 正在并行做 git、设置、认证、诊断和测试两组。

## 范围

**做**（计划 T018 一行；设计 5.3.4 第三至五组、5.2.6）：
1. **项目与 agent**：
   - `project/add`、`project/trust`、`project/trust_info`、`project/remove`；
   - `agents/capabilities`、`agents/refresh`、`agents/restart`。
2. **会话**：
   - `session/create`、`session/send`（含 v1.5 的 `reply_to`）、`session/interrupt`、`session/queue_resume`、`session/queue_edit`（v1.4 的 `QueueOp`）；
   - `session/set_config`、`session/confirm_config`、`session/rename`、`session/archive`、`session/restore`、`session/mark_read`；
   - `session/get`、`session/capabilities`、`session/list_archived`；
   - `PromptContent` 已在 T011 定义，这里直接复用。
3. **交互请求**：
   - `interaction/respond`：`input.key` 的取值包括 v1.4 的 `digit`，`digit` 取 1–9；
   - `interaction/get`。
4. **5.3.11 末尾"T012 开工前的裁定"里的取值**：
   - `findings` 条目：`{kind, path, target?, outside_workspace, detail}`，`kind` 是开放枚举；
   - `diff_from_trusted`：按文件列出 `{path, change}`，`change` 复用 `DiffChange`；
   - `fallback_reason`：`unsupported`、`turn_mismatch`、`not_prompting`、`prompt_required`、`timeout`、`agent_error`；
   - 补充两个错误码：`project/trust_info` 加 `unavailable/scan_failed`，`session/create` 加 `forbidden/scope`；
   - `session/queue_edit` 的 scope 按 5.8.3 中 `session/send` 那一行。
5. **权限矩阵加一种格子**："scope 允许，但按 SEC-9 由策略拒绝"，用在 `project/add` 等行的 remote_device 一列（5.8.3）。

**不做**：
- git、设置、认证、诊断、测试（T013，并行进行中）；
- 任何方法的实现；
- schema 导出（T015）。

## 验收标准

- [ ] 新增 UNIT-proto-methods-core-session：
  - 本任务每个方法的参数和结果都能往返序列化，取值用 5.3.4 的典型值；
  - 方法级 scope 与 5.8.3 的权限矩阵逐格一致（往 `MATRIX` 加行，每行注明出处）；
  - SEC-9 格子有用例；
  - `interaction/respond.input` 的 `digit` 有用例。
- [ ] T012 的跨组测试（`catalog.rs`）在加入本任务的方法后仍然通过。
- [ ] 仍能编译到 `aarch64-apple-ios-sim`。在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T018-protocol-methods-session/ci/` 通过，摘要随任务提交。
- [ ] 本任务新增的测试，名字以 `unit_proto_methods_core_session_` 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **接入方法的步骤**：详见 `docs/tasks/T012-protocol-methods-core/notes.md` 中"给 T018、T013 的说明"一节。
  - 新建组文件，用 `method!` 声明，并提供 `register`；
  - `methods/mod.rs` 加一行 `mod`，`catalog()` 加一行；
  - 在组文件的测试模块里写 `samples()`，并加进 `catalog.rs` 的覆盖测试；
  - 按 5.8.3 往 `MATRIX` 加行；
  - 参数必须是具名结构体；
  - 类型按完整的方法名命名，例如 `session/send` 对应 `SessionSend`。（T012）
- **用例编号的归属**（编排者裁定）：
  - `catalog.rs` 里的跨组测试属于 T012 的 UNIT-proto-methods-core-stream，不改名，它们会自动检查你加的方法。
  - 你另写以 `unit_proto_methods_core_session_` 开头的测试，用共享的测试辅助函数检查本组的样例和矩阵行。
- **交互请求复用 T017 的类型**：
  - `interaction/respond.outcome` 用 `InteractionOutcome`；
  - `interaction/get` 的状态用 `InteractionStatus`（含 `pending`）；
  - 宿主核对摘要用 `request_digest`。（T017）
- **`session/archive.blockers` 用的枚举由 T013 定义**，取值对应 ADR-10 清理规则的 7 条，`worktree/check` 也用它。
  - 两个任务并行进行，所以 **`session/archive` 放到最后做**。
  - 做到它时，如果 T013 还没合并，就停下来汇报"其余已就绪，等 T013"。编排者在 T013 合并后把 main 合入你的分支，再通知你继续。
- **惯例**：
  - `lib.rs` 只放 `mod` 声明；
  - 用 `ids` 里的前缀 ID 类型；
  - 开放枚举用 `open_enum!`，开放联合用 `open_union!`；
  - 整数不超过 2^53−1。

## 并行注意

T013 同时在往 `methods/mod.rs`、`catalog()`、`catalog.rs` 的覆盖测试和 `MATRIX` 里加行。你在这些地方只做附加，合并冲突由编排者处理。其余文件放在你自己的组文件里。

## 规模

T012 的执行者估计这三组约 1,090 行（代码约 820 行，测试约 270 行）。上限约 1,500 行。**开工前先在 `notes.md` 里按组写行数估计。每写完一组就统计一次实际行数**，不要等全部写完（守则第 8 条；T012 在这一点上没做到）。预计会明显超出时，先停下来汇报。

## 参考资料

- 设计：
  - `03-protocol-v1.md` 5.3.4 第三至五组、5.3.11 末尾"T012 开工前的裁定"；
  - `08-security.md` 5.8.2、5.8.3；
  - `02-host.md` 5.2.6（项目信任、配置项、发送与队列）；
  - PRD 的 SEC-9；ADR-10（清理规则）。
- T012 的代码、`notes.md` 和汇报；T011、T017 的领域类型。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料（`notes.md` 等）由执行者在任务分支上提交。
