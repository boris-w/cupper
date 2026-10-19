# T012 协议方法 I：握手、流、项目、agent、会话与交互

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T012-protocol-methods-core`
- 依赖：T017（已合并）。M1a 第 3 波；T015（schema 导出）依赖它，桌面链（T015 → T071）只差 1 的松弛。

## 背景

宿主协议的方法只定义类型，不含实现。参数和结果都是 Rust 类型，schema 从这些类型导出，再生成 TS（ADR-04）。T010 已经有 `Method`/`Notification` trait，要求每个方法声明 `SCOPE` 和 `IDEMPOTENCY`（`rpc.rs`、`schema.rs`）。T011、T017 已经定义好事件和领域类型。本任务定义 5.3.4 前五组的全部方法。

## 范围

**做**（计划 T012 一行；设计 5.3.2、5.3.3、5.3.4）：
1. **握手与生命周期**：
   - `host/hello`；
   - `initialize`：包括各 `auth` 变体、`role`、`features`、`limits`、`host_proof`，见 5.3.2；
   - `ping`、`client/presence`、`host/status`、`host/prepare_upgrade`、`host/exit`、`host/shutdown`、`ui/navigate`。
2. **流与内容**：
   - `stream/subscribe`、`stream/unsubscribe`、`stream/events`、`stream/snapshot`、`stream/reset`、`stream/ack`；
   - `history/page`、`blob/get`、`blob/put`。
3. **项目与 agent**：
   - `project/add`、`project/trust`、`project/trust_info`、`project/remove`；
   - `agents/capabilities`、`agents/refresh`、`agents/restart`。
4. **会话**：
   - `session/create`、`session/send`（包括 v1.5 的 `reply_to`）、`session/interrupt`、`session/queue_resume`、`session/queue_edit`（v1.4 的 `QueueOp`）；
   - `session/set_config`、`session/confirm_config`、`session/rename`、`session/archive`、`session/restore`、`session/mark_read`；
   - `session/get`、`session/capabilities`、`session/list_archived`；
   - `PromptContent` 及其各个块，包括 `text_blob` 和 `review_comment`。
5. **交互请求**：
   - `interaction/respond`，其中 `input.key` 可以取 v1.4 的 `digit`；
   - `interaction/get`。
6. **方法级声明**：
   - 每个方法都声明 `SCOPE` 和 `IDEMPOTENCY`，按 5.3.4 的表和 5.8.3 的权限矩阵；
   - 每个方法可能返回的错误码，要能在 schema 里查到，按 T010 的注解机制。
7. **M1b 和 M3 的方法也一并定义**，契约在 T016 冻结。里程碑要能被读到（注解或常量），供以后的门控和文档使用。

**不做**：
- 第六组"Git、工作区与文件"和第七组"设置、认证、诊断、测试"（T013）；
- 任何方法的实现；
- schema 导出与 TS 生成（T015）；
- 视图（T014，正在进行）。

## 验收标准

- [ ] 新增 UNIT-proto-methods-core：
  - 每个方法的参数和结果都能往返序列化（Rust → JSON → Rust 相等），用 5.3.4 里的典型取值；
  - 方法级的 scope 与 5.8.3 的权限矩阵逐格一致：用一张由测试写定的表比对，表里每一行都注明来自 5.8.3 的哪一格；
  - 需要 `command_id` 的方法一律是"命令"幂等，反之亦然。
- [ ] 宽容解码：`stream/events`、`stream/snapshot`、`history/page` 里的事件可以逐个解码，一个残缺事件不会让整条消息解析失败。要有用例。
- [ ] 协议里的整数不超过 2^53−1，导出给其他语言的产物不依赖 map 键顺序（`CLAUDE.md`）。
- [ ] 仍能编译到 `aarch64-apple-ios-sim`；在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T012-protocol-methods-core/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头（`unit_proto_methods_core_`）。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **`ErrorData.supported`**：目前放在 `details` 里，依赖版本区间类型。T012 以附加方式改成具名字段。（T010）
- **事件的宽容解码**：`stream/events`、`stream/snapshot`、`history/page` 的参数如果整体用派生的 `Deserialize`，一个残缺事件就会让整条通知解析失败。参数类型要让客户端能逐个事件调用 `Event::decode` 和 `EventBody::decode`（宽容模式）。（T011）
- **交互请求复用 T017 的类型**：
  - `interaction/respond.outcome` 复用 `InteractionOutcome`；
  - `interaction/get` 的状态复用 `InteractionStatus`（含 `pending`）；
  - 宿主核对摘要用 `request_digest`。它返回 `Result`，只在出现超过 2^53 的整数时失败。（T017）
- **惯例**：
  - `lib.rs` 只放 `mod` 声明；
  - 用 `ids` 里的前缀 ID 类型和 `TimestampMs` 这类带注解的类型；
  - 开放枚举用 `open_enum!`，开放联合用 `open_union!`；
  - 性质测试用 proptest。

## 并行注意

T014（协议视图）正在同一个 crate 里进行，改的是视图和同步相关的文件。你新增方法相关的文件（例如 `methods/` 目录），不要改 T014 可能动到的 `sync.rs`、`session.rs`、`index.rs`。必须改共享文件（`lib.rs` 的 `mod` 行、`rpc.rs`）时只做附加，并在汇报里列出，合并冲突由编排者处理。

## 规模

计划估为 M（400–900 行）。方法数量多（约 40 个），估计会偏大。**开工前先在 `notes.md` 里按方法组写行数估计**；动手后每完成一组统计一次（守则第 8 条）。预计会明显超出约 1,500 行的上限时，先停下来汇报，并给出拆分建议，例如按"握手与流"和"项目、会话与交互"分开。

## 参考资料

- 设计：
  - `03-protocol-v1.md` 5.3.2、5.3.3、5.3.4（前五组）、5.3.5、5.3.8、5.3.9；
  - `08-security.md` 5.8.2、5.8.3（权限矩阵）；
  - `02-host.md` 5.2.6（项目信任、配置项）。
- T010、T011、T017 的代码和汇报（`crates/cupper-protocol/`）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。其他物料（`notes.md` 等）由执行者在任务分支上提交。
