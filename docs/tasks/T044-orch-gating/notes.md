# T044 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）
- 日期：2026-10-20

## 读到的现状（开工前）

- `SessionActor`（`orchestrator/session/actor.rs`）只处理 `send`、`interrupt`、`queue_resume`、`interaction/*`、`session/get`、`session/capabilities`；`session/set_config` 在协议里是 M1b（`milestone: M1b`，UNIT-unsupported-set 的允许表里是 `Later(M1b)`），`session/confirm_config` 是 `Until("T044")`。
- 适配器退出：`TurnMachine::adapter_exited` 已把回合标为 `failed`（`adapter_exited`），`link` 变为 `Detached(Some(会话))`，下一次发送时 `Attach(Resume)`；交互请求由 broker 标为 `expired`。**缺**：`notice{agent_restarted}`、K6（"Session not found" 时自动 resume 再重发一次）、K7 的 notice。
- 晚到输出：`Coalescer` 已经写 `late_tool_output`（每个原工具一个条目、追加输出、`completed` 结束）和 `may_still_run`；没有进行中的回合时由 judge 开自发回合。T044 这部分主要是 IT-orch-gating 里的覆盖，可能有小的接线。
- 队列暂停：`Queue::pause` 在队列为空时不起作用；协议 5.3.4 要求 `config_unconfirmed`、`project_config_changed` 期间"新消息照常排队、不发出"，空闲时发送也不能开回合，所以 `TurnMachine` 要加一道"闸门"。
- 信任：`trust::scan`、`Trust::rescanned` 是纯的（扫描阻塞，放线程池）；项目的信任状态在 `projects` 表里，而它的唯一写者是 T034 的 `IndexActor`（还没合并），会话 actor 只能经 `IndexNotes` 一类的接口通知它。
- 会话的 `ProjectConfigPolicy` 在 `session/create` 时定下，存在 `Parts.project_config`，attach 时原样传给驱动。

## 开工前的规模估计

实现按 T054、T060、T098 的经验乘 1.3（文档注释和 rustfmt 展开），测试按写出来的估。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 档位核对（纯核心） | 记下档位的设定者和是否经原生确认进入 `full_access`；Attach、resume 之后和每次 `ConfigChanged` 时核对实际档位（`capabilities::tier`、`policy::ceiling`）；进入 `config_unconfirmed` | 160 | — |
| 回合中越权 | 先用 `SetConfig` 改回、`Clock` 等 2 s；失败、超时或再次越权就走中断流程；`notice{config_unconfirmed}` 和审计 | 90 | — |
| 闸门 | `TurnMachine` 的"队列暂停时空闲发送也只排队"（`config_unconfirmed`、`project_config_changed`），解除后出队 | 60 | — |
| `session/confirm_config` 与接线 | 方法（核对 `config_digest`）、`Routes::sessions` 登记、删允许表那一行；`facts` 给 `session/set_config` 补 `mode_tier`；`sessions.mode_set_by`、`full_access_confirmed_at` 的行；`session.configured{confirmation_required}` | 150 | — |
| 信任门控 | Attach 前、发 prompt 前、回合结束时重扫：经 T098 的仓库档取 git 根目录（`SessionDeps` 加 `GitTool` 和可写根目录的来源）、`trust::scan` 放阻塞线程池、`Trust::rescanned`；`TurnMachine` 的 Preparing 多等一个"信任检查完成"；`stale` 时暂停、`notice{project_config_changed}`、通知索引；未信任时 `settingSources: ["user"]`、`project_config: ignored` | 300 | — |
| "不加载项目配置继续" | `session/close` 后带 `Ignore` resume，`session.configured` 改写 | 60 | — |
| 崩溃与晚到 | `notice{agent_restarted}`；K6 的自动 resume 与重发（只在这个 prompt 还没有更新时）；K7 的 notice；晚到输出的接线核对 | 100 | — |
| IT-orch-gating | 夹具扩展（驱动替身的 `ConfigChanged`、attach 选项记录、各类 `PromptFailed`；临时项目和 git 仓库）；档位与 `config_unconfirmed`、`confirm_config`（120）；越权改回与中断（80）；三个时机的重扫、`stale`、继续（150）；退出、K6、K7（100）；晚到输出（50） | — | 500 |
| **合计** | | **约 920，乘 1.3 约 1,200** | **约 500** |

合计约 **1,700 行**，超过约 1,500 的停止线。按 brief 和守则第 8 条，开工前停下汇报，没有写代码。

## 拆分建议

三部分都改 `SessionActor`、`TurnMachine` 和同一个 IT 夹具，并行会在 `actor.rs`、`turn.rs`、`core.rs` 上冲突，建议顺序做，或者只把第三份与前两份之一并行：

| 拆分 | 内容 | 用例 | 估计 |
|---|---|---|---|
| **T044a 档位、越权与 `confirm_config`** | 档位核对、回合中越权、闸门、`confirm_config` 与接线；IT-orch-gating 的夹具 | IT-orch-gating 的档位部分 | 约 780 |
| **T044b 信任门控** | 三个时机的重扫、git 根目录、`stale` 暂停与提示、未信任时的 `settingSources`、"不加载项目配置继续"；复用 a 的闸门 | IT-orch-gating 的信任部分 | 约 620 |
| **T044c 崩溃与晚到输出** | `agent_restarted`、K6、K7、晚到输出的核对 | IT-orch-gating 的崩溃与晚到部分 | 约 280 |

c 只碰 `TurnMachine` 的失败路径和 notice，与 a 的冲突最小，可以和 a 并行；b 依赖 a 的闸门。

## 需要裁定的问题（不管拆不拆都要先定）

1. **`session/set_config` 是 M1b**（协议和允许表都这么写）。T044 只给 `facts` 补 `mode_tier`、在越权判断里把"经有权限的 `set_config` 设定"当作一种合法来源，还是要在 M1a 就实现 `session/set_config`？后者另需约 150 行。
2. **"不加载项目配置继续"在协议里没有方法**（`session/queue_resume` 没有参数，`project/trust` 是信任）。要加一个方法或参数（协议改动，T015 的生成物随之变），还是 M1a 只做宿主内部的入口、由后续任务接到线上？
3. **`projects` 表的唯一写者是 T034 的 `IndexActor`**（未合并）。会话 actor 重扫发现 `stale` 时，状态改写要经 `IndexNotes` 加一个通知（例如 `trust_rescanned(project, scan 摘要)`）交给 T034，还是本任务先在会话 actor 里只读、不改项目状态（只暂停本会话、写 notice），改写留给 T034？
4. **发 prompt 之前发现 `stale` 时，已经开始的回合怎么结束**（`turn.started` 已写，处于 Preparing）：标 `failed`（新的失败码 `project_config_changed`？协议的 `FailureCode` 里没有），还是标 `cancelled` 并把这条消息放回队首？设计没有写。
5. **可写根目录的来源**：仓库档的 `run` 需要"全部项目和工作区根目录"。M1a 只有本地模式，是否就用 `projects` 表里全部未删除项目的路径？

## 收窄后的计划（编排者裁定后，计划 v1.3.19）

T044 = 档位、越权、闸门、`confirm_config` 与接线 + 崩溃与晚到输出；信任门控归 T109。上限约 1,500，硬停止线 1,600。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `TurnMachine` | 闸门 `Input::Gate`（暂停期间新消息排队、空闲也不开回合、Preparing 不发 prompt、`queue_resume` 不解除）；`agent_session()`；K6：prompt 没有任何更新就收到 "Session not found" 时 resume 并重发一次，写 `session_resumed`；适配器退出、`process_exited`、`restartable` 写 `agent_restarted` | 140 | — |
| `TierGuard`（纯核心）与 `SessionCore` 接线 | 设定者与原生确认；Attach、`ConfigChanged` 时核对（`full_access` 未确认、高于设定者的上限）；回合中越权：`SetConfig` 改回、`Clock` 等 2 s，失败、超时或再次越权就中断，然后进入 `config_unconfirmed`；选项改档位时登记合法来源（`set_config` 的口子预留）；`confirm_config` 解除 | 230 | — |
| `SessionActor` 等接线 | `ConfigChanged` 改写 `session.configured.config`；`confirmation_required` 与 notice、审计（`AuditRecord.conn` 改为 `Option`，会话自己发起的没有连接）、`mode_set_by`、`full_access_confirmed_at` 两个行字段；`session/confirm_config`（`config_digest` 核对）、`Routes::sessions` 登记、删允许表那一行；`facts` 给 `set_config` 补 `mode_tier` | 200 | — |
| IT-orch-gating | 档位与 `config_unconfirmed`、`confirm_config`、闸门；越权改回与中断；退出、K6、K7；晚到输出 | — | 400 |
| **合计** | | **约 570，乘 1.3 约 740** | **约 400** |

合计约 **1,140 行**，在上限之内。每完成一个模块统计一次。
