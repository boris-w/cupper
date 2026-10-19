# T043 汇报

- 执行者：子 agent（Rust 工程师，宿主编排）
- 分支：`feat/T043-session-actor`
- 完成日期：2026-10-19

收窄后的范围（A）全部完成，IT-orch-lifecycle 的 4 个用例通过，`cargo xtask ci pr` 在干净的 worktree 里通过（rust 1,093 个用例全过）。改动新增 2,028 行，在 2,100 的上限以内。有两件事在 T088 之前要知道：agent 发来的审批请求暂时没人应答；`send` 的 `reply_to` 暂时不写入。

## 做了什么

- **`pure::Alarm`**（T022 的交接）：
  - 给 `TimerTable::next_deadline` 维持一个 `Clock::timer_at`，截止时间变了才重设；
  - `rung()` 中途被取消也安全，没有定时器时一直挂起。
- **组合核心 `orchestrator/session/core.rs`**：
  - 把 `session_props.rs` 的 `Session` 原样提成正式代码 `SessionCore`，性质测试改用它，两条 PROP 照常通过；
  - 回合机加了 `turn_id()`，合并器加了 `set_stops_commands()`（裁定第 3 条）。
- **`driver::api::AgentPort`**：
  - `attach(Attach, 事件发送端)` 返回回复的 future，`send(DriverCommand)` 发不需要回复的命令；
  - T055 实现，测试用进程内替身。
- **`SessionActor`（`actor.rs`）**：
  - 循环等四样东西：邮箱、agent 事件、attach 结果、`Alarm`；
  - 每个输入是核心的一步，一步的事件、它改动的行、命令的账本记录在同一个事务里提交（`Batch::with`）；提交之后才回复、发驱动命令、起 attach 的 task；
  - 没有事件、只有行的一步（例如 resume 回来的 attach），经写线程单独提交；
  - 前置条件不满足时用 `Ticket::reject`：`QueueFull` → `too_large/queue_full`，`TurnMismatch` → `conflict/turn_mismatch`；
  - `Appended::rejected` 不为空时，测试和 test-harness 构建里 panic，其他构建写 stderr；
  - 快照端口一律回 `skipped`，不带 `reason`（裁定第 2 条）；
  - attach 带回的能力或配置变了，才写 `session.capabilities`、`session.configured`；`agent_session_id` 写进库。
- **行（`rows.rs`）**：
  - `turns`：开始、结束、快照状态；
  - `sessions`：`state`，第一个回合开始时清空 `initial_prompt_json`，`last_turn_end_seq`、`config_json`、`agent_session_id`、`queue_version`、`queue_paused`；
  - `queue_entries`：随 `queue.updated` 整体替换，带 `command_id` 和新加的 `reply_to` 列。按授权同时改了 `0001` 和 5.4.2 的 SQL。
- **`SessionManager`（`manager.rs`）**：
  - 句柄表；`SessionPort::deliver` 按 `session_id` 同步投进邮箱；
  - `facts` 给出 `mode_tier`（来自能力缓存）和 `running_turns`；
  - 在静止屏障里登记 `session`，每个 actor 运行期间持有 `Participant`。
- **`session/create`（本地模式）**：
  - 检查：worktree 回 `unsupported`；agent 不存在回 `unavailable`；项目不存在回 `not_found`；信任门控走 `policy::attach`；模式按缓存里的 `risk` 换成档位，换不出按 `full_access`，再 `policy::tier_change`；
  - 一个事务写 `sessions` 行（含第一条消息、标题、`mode_set_by`）、`session.configured`、`workspace.ready` 和账本行；
  - 提交之后才登记句柄、启动 actor，actor 的第一步就是这条消息。
- **其余方法**：`send`（回合进行中排队，`queue.updated` 的条目带完整的 `content`）、`interrupt`、`queue_resume`、`get`（`sessions` 行加视图算出 5.2.9 的状态）、`capabilities`。
- **测试辅助**：网关的 `Peer` 从 `dispatch/tests.rs` 挪到 `gateway/testkit.rs`，只改了可见性；`store::testutil` 对 crate 内的测试可见。

## 关键取舍

- **交互代理的位置**：`SessionCore` 保留了性质测试里的请求计数（`Input::Asked`、`Input::Resolved`，在 Settle 时结清），作为交互代理的接入点；T088 换成 `InteractionBroker`。这样性质测试的语义一点没变。
- **排队消息的 `command_id`**：`queue_entries.command_id` 不能为空，所以 actor 记下每条排队消息来自哪个命令，出队后删掉。
- **控制规模**：第一稿合计超过了 2,200 行的硬停止线（actor 552、rows 251、测试 609 行）。我在提交之前压缩：行的写法改成"语句加参数"的表，测试断言写单行。没有删减任何检查。

## 验证方式

- 验收标准逐条：
  - **IT-orch-lifecycle**：4 个用例，全部经 `Dispatch` 走策略 → 账本 → `SessionPort` → actor → 写者：
    - **创建**：第一条消息成为回合，事件序列逐个核对；同一命令重放回 `replayed`；行、回合快照、检查点与事件同一事务提交；`get`、`capabilities` 的结果核对。
    - **排队**：回合进行中排队；排满 50 条后回 `queue_full`；回合结束后按顺序出队，出队的用户消息带 `queue_id`。
    - **中断**：prompt 回 `cancelled` 和 10 s 截止两种结局；`turn_mismatch`；队列暂停后 `queue_resume`；截止之后先 `Close` 再 `Resume`（K1）。
    - **创建时的检查**：worktree、未知 agent、未知项目、档位越权被拒；`policy/describe` 给出 `full_access`；有 `approve` 的 CLI 可以放宽到 `elevated`。
  - **不睡眠**：全部用 `ManualClock`；等待靠订阅会话流加读日志，或驱动替身收到的命令。连跑 15 次稳定。
  - **CI**：`cargo xtask ci pr --record docs/tasks/T043-session-actor/ci/` 在干净的 worktree 里通过（提交 `8b4e967`）：
    - rust 1,093 个通过；
    - protocol 176 个通过；
    - ts 291 个通过；
    - license 和 testdata-scan 通过。

    摘要已提交。
  - **用例编号**：测试名都以 `it_orch_lifecycle_` 开头，编号已在 `tests/ids.toml` 的 T043 段里。
- **规模**：新增 2,028 行，删除 260 行，不含笔记、设计文档和 CI 摘要。挪动的代码（`Session`、`Peer`）同时计入增和删，净增约 1,770 行。

## 提交列表

```
f05770f docs(task): T043 CI summary
8b4e967 docs(task): T043 sizes, notes and handoffs
55ddf36 test(host): IT-orch-lifecycle
706c12a feat(host): run sessions in actors behind the gateway's session port
fc5a04d test(host): move the dispatch tests' Peer into the gateway testkit
3970704 feat(host): add AgentPort, a session's way to its agent's runtime
3ad6eb7 refactor(host): make the session's composed core production code
2139b17 feat(host): add Alarm, the clock timer for a core's next deadline
16c0103 feat(host): keep a queued message's reply_to in queue_entries
161b1b3 Merge branch 'main' into feat/T043-session-actor
27432ee docs(task): T043 size estimate and split proposal
```

## 遗留问题 / 范围外发现

- **交给 T088**：
  - agent 发来的请求（`DriverEvent::Ask`）现在没人应答，actor 也不处理 `CancelAsks` 和 `Settle`。接线点在 `SessionCore` 的请求计数和 actor 的 `Action::Machine(_)` 分支。
  - `send` 的 `reply_to` 暂时不写进 `SendRequest`，核对之后再填；列和行的写法已经有了。
  - 还要补 `interactions` 的行和 `facts.option`。
- **交给 T089**：
  - 不在内存里的会话，现在 `deliver` 回 `not_found`；重启之后的 `get` 也一样。按需载入接在 `deliver` 里。
  - 排队消息到命令的对照要从 `queue_entries` 恢复。
- **交给 T037（宿主组装）**：`SessionManager::new(SessionDeps { streams, writer, readers, clock, ids, durations, agents, quiescence })`，再 `Routes::start(.., Arc::new(manager), index)`。
- **交给 T044**：
  - 信任状态只在创建时判定一次，attach 和每次 prompt 之前的重扫由 T044 做；
  - `facts` 对 `session/set_config` 还没有 `mode_tier`。
- **交给 T061**：快照端口是 `actor.rs` 里的 `skipped()`，换成真正的快照任务即可。
- **范围外**：
  - `cupper-host` 的 tokio 依赖加了 `macros` 特性，用于 actor 的 `select!`；`Cargo.lock` 没有变化。
  - `turns.snapshot_status` 只有一列，最后记的是结束快照的状态。
- worktree 保留着，可以继续追问。