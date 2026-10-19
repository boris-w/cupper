# T043 工作笔记

- 执行者：子 agent（Rust 工程师，宿主编排）

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行。按任务书的要求，先写草估，再按计划 1.1 的经验放大：实现 ×1.7，组合测试 ×1.5。模块按可以拆开的三组列出：骨架与命令面（A）、交互接线（B）、恢复与超长输出（C）。

### A：actor 骨架与命令面

**实现**

| # | 模块 | 内容 | 草估 |
|---|---|---|---|
| A1 | `pure` 的 `Alarm` | 给 `TimerTable::next_deadline` 维持一个 `Clock::timer_at`，截止时间变了才重设；没有定时器时一直挂起（T022 的交接） | 35 |
| A2 | `driver::api` 的 `AgentPort` | actor 到 `AgentRuntime` 的接口：`attach(Attach, 事件发送端) -> 回复的 future`、`send(DriverCommand)`；按 `AgentKind` 取端口。T055 实现，测试用进程内替身 | 30 |
| A3 | `orchestrator/session/core.rs` | 组合核心：回合机、裁判、合并器，按 `session_props.rs` 的 `Session` 接线。打算把那里的 `Session` 提成正式代码，`session_props.rs` 改用它，免得两份接线各自漂移；再加上 actor 要执行的输出（attach、快照、驱动命令、回复） | 110 |
| A4 | `orchestrator/session/actor.rs` | 邮箱循环（命令、驱动事件、`Alarm`）；每一步的动作执行：事件攒成一个 `Batch`，`with` 里写行和账本的 `Completion`，`append` 后 `committed()` 再回复；`Appended::rejected` 记日志，测试构建里 panic；驱动命令；attach 另起 task（`policy::attach`、工作区、配置，回来后按需写 `session.capabilities`、`session.configured`，更新合并器的 `interrupt_stops_commands`）；快照端口直接回 `skipped`；后续输入回送 | 300 |
| A5 | `orchestrator/session/rows.rs`（部分） | `sessions`（创建时插入、`state`、第一个回合开始时清空 `initial_prompt_json`、`agent_session_id`、`config_json`）、`turns`（开始、结束、快照状态）、`queue_entries`（随 `queue.updated` 整体替换，带 `command_id`）和 `queue_version`、`queue_paused` | 110 |
| A6 | `orchestrator/session/manager.rs` | 句柄表；`SessionPort::deliver` 按 `session_id` 同步投进邮箱、返回回复的 future；`facts`：`mode_tier`、`running_turns` | 100 |
| A7 | `session/create` | 查项目行和信任状态、能力缓存；`InitialConfig` 的模式按 `risk` 换成 `Tier` 后 `policy::tier_change`（映射里没有的按 `FullAccess`）；`policy::attach`；只做本地模式，worktree 回 `unsupported`；一个事务：`sessions` 行（含 `initial_prompt_json`）、`session.configured`、`workspace.ready`、账本行；启动 actor，开第一个回合 | 110 |
| A8 | 命令面 | `send`、`interrupt`、`queue_resume` 的参数与回复（`QueueFull` → `too_large/queue_full`，`TurnMismatch` → `conflict/turn_mismatch`）；`session/capabilities`；`session/get`（`sessions` 行加视图算出摘要） | 90 |
| | 小计 | | **885**，×1.7 ≈ **1,505** |

**测试**

| 项 | 内容 | 草估 |
|---|---|---|
| `Alarm` 单元测试 | `ManualClock` 推进、改早、取消 | 30 |
| 夹具 | 临时 host.db、写线程、读池、`Streams`、账本、方法表；直接写库造项目行和能力缓存；进程内驱动替身（记下命令，由用例回复 attach、发事件）；会话流上的事件等待器（`Sink` + `Notify`，不睡眠）；网关的 `Peer`（现在是 `dispatch/tests.rs` 私有的，要挪到 `gateway/testkit.rs` 做成 `pub(crate)`） | 200 |
| IT-orch-lifecycle（A 部分） | 创建 → 第一个回合（同一事务清空 `initial_prompt_json`）→ 结束；回合进行中排队、结束后出队；`queue_full`；中断：prompt 回 `cancelled`、10 s 截止（`ManualClock`）；队列暂停与 `queue_resume`；经账本重放；创建时档位被拒 | 170 |
| | 小计 | **400**，×1.5 = **600** |

**A 合计约 2,100 行。**

### B：交互接线

| # | 内容 | 草估 |
|---|---|---|
| B1 | 组合核心接上交互代理：请求先送裁判（`Asked`，必要时先开自发回合），再送合并器（`Interject`），然后交互代理带着回合 ID 写出；撤回、`CancelAsks`、`Settle`、适配器退出转给交互代理；每次解决后送裁判 `Resolved`；回合机加一个读当前回合的接口 | 70 |
| B2 | `interaction/respond`（`Respond.tier` 用会话当前配置按能力换算）、`interaction/get`（视图里的请求，或交互代理的已结束表）、`reply_to` 核对（交互代理加一个"本会话已解决"的查询）、`facts.option` | 80 |
| B3 | `interactions` 行：写出时插入（带适配器实例），解决时更新 | 45 |
| B4 | 通知索引的接口（T034 之前为空实现）；CLI、test_harness 完成的回答写审计 `interaction.responded`（网关审计记录加一种） | 30 |
| | 实现小计 | **225**，×1.7 ≈ **383** |
| 测试 | 经网关回答（先到先得、后到 `already_resolved`）；中断取消挂着的请求；回合外的请求开自发回合并带上回合 ID；收尾期间到达的请求进下一个自发回合（交接里要确认的一条）；`reply_to` 的接受与拒绝（未解决、别的会话、不存在） | **140**，×1.5 = **210** |

**B 合计约 590 行。**

### C：恢复与超长输出

| # | 内容 | 草估 |
|---|---|---|
| C1 | 按需载入：内存里没有 actor 的会话，先建邮箱再在 task 里载入，保证命令顺序；读 `sessions` 行、队列、交互、最大回合序号 | 70 |
| C2 | `TurnMachine` 从库恢复（回合序号、队列、agent 会话），`QueueCore` 从行恢复 | 35 |
| C3 | 已结束的请求用 `Input::Ended` 交给交互代理 | 20 |
| C4 | 补写 T037 启动清理的事件：运行中回合里没结束的条目 `abandoned`、挂着的请求 `expired`（`host_restarted`）、`turn.completed{interrupted}`，同一批写出 | 60 |
| C5 | 检查点因 `view_v` 不符作废时，重发 `session.configured`、`session.capabilities`、`queue.updated` | 30 |
| C6 | 准备阶段崩溃（`initial_prompt_json` 仍在）：重新开第一个回合 | 15 |
| C7 | 超长工具输出：超过内联上限后停发 delta、标 `truncated`、每秒至多一次更新尾部（新时长登记进 `durations.rs`）；条目结束时 `BlobStore::put`，同一批里 `add_ref`；`Replace` 超长同样处理 | 130 |
| | 实现小计 | **360**，×1.7 ≈ **612** |
| 测试 | 重启：丢掉管理器，按 T037 的方式改库，再载入，断言补写的事件、恢复的回合序号与队列、resume 用的 agent 会话、对已结束请求的回答；检查点作废后的重发；准备阶段崩溃；超长输出（流式与一次给出） | **150**，×1.5 = **225** |

**C 合计约 840 行。**

### 合计

约 **3,530 行**（A 2,100 + B 590 + C 840），是上限的 2.3 倍。按任务书，开工前停下，见下一节。

## 拆分建议

A 里有一块分不开的固定成本：actor 的循环与执行（A4）、管理器与端口（A6）、行（A5）、测试夹具，放大后合计约 1,170 行，还没算任何一个方法。所以不管怎么拆，第一个任务都会超过 1,500 行；能做的是让它只含骨架和最少的方法。

**方案一（建议）：拆成三个任务。**

- **T043（收窄，关键路径）**：A。actor 骨架、`session/create`、`send`、`interrupt`、`queue_resume`、`get`、`capabilities`，经网关的整条路径。约 2,100 行，超出约 40%，请编排者决定是否接受；写的时候测试用表驱动、断言写单行，尽量压缩。
- **T043b（交互接线）**：B，约 590 行。依赖 T043。T044（适配器退出时请求 `expired`、K6）和 T046（`approve`/`reject`）都要它，所以在关键路径上：T043 → T043b → T044。
- **T043c（恢复与超长输出）**：C，约 840 行。依赖 T043，可以和 T043b、T044 并行；两者都会在 `actor.rs`、`manager.rs` 上加接线，冲突面是几处调用点。T062、T069 的崩溃一致性用例依赖它。

**方案二：任务书举例的两分法。**

- T043：A，约 2,100 行；
- T043b：B + C，约 1,430 行。

关键路径一样，只是 C 也压在 T044 前面。

**方案三：A 再按纯核心与 I/O 拆。** 先做纯的组合核心（A3、B1、C2、`Alarm`、超长输出的判定），`session_props.rs` 改用它；再做 I/O 的 actor 与命令面。第二个任务仍约 1,850 行，而且关键路径上多一个任务，不建议。

## 待裁定

1. **`queue_entries` 没有 `reply_to` 列**：重启后从库恢复队列时，排队条目的 `reply_to`（v1.5）会丢。建议按 P-4 在 `0001` 里加 `reply_to TEXT`，同步改 5.4.2。`store` 的测试会拿迁移结果和设计里的 SQL 比对，所以设计也要改。
2. **`git` 快照前的快照状态**：计划 1.3 说"与非 git 项目相同"，记为 `skipped`。打算不带 `reason`；带 `not_a_repo` 对 git 项目是错的。
3. **能力缓存为空时的 `session/create`**：模式换不出档位，按规则当作 `full_access`，于是只有 write 的 CLI 指定 `permission_mode` 会被拒；不指定模式时不做档位检查。合并器的 `interrupt_stops_commands` 先按 `false`（保守，会标 `may_still_run`），attach 之后按能力更新，需要给 `Coalescer` 加一个设置函数（T041 的代码，几行）。
4. **actor 空闲 10 分钟退出**（5.2.3、5.2.6）不在任务书的范围里。建议放进 C：按需载入是它的另一半。
5. **组合核心的位置**：把 `session_props.rs` 里的 `Session` 提成正式代码，性质测试改用它（B 把交互代理接进去之后，性质测试里的替身计数也一并换掉）。
6. **网关的 `Peer`**：IT 要经过策略和账本，需要把 `dispatch/tests.rs` 里私有的 `Peer` 挪到 `gateway/testkit.rs`，只改可见性，不改行为。
7. **`interaction.responded` 审计**：网关的审计记录现在都按连接组织；打算加一种带 `ConnId` 的记录，由会话 actor 发进同一个 channel，落盘归 T059。
8. **UNIT-unsupported-set**：T033 已经用 `Routes::sessions` 登记了 M1a 的会话方法，允许表里没有本任务要删的行。

## 裁定之后（计划 v1.3.11）

编排者采用方案一：本任务只做 A，上限放宽到约 2,100 行，硬停止线 2,200 行；B 归 T088，C 归 T089。上面 7 项待裁定全部认可，写进了 `02-host.md` 5.2.6 末尾"T043 开工前的裁定"。

### 实际规模

按 `git diff --numstat` 统计，不含本笔记和 CI 摘要。

| 提交 | 内容 | 增 | 删 |
|---|---|---|---|
| `feat(host): keep a queued message's reply_to …` | `0001` 和 5.4.2 的 `queue_entries.reply_to` | 2 | 0 |
| `feat(host): add Alarm …` | `pure/alarm.rs` | 85 | 2 |
| `refactor(host): make the session's composed core …` | `session/core.rs`（从 `session_props.rs` 挪来）；`TurnMachine::turn_id`、`Coalescer::set_stops_commands` | 230 | 168 |
| `feat(host): add AgentPort …` | `driver/api.rs` | 20 | 3 |
| `test(host): move the dispatch tests' Peer …` | `gateway/testkit.rs` | 82 | 74 |
| `feat(host): run sessions in actors …` | `actor.rs` 533、`manager.rs` 389、`rows.rs` 175、`mod.rs` | 1,130 | 12 |
| `test(host): IT-orch-lifecycle` | `session/tests.rs` | 479 | 1 |
| 合计 | | **2,028** | 260 |

挪动的代码（`Session` 约 160 行、`Peer` 约 75 行）同时计入增和删，净增约 1,770 行。`actor.rs` 第一稿 552 行、`rows.rs` 251 行、测试 609 行，合计超过了硬停止线；在提交之前压缩：行的写法改成"语句加参数"的表，测试的断言改成单行（行拼成字符串、事件类型拼成一行、错误码拼成 `code reason`）。

### 实现要点

- **组合核心**（`session/core.rs`）：`session_props.rs` 的 `Session` 原样提成正式代码，性质测试改用它。`Input::Asked`、`Input::Resolved` 和 Settle 时结清请求的计数，是交互代理的位置，T088 换成 `InteractionBroker`。
- **actor 的一步**：
  - 每个输入是核心的一步；快照的回复作为下一个输入，各自一个批次。
  - 一步的事件、它改动的行、命令的账本记录在同一个事务里提交（`Batch::with`）；之后才回复、发驱动命令、起 attach 的 task。
  - 没有事件、只有行的一步（例如 resume 回来的 attach），行和账本记录经写线程单独提交。
  - 前置条件不满足（`queue_full`、`turn_mismatch`）的命令用 `Ticket::reject`，不记录。
  - `Appended::rejected` 不为空时，测试和 test-harness 构建里 panic，其他构建写 stderr。
- **定时器**：`TimerTable` 加 `Alarm`；到期后一次取一个键送进核心，因为前一步可能取消后一个。
- **attach**：另起 task，结果从 channel 回来。带回的能力和配置变了才写 `session.capabilities`、`session.configured`；`agent_session_id` 写进行；合并器的 `interrupt_stops_commands` 按能力更新。
- **行**（`rows.rs`）：
  - `turn.started` 插入 `turns`，`sessions.state` 改为 `running`，同时清空 `initial_prompt_json`；
  - `turn.completed` 更新 `turns` 和 `sessions.state`（`failed` → `error`，`interrupted` → `interrupted`，其余 → `idle`）、`last_turn_end_seq`；
  - `snapshot.recorded` 更新 `turns.snapshot_status`；
  - `queue.updated` 整体替换本会话的 `queue_entries`，`command_id` 由 actor 记下每条排队消息来自哪个命令；
  - `session.configured` 更新 `config_json`。
- **`session/create`**：本地模式。查项目行和信任（`policy::attach`）、能力缓存；模式按缓存里的 `risk` 换成档位，换不出就当 `full_access`，再 `policy::tier_change`。一个事务写 `sessions` 行（含 `initial_prompt_json`，标题取第一条消息的预览，`title_source: prompt`，`mode_set_by` 为创建者）、`session.configured`、`workspace.ready` 和账本行；提交后才登记句柄、启动 actor、回复。actor 的第一步就是这条消息。
- **`session/get`**：读 `sessions` 行，状态按 5.2.9 的表从视图和回合算出。
- **`facts`**：`session/create` 的 `mode_tier` 来自能力缓存；`running_turns` 是各 actor 的运行标志之和。
- **`reply_to`**：核对归 T088，本任务的 `send` 不写它（`SendRequest.reply_to` 恒为 `None`），列和行的写法已经有了。

## 给其他任务的接入点

- **T088（交互接线）**：
  - `SessionCore` 的 `asks` 计数换成 `InteractionBroker`；
  - actor 里 `Action::Machine(CancelAsks | Settle)` 现在不处理，`DriverEvent::Ask` 也没人应答，都从这里接；
  - `interactions` 的行加在 `Rows::of` 里；
  - `send` 的 `reply_to` 核对后填进 `SendRequest`；
  - `facts` 加上 `option`。
- **T089（恢复、空闲退出、超长输出）**：
  - `SessionManager::deliver` 找不到句柄时回 `not_found`，按需载入接在这里：先建邮箱、登记句柄，再在 task 里载入，命令顺序不变；
  - actor 的 `commands`（排队消息到命令的对照）要从 `queue_entries` 恢复；
  - 空闲退出时关闭邮箱即可，`run` 在邮箱关闭时返回。
- **T037（宿主组装）**：`SessionManager::new(SessionDeps { streams, writer, readers, clock, ids, durations, agents, quiescence })`，然后 `Routes::start(.., Arc::new(manager), index)`。`SessionManager::new` 自己在静止屏障里登记 `session`。
- **T044**：信任状态目前只在创建时判定一次，attach 和每次 prompt 之前的重新扫描归 T044；`facts` 对 `session/set_config` 还没有 `mode_tier`。
- **T055**：实现 `AgentPort`；`SessionDeps.agents` 按 `AgentKind` 放每种 agent 的端口；attach 时传进来的事件发送端收这个会话的全部事件。
- **T061**：快照端口是 `actor.rs` 的 `skipped()`，换成真正的快照任务，结果同样作为输入回送。
