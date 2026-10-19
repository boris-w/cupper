# T048 会话纯核心 I-b：回合状态机

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主编排）。与 T040 是同一位执行者，`TurnMachine` 的实现是这位执行者在 T040 里写的。
- 分支：`feat/T048-turn-machine`。分支从 T040 原分支的末端（`42da246`）开出，已经包含 `TurnMachine` 的实现提交 `e03a930`。
- 依赖：T040（驱动接口与 `QueueCore`）。本任务在 T040 合并后开工，开工前编排者先把 main 合入本分支。T041（自发回合）、T043（会话 actor，关键路径）依赖它。

## 背景

T040 写完实现后，累计 1,473 行，补完测试预计约 1,950 行，因此拆成两个任务（计划 v1.3.3）：
- T040 收窄为驱动接口和 `QueueCore`；
- 本任务负责 `TurnMachine` 和四条性质测试。

## 范围

**做**（计划 T048 一行；设计 5.2.6、5.2.16、5.6.2）：
1. 审视并补完已有的 `TurnMachine`（`orchestrator/turn.rs`）：
   - 各个状态；
   - 3 s 准备超时、10 s 中断兜底；
   - 在途规则；
   - 先 close 再 resume；
   - 结束顺序：Settle → 结束快照 → `turn.completed`；
   - 出队与暂停。
2. 与 `QueueCore` 接线。
3. 按 5.3.11 末尾"T040 实现时的裁定"实现以下几条：
   - 回合结束时还挂着的请求，经 `Settle` 交给交互代理，标为 `cancelled`，原因是 `turn_ended`；
   - 暂停的解除规则；
   - agent 自己取消时不暂停队列；
   - Cancelling 状态下结果怎样记录；
   - 只有发过 prompt 的回合才拍结束快照。
4. 在 `cupper-protocol` 的 `ResolutionReason` 里附加 `turn_ended`：
   - 只做附加；
   - 线上取值写进 5.3.11 的取值表；
   - 有往返用例。

**不做**：
- Steer（T140）：`mode: steer` 按排队处理，回复带 `fallback: unsupported`；
- 自发回合（T041）：只留进入和退出的位置；
- 交互请求的跟踪（T042）；
- actor 与事件落盘（T043）。

## 验收标准

- [ ] 新增 PROP-queue-fifo：
  - 排队的消息按到达顺序出队；
  - 暂停期间不出队，恢复后接着出队；
  - 空闲时发来的新消息会解除暂停。
- [ ] 新增 PROP-interrupt-10s：
  - 中断后 10 s 内，prompt 返回 `cancelled`，回合记为 `cancelled`；
  - 超过 10 s，回合以 `host_timeout` 结束，迟到的结果按 `PromptTag` 识别后忽略。
- [ ] 新增 PROP-turn-single-prompt：任何输入序列下，同一会话同时在途的 prompt 都不超过一个。
- [ ] 新增 PROP-prompt-inflight-no-end：
  - prompt 在途时，回合不会结束，中断截止的情况除外；
  - 引用回合的事件都排在 `turn.completed` 之前。
- [ ] 全部测试用 `CoreDriver` 在虚拟时间上驱动，性质测试用 proptest。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T048-turn-machine/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项

- T040 的 `notes.md` 和汇报：
  - 驱动接口的形状；
  - 留给后续任务的事项：K6 自动 resume、门控暂停归 T044；从数据库恢复归 T043；Ending 期间的更新归 T041、T043；Cancelling 期间新到的请求归 T042。
- 纯核心框架：`pure/mod.rs` 的适配循环；核心需要 ID 时用 `IdGen::new_id_at(now.wall_ms)`。（T022）
- 用到 `ManualClock`、`CoreDriver` 的集成测试，声明 `required-features = ["test-harness"]`。（T022）

## 规模

已有的实现约 886 行（实现 751 行，测试 135 行）。补完四条 PROP、测试环境和场景测试，预计总量约 1,250–1,400 行，上限约 1,500 行。**每完成一条 PROP 就统计一次行数**（守则第 8 条）。预计会明显超出时，先停下来汇报。

## 参考资料

- 设计：
  - `02-host.md` 5.2.6（回合状态机、发送与队列）、5.2.16；
  - `06-agent-driver.md` 5.6.2、5.6.9；
  - `03-protocol-v1.md` 5.3.11 末尾的 T014、T040 裁定。
- T040 的任务书（含补充裁定）、`notes.md` 和汇报。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。`notes.md` 写在本目录，由执行者在任务分支上提交。
