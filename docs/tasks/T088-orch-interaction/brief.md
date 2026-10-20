# T088 会话 actor I-b：交互接线

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主编排）。T043 的执行者接手，在 T058 修复第 1 轮之后
- 分支：`feat/T088-orch-interaction`
- 依赖：T043（已合并）、T058 修复第 1 轮（要先合并）。T044 依赖本任务

## 背景

T043 按计划 v1.3.11 收窄为"骨架与命令面"，交互接线拆成本任务（见 `docs/tasks/T043-session-actor/brief.md` 末尾的补充裁定）。现在组合核心 `SessionCore` 里用一个请求计数（`Input::Asked`、`Input::Resolved`，在 Settle 时结清）占着交互代理的位置；agent 发来的请求（`DriverEvent::Ask`）没人应答，actor 也不处理 `CancelAsks`、`Settle`。

交互代理 `InteractionBroker`（T042）已经是纯核心，本任务把它接进组合核心和 actor（设计 5.2.7）。

## 范围

**做**（计划 T088 一行；设计 5.2.7、5.3.6、5.3.4 的交互方法；T043 的 `notes.md`"给其他任务的接入点"）：
1. **把 `InteractionBroker` 接进 `SessionCore`**，替换请求计数：
   - 顺序按 T058 的组合约定：请求先送裁判（`Asked`），再送合并器（`Interject`，刷出缓冲的文本并结束活动组），然后由交互代理写 `interaction.requested`；每次解决之后送裁判 `Resolved`；
   - 因 `Asked` 而开出的自发回合，要在交互代理写出请求之前开好，请求才带上 `turn_id`；
   - 回合收尾期间到达的请求，放进随后开的自发回合（T058 的交接，接线时请确认）；
   - `CancelAsks`、`Settle`：由交互代理把挂着的请求标为 `cancelled`，对 ACP 应答 `Cancelled`，之后才拍结束快照、写 `turn.completed`（5.3.11"T014 实现时的裁定"：回合在它的请求全部解决之前不结束）。
2. **actor 接线**：`DriverEvent::Ask` 交给核心；交互代理要应答 ACP 时，经 `AgentPort` 发出（应答句柄的形状按 `driver/api.rs` 现有的定义，缺什么就补在 `DriverCommand` 里）；`Action::Machine(CancelAsks | Settle)` 从这里接。
3. **`interaction/respond`、`interaction/get`**：按 5.2.7 的检查顺序（请求存在且 `pending`、摘要一致、principal 在 `actionable_by` 内、选项存在且 `requires` 满足、带 `mode_after` 的选项按档位规则、window 角色的 `input` 证据与 400 ms、提问的答案符合字段定义）。`Respond.tier` 传会话当前的档位。登记进 `Routes::sessions`，并从 UNIT-unsupported-set 的允许表里删掉相应的行。
4. **`interactions` 表的行**：加在 `Rows::of` 里，与 `interaction.requested`、`interaction.resolved` 同一事务。
5. **`session/send.reply_to` 的核对**（5.3.4）：核对通过后写进用户消息和排队条目（`SendRequest.reply_to`、`queue_entries.reply_to`，列和行的写法 T043 已经有了）。
6. **`facts.option`**：`policy::Facts` 里的 `option`，由会话端口按请求和选项给出。
7. **通知索引的接口**：交互请求出现和解决时通知索引。索引 actor 由 T034 做，本任务定义接口并在 actor 里调用；T034 之前用空实现。
8. **`interaction.responded` 审计**（T043 开工前的裁定）：由 CLI 或 test_harness 完成的回答，新增一种带 `ConnId` 的网关审计记录，由 actor 发进现有的审计 channel。落盘归 T059。

**编排者裁定（T042 遗留）**：`interaction/respond` 的 `x-errors` 加上 `not_found`，请求 ID 在会话里不存在时回 `not_found`，与 `interaction/get` 一致。改协议后运行 `pnpm gen`，生成物随提交。授权本任务同时改 `03-protocol-v1.md` 5.3.4 表里 `interaction/respond` 一行的错误码。

**不做**：
- 宿主重启后用 `Input::Ended` 载入已结束的请求、按需载入（T089）；
- `Ask.item_id` 按 `toolCallId` 找条目：`DriverEvent::Ask` 目前不带 `toolCallId`，由 T053、T055 补上；
- 审计落盘（T059）；索引 actor（T034）；
- 档位核对与 `config_unconfirmed`（T044）。

## 验收标准

- [ ] 新增 IT-orch-interaction，经 `Dispatch` 走策略 → 账本 → 会话端口 → actor → 写者的整条路径，至少覆盖：
  - 回合内的审批：请求写出时带 `turn_id`，回答之后 ACP 收到对应选项，`interaction.resolved` 带 `resolved_by`；
  - 回合外的请求开出自发回合，回合在请求解决之前不结束；
  - 先到先得：第二个回答得到 `conflict/already_resolved`，带 `resolved_by`；
  - 摘要不一致、principal 不在 `actionable_by` 内、选项的 `requires` 不满足、window 角色不足 400 ms，各自被拒；
  - 中断回合时挂着的请求变为 `cancelled`，ACP 收到 `Cancelled`，之后才写 `turn.completed`；
  - Claude 发 `$/cancel_request` 撤回：`cancelled`（`reason: withdrawn`）；
  - `reply_to` 的核对：指向有效请求时写进用户消息和排队条目，无效时被拒；
  - `interaction/respond` 对不存在的请求回 `not_found`；
  - CLI 完成的回答发出了 `interaction.responded` 审计记录。
- [ ] 两条组合性质（PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable）在接上交互代理之后照常通过；性质测试改用真实的交互代理，不再用计数替身。
- [ ] 不睡眠：全部用 `ManualClock`。
- [ ] 测试名以用例编号开头；IT-orch-interaction 已在 `tests/ids.toml` 的 T088 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T088-orch-interaction/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T043：接线点是 `SessionCore` 的请求计数和 actor 的 `Action::Machine(_)` 分支；`reply_to` 列和行的写法已有；补 `interactions` 的行和 `facts.option`。
- T058：组合约定（裁判、回合机、合并器的顺序，`Asked`、`Interject`、`Resolved`）。
- T042：写 `interaction.requested` 之前先刷出 normalizer 缓冲的文本；`Respond.tier` 传会话当前档位；`$/cancel_request` 之后交互代理仍会发一次 `Answer { Cancelled }`，运行时据此放掉应答句柄。
- T014 审阅：交互请求一律属于某个回合，没有进行中的回合时按 K9 开 `autonomous` 回合。

## 注意

- 代码约束：ACP 回调里只做 channel 发送；时间只经 `Clock`；每条事件流只有一个写者（会话流的写者是 actor）。
- `actionable_by` 始终包含 `test_harness`（5.3.6，T042 的裁定）。
- 改了 `cupper-protocol` 之后跑 `pnpm gen`，TS 侧的类型和 zod 随提交。

## 规模

计划估为 M。T043 开工前估这一块约 590 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 1,000 行时，必须先停下来汇报**。

## 参考资料

- 设计：`02-host.md` 5.2.7、5.2.6 末尾"T043 开工前的裁定"；`03-protocol-v1.md` 5.3.4、5.3.6、5.3.11 末尾的各项裁定；`08-security.md` 5.8.6（审计）。
- `docs/tasks/T042-interaction-broker/notes.md`、`docs/tasks/T043-session-actor/notes.md`、`docs/tasks/T058-autonomous-judge/`。
- `crates/cupper-host/src/orchestrator/interaction/`、`orchestrator/session/`。

## 工作方式

T058 修复第 1 轮合并之后，在原来的 T043 worktree 里从最新的 main 开新分支：`git checkout -b feat/T088-orch-interaction main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
