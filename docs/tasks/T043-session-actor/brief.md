# T043 会话 actor I：创建、发送、中断与持久化

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主编排）
- 分支：`feat/T043-session-actor`
- 依赖：T033、T041、T042、T058，均已合并。**在关键路径上**：T033 → T043 → T044 → T046 → T090 → …

## 背景

会话的各个纯核心都已就绪：
- 回合机 `TurnMachine`（T048）、队列 `QueueCore`（T040）；
- 合并器 `Coalescer`、分组（T041）；
- 交互代理 `InteractionBroker`（T042）；
- 自发回合裁判 `AutonomousJudge`（T058）；
- 组合测试核心 `Session`，在 `orchestrator/session_props.rs` 里，示范了它们之间怎么接线。

周边的部件也已经有了：
- 存储的流与写者（T026）、blob（T027）、账本（T032）、策略（T031）；
- 网关分发，以及等待实现的 `SessionPort`（T033）；
- 驱动接口 `driver::api`（T040）。

本任务做第一个真正的 actor：
- `SessionManager`；
- `SessionActor`：一个适配层，把上面这些纯核心接到邮箱、时钟、存储和驱动上；
- 实现 `SessionPort`，让网关能把会话命令投递进来。

驱动的真实实现归 T055。本任务用进程内的驱动替身测试。

## 范围

**做**（计划 T043 一行；设计 5.2.6、5.5.2（本地模式）、5.5.3、5.5.5）：
1. **`SessionManager`**：
   - 会话句柄表；
   - 按需载入会话；
   - 实现 `SessionPort`：`deliver` 按 `params.session_id` 投进会话邮箱，`facts(&Confirmable)` 给出事实；
   - 用 `Routes::sessions(port)` 登记 M1a 的会话方法；接上之后，从 UNIT-unsupported-set 的允许表里删掉相应行。
2. **`SessionActor` 适配层**：
   - 按 `pure/mod.rs` 文档里的适配循环，把纯核心接到邮箱、`Clock` 定时器、写者和驱动上；
   - 接线以 `session_props.rs` 的 `Session` 为准，见交接事项；
   - 给 `TimerTable::next_deadline` 维持一个 `Clock` 定时器的小助手，加在 `pure` 里（T022 的交接）。
3. **方法**：
   - `session/create`：本地模式，第一条消息随受理一起落盘；
   - `session/send`：
     - 回合进行中时排队；
     - `queue.updated` 的条目带完整的 `content`；
     - `reply_to` 先核对，再写进用户消息和排队条目（v1.5）；
     - 队列满时回 `too_large/queue_full`；
   - `session/interrupt`、`session/queue_resume`、`session/get`、`session/capabilities`。
   - 命令用账本的 `Ticket`：受理后，结果和首批事件放进同一个 `Batch`。
4. **交互的接线**：`interaction/respond`、`interaction/get` 走交互代理；`Respond.tier` 传会话当前的档位。
5. **回合事件与检查点**：
   - 回合结束时，回合快照、检查点、裁剪在同一个事务里提交，由 T026 的写者完成；
   - 被放弃的条目补上 `item.completed`；
   - `turn.completed` 由合并器写出。
6. **快照端口**：git 快照归 T061。在那之前，回合快照记为 `skipped`，与非 git 项目相同（计划 1.3）。
7. **准备阶段的恢复**：载入会话时：
   - 补写 T037 启动清理留下的事件：回合 `interrupted`、交互 `expired`；
   - 从数据库恢复回合序号、队列和 agent 会话；
   - 用 `Input::Ended` 把已经结束的请求载入交互代理；
   - 检查点因 `view_v` 不符而作废时，重发 `session.configured`、`session.capabilities` 和 `queue.updated`。
8. **超长工具输出**：超过内联上限时转存 blob（`BlobStore`、`add_ref`），标 `truncated`，每秒更新一次尾部。
9. **时长**：各时长接 `test_config.durations`。

**不做**：
- 档位、信任门控、崩溃与晚到输出的后续处理（T044）；
- 驱动的真实实现（T055）；
- worktree 模式（M1b）。

## 验收标准

- [ ] 新增 IT-orch-lifecycle，进程内运行，用驱动替身，覆盖：
  - 创建；
  - 发送，以及回合进行中的排队；
  - 中断；
  - 恢复队列；
  - `reply_to` 的核对；
  - 重启后载入会话的恢复；
  - 交互请求的回答。
- [ ] 经网关分发进来的会话命令，能走通策略 → 账本 → `SessionPort` → actor → 写者这一整条路径。
- [ ] 全部用 `ManualClock` 驱动，不睡眠。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T043-session-actor/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。IT-orch-lifecycle 已在 `tests/ids.toml` 的 T043 段里登记。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

以下各条都写在 handoff-notes 里，标注了来源，这里列出要点：

- **接线**（T058、T041）：
  - 照 `session_props.rs` 的组合核心 `Session` 接线：驱动事件依次交给裁判、回合机，如果是更新，最后交给合并器；
  - 回合机的 `TurnSignal` 立即交给合并器；这一步结束后再交给裁判，裁判的 `Start`、`End`、`Hold` 作为回合机的输入送回去；
  - `turn.completed` 由合并器写出，actor 收到 `Action::Complete` 时自己不写；
  - 请求先送裁判（`Asked`），再送合并器（`Interject`），每次解决之后送 `Resolved`；
  - 因 `Asked` 而开出的自发回合，要在交互代理写出请求之前就开好；
  - 构造合并器时，传入能力里的 `interrupt_stops_commands`；
  - 回合收尾期间到达的请求，放进随后开的自发回合，裁判会等它解决后才结束这个回合。接线时请确认这一点，并写进汇报。
- **回合机的输出**（T048）：
  - `Settle` 之后补齐被放弃条目的 `item.completed`：`abandoned`，Codex 还要加 `may_still_run`；
  - `Complete` 时由合并器补上 `final_item_ids`；
  - 执行 `Action::Attach` 时补上工作区、配置和信任状态，先调用 `policy::attach`；
  - `QueueFull` 映射为 `too_large/queue_full`。
- **交互**（T042）：
  - 重启后用 `Input::Ended` 载入已经结束的请求；
  - 写 `interaction.requested` 之前，先给合并器送 `Input::Interject`，并通知索引；索引端口要等 T034，这里留好接口；
  - CLI、test_harness 完成的回答，写审计 `interaction.responded`。
- **策略**（T031）：`session/create` 的档位，用能力里的 `risk` 换算成 `Tier` 后调用 `policy::tier_change`，映射里没有的模式传 `FullAccess`；`config_unconfirmed` 的判定用 `policy::ceiling`。
- **账本**（T032）：
  - 受理后调用 `ticket.complete(Ok(result))`，把 `Box::new(completion.row())` 放进 `Batch::with`；
  - `append` 成功后调用 `completion.committed()`；
  - 前置条件不满足时调用 `reject(err)`。
- **存储**（T026、T027）：
  - `Appended::rejected` 不为空，说明写者违反了视图规则，要记日志；test-harness 构建里让用例失败；
  - 事件引用 blob 时，在同一个写操作里调用 `add_ref`，返回 `false` 时让客户端重新上传；
  - 检查点作废时要重发，见范围第 7 条。
- **启动清理**（T037 的裁定，`02-host.md` 5.2.13 末尾）：T037 只改库，会话流里对应的事件由本任务在载入时补写。
- **不变式**（T014，5.3.11 末尾）：
  - 引用某个回合的事件，都写在这个回合的 `turn.completed` 之前；
  - 交互请求一律属于某个回合。
- **惯例**：
  - 时间只经 `Clock` 获取，定时只经 `Clock`；
  - 每条事件流只有一个写者；
  - SQLite 只在写线程或读池上运行；
  - 关掉 fd 之后要判断对端是否关闭时，以读到 EOF 为准；
  - 改了协议类型，就跑 `pnpm gen` 并提交生成物；
  - 新增用例编号时，在 `tests/ids.toml` 的 T043 段里登记。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 并行注意

T037（宿主组装）正在并行进行，它会调整 `Routes::start` 的签名，并为会话端口留接入点。本任务实现 `SessionPort`，并在测试里组装；真正接进宿主 `main`，等 T037 合并之后再做，如果冲突由编排者处理。

## 规模

计划估为 L，上限约 1,500 行。这是第一个 actor，接线面最广。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**；计划 1.1 的执行经验是实现按草估的 1.7 倍、组合测试按 1.5 倍计。推算会超出上限时，开工前就停下来汇报，并给出拆分建议，例如把"actor 适配层与创建、发送、中断"和"恢复、交互接线与超长输出"分开。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**预计会超出上限时，必须先停下来汇报，不要继续写**。

## 参考资料

- 设计：
  - `02-host.md` 5.2.6（会话编排）、5.2.7、5.2.8、5.2.13（T037 的裁定）、5.2.16；
  - `05-flows.md` 5.5.2、5.5.3、5.5.5；
  - `03-protocol-v1.md` 5.3.4（会话方法）、5.3.11 末尾的各项裁定。
- `orchestrator/`（T040、T041、T042、T048、T058）、`store/`、`ledger/`、`policy/`、`gateway/` 的代码和 `notes.md`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
