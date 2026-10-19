# T040 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：
  - 执行者写完实现时，累计已有 1,473 行，按守则第 8 条停工。编排者裁定拆分（见 `brief.md` 末尾的补充裁定）。
  - 本分支重置到 `13bf686`，挑回 notes 提交，再合入 main。`TurnMachine` 留在 `feat/T048-turn-machine`。
- 核对：
  - 执行者在干净的 worktree 里跑了 `cargo xtask ci pr --record`（`d2b8951`）：rust 530、ts 64、license 1，全部通过，摘要为 `ci/pr-d2b895179b2a.json`。分支已经包含 main 的最新提交。
  - 没有 `unsafe`，没有个人信息；`driver::api` 和 `orchestrator/` 里没有 ACP 类型。
- 代码审阅：
  - `driver::api` 全是纯数据：
    - `PromptTag` 用来认出迟到的结果；
    - `AskId` 让 ACP 的 responder 留在驱动实现里；
    - `AgentError` 分为 8 类，并映射到 `FailureCode`。
  - `QueueCore` 不实现 `Core`，作为回合状态机同步调用的组件。它没有定时器，唯一的调用方是 `TurnMachine`，这样组合最直接。接受。
  - `queue_edit` 先核对版本，再整体生效或整体不生效；失败时队列不变，用例覆盖了这一点。
- 规模：652 行（实现 448 行，测试 204 行），属于 M。
- **编排者裁定（队列满）**：排队消息已到上限（50 条）时，`session/send` 和 `queue_edit` 返回 `too_large/queue_full`。
  - `session/send` 的可能错误本来就有 `too_large`，50 条也是 5.3.9 限额表里的一项。
  - 已写进 5.3.11，并通知 T018 在 `session/send` 的 `ERRORS` 里加上 `queue_full` 这个 reason。
- 转交事项已写进 `docs/tasks/handoff-notes.md`：
  - T048：重新登记两个时长；`turn_ended`；`#[expect(large_enum_variant)]`；
  - T043/T048：把队列相关的类型映射到协议的错误码；
  - T055：`driver::api` 的实现要点。
