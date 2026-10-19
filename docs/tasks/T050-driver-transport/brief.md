# T050 驱动层 I：传输、线路分接与 ACP 连接

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，agent 驱动）
- 分支：`feat/T050-driver-transport`
- 依赖：T003（K1 已通过）、T022（均已合并）。M1a 第 2 波；T053、T055 依赖它。

## 背景

宿主通过 ACP Rust SDK（`agent-client-protocol = "=3.3.0"`）驱动适配器（ADR-07）。T003 的技术验证（K1）确认了以下几点：
- 扩展消息全部由 SDK 支持：`_session/steering`、`_auth/status_update`、`$/cancel_request`；
- 线路分接层只负责三件事：给原始帧编号并记录、做结构化脱敏、处理 K19 的枚举容错；
- SDK 有两个坑：K27（未处理的请求永远收不到响应）和 K28（丢弃在途请求会取消回合）。

本任务实现传输、分接层和 ACP 连接。提取层（T053）和 AgentRuntime（T055）另有任务。

## 范围

**做**（计划 T050 一行；设计 5.6.1、5.6.9、5.6.10，v1.6）：
- **`AcpTransport`**：子进程 stdio 和内存双工两种实现。子进程由 `cupper_sys::spawn` 拉起；本任务只需要接口和内存实现能用于测试。
- **线路分接**：
  - 给原始帧编号；
  - 做结构化脱敏后写入原始帧，脱敏范围包括 `_auth/status_update` 里的账号字段；
  - K19 改写：SDK 的封闭枚举遇到未知值时改写成 SDK 能解析的值，下发给客户端的永远是原值；
  - 不再路由扩展消息。
- **ACP 连接任务**：
  - 回调里只做 channel 发送（`CLAUDE.md`）；
  - `fs`、`terminal` 声明为不支持；
  - `clientInfo.name` 固定为 `cupper`。
- **扩展消息走 SDK**：
  - `_session/steering` 用 SDK 的派生宏声明成请求类型；
  - `_auth/status_update` 用类型化的通知处理器接收；
  - `$/cancel_request` 由 SDK 标记 `Responder::cancellation()`。
- **SDK 的两条约束**：
  - K27：在处理器链最后注册一个无类型的 `on_receive_dispatch`，未知请求回 method not found，未知通知直接丢弃，响应照常转发；
  - K28：放弃等待时调用 `detach()`，不能丢弃 future，否则会自动发出 `$/cancel_request`。

**不做**：提取层（T053）；CLI 发现（T054）；AgentRuntime 与路由（T055）。

## 验收标准

- [ ] 新增 UNIT-tap：编号；结构化脱敏（含账号字段）；K19 改写后，下发的仍是原值。
- [ ] 新增 UNIT-transport-memory：内存双工的收发与关闭。
- [ ] 新增 UNIT-acp-unhandled：沿用 T003 的 `tests/unhandled.rs`（`spikes/T003-tech-validation/q9-probe/`），断言未知请求回 method not found、未知通知被丢弃。
- [ ] 新增 UNIT-acp-detach：放弃等待后不会发出 `$/cancel_request`。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T050-driver-transport/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 注意

- ACP SDK 锁精确版本 `=3.3.0`（ADR-02）。依赖写在 `cupper-host` 自己的 `Cargo.toml` 里。其他依赖有第二个使用方时，可以提到 `[workspace.dependencies]`，并在汇报里说明。
- 在内存双工上测试时，对端可以用一个最小的测试替身 agent。假 agent（T051）还没合并，不要依赖它。
- 不运行真实的 Claude 或 Codex CLI。
- 规模上限约 1,500 行。每完成一个模块统计一次行数（守则第 8 条），预计会超出时先停下来汇报。

## 参考资料

- 设计：`06-agent-driver.md` 5.6.1、5.6.9（K19、K27、K28）、5.6.10；`02-host.md` 5.2.16（`AcpTransport`）。
- T003：`docs/tasks/T003-tech-validation/report.md`、`spikes/T003-tech-validation/q9-probe/`。
- ADR-07（含 v1.6 的修订）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。
