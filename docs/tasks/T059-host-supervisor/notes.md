# T059 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）
- 日期：2026-10-20

## 读到的现状（开工前）

- **退出的等待点**：`Host::run` 等 `exits`（`Control` 发 `Exit::Requested`）、SIGTERM、写线程结束三者之一；`Host::exit` 固定发 `user_quit`，`force` 只在 `Requested { force: false }` 以外为真。空闲、孤立、待升级要作为新的 `Exit` 取值走同一个通道。
- **活动**：`Part::activity()` 只有 `running_turns`、`pending_interactions`，没有排队消息数，也没有变化通知。会话句柄里有 `running`、`pending` 两个原子量，由会话 actor 写；排队消息数要照样加一个（队列在会话核心里）。连接数从 `Gateway::clients()` 取。
- **新会话和新回合的闸门**：`SessionManager::deliver` 里 `session/create` 走 `create`，其余按会话分发到 actor；闸门可以放在这里，对 `session/create`、`session/send` 回 `unavailable`。待升级有现成的 `pending_upgrade`；孤立时拒绝用什么 `reason`，协议里没有对应的取值（见下文"要确认的事"）。
- **父进程**：`dev-auth` 构建启动时 ppid 已是 1 的情况 T021 已处理（`startup::launcher()` 返回 `LauncherGone`，以错误码退出）。运行中的父进程退出要另外监视：标准库的 `std::os::unix::process::parent_id()` 就够，按 `Clock` 定时轮询，不需要新的 `unsafe`。
- **自检**：`cupper_sys::own_cdhash()`、`file_cdhash(path)` 都有。新连接的钩子加在 `Gateway::serve` 的 accept 之后；"拉起子进程之前"目前只打算挂在适配器拉起（`driver/runtime/launch.rs`）上，git、登录 shell 探测用的是系统程序，与包是否被替换无关（见"要确认的事"）。
- **清理残留**：`cupper_sys::ProcessGroup`（`pgid` + 首进程启动时间，`signal` 只发给仍是记录中那个的组）、`driver::runtime::lifecycle::{union, signal}` 都能复用；`adapters` 表有 `pgid`、`start_us`、`groups`（`[[pgid, start_us], …]`）、`ended_at`。适配器 actor 里 TERM → 等 → KILL 的时序绑在它自己的定时器表上，不能直接拿来用；启动时的清理写成一个独立的 async 函数（经 `Clock` 等待、每 50 ms 轮询），T055 可复用。
- **审计**：`AuditEvent` 现有 `Accepted`、`Rejected`、`Initialized`、`Closed`（12 种原因，含 `HostExiting`）、`Subscribed`（只进 host.log）、`InteractionResponded`（T088）、`ConfigUnconfirmed`（T044；`config.confirmed` 还没有产生者）、`Elevation`（T038 的四种 `step`）。T109 没有审计记录。`audit` 表列为 `ts, event, conn_id, principal, peer_json, detail_json`。host.log 是 tracing 订阅者（T028），审计行用一个固定 target 写进去即可。`conn.accepted` 的对端信息用 `AuditToken::signing_identity()`（T133）和 `pid_path`。
- **流订阅**：`routes.streams(..)` 确实还没接，`Host` 里也没有 `streams` 字段了（只剩局部变量），接上即可。
- **用例**：进程级用例在 `tests/lifecycle.rs`，用 `CARGO_BIN_EXE_cupper-host` 和 harness 客户端；假 agent 有 `wait_cancel`（回合一直运行到取消），可以用来让 IT-self-check 在待升级时还有运行中的回合。

## 开工前的规模估计

实现按以往的实际偏差乘 1.3，测试乘 1.2（任务书给的经验是实现 1.7、组合测试 1.5；我的粗估已经按模块列到函数，取自己的历史偏差）。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 监护纯核心 | 输入：连接数与活动（含排队消息）、安装器忙闲、孤立、包已变（自检失败或 T129 的 `Failed(BundleChanged)`）、定时器；输出：是否接受新会话和新回合、何时以什么原因退出、定时器。空闲 10 分钟、孤立等回合结束、待升级立即退出的条件与优先级 | 195 | 155（UNIT-orphan-policy，表驱动） |
| 监护 actor 与宿主接线 | 新的 actor 种类（登记、`Participant::idle`、一个闹钟一张表）；按 `Clock` 定时观察活动与连接数；`dev-auth` 下轮询父进程；`Exit` 加三个取值、`host/shutdown` 的 `reason` 按取值；会话管理器的闸门（`session/create`、`session/send`）；`Activity` 加排队消息数（会话核心、actor、管理器）；`host/status.pending_upgrade` 取监护状态；自检钩子（accept 之后、适配器拉起之前）；给 T129 的入口 | 340 | — |
| 自检 | `current_exe()` 的 `file_cdhash` 对 `own_cdhash()`，后者失败时不检 | 30 | — |
| 清理残留 | 读 `ended_at` 为空的 `adapters` 行及其 `groups`；TERM → 每 50 ms 轮询、最长 2 s → KILL；行标为已结束；排在 `maintenance.spawn` 之前；两个时长 | 130 | 70（`setsid` 拉起、忽略 TERM 的测试进程组被 KILL，结束后无残留） |
| 审计落盘与正式格式 | 接收端交给写入任务（登记、`Hold`）；每种事件映射成 5.8.6 的事件名与内容（含 `host_exiting`、提权四种、`interaction.responded`、`config.unconfirmed`）；`conn.accepted` 补可执行文件、签名 identifier、Team ID；同时写 host.log；Maintenance 里 90 天清理；改掉 `gateway/audit.rs` 开头"归 T038"的注释 | 325 | 110（表里的正式记录、90 天清理） |
| 流订阅 | `routes.streams(streams, readers)` | 5 | 25（真实宿主上订阅一次） |
| 进程级用例 | IT-self-check（复制出的宿主可执行文件被删、`wait_cancel` 的回合在跑：新连接触发待升级、新建会话被拒、`host/status` 为真、取消回合后以 `upgrade` 退出）；IT-idle-exit（缩短时长；连着时不退出，断开后退出） | — | 240 |
| 时长登记 | 空闲、观察间隔、父进程轮询、清理的等待与轮询 | 20 | — |
| **合计** | | **约 1,045** | **约 600** |

合计约 **1,650 行**，超过约 1,500 的上限。按任务书和守则第 8 条，开工前停下汇报，没有写代码。

## 拆分建议

按编排者给的方向，把审计整块拿出去：

| 拆分 | 内容 | 验收 | 估计 |
|---|---|---|---|
| **T059 监护、孤立、自检与清理** | 监护纯核心与 actor、`Exit` 的新取值与 `reason`、会话闸门与排队消息数、父进程监视、自检（新连接、适配器拉起）、给 T129 的入口、清理残留、`routes.streams` | UNIT-orphan-policy、IT-self-check、IT-idle-exit、清理例程的用例、流订阅用例 | 约 1,210 |
| **新任务：审计落盘与正式格式** | 写入任务、全部事件的正式格式与 host.log、`conn.accepted` 补对端信息、90 天清理 | 审计的两条验收（正式记录、90 天清理） | 约 435 |

- 两半几乎不碰同一批文件：审计动 `gateway/audit.rs`、一个新的审计写入模块、`store/retention.rs`，在 `Host::start` 里只改接收端的去向；T059 动 `supervisor/`、`orchestrator/session/`、`gateway/mod.rs`（accept 钩子）、`driver/runtime/launch.rs`。可以并行。
- 审计任务不在 T062 的前置里也不影响其他任务：现在的审计记录仍发到同一个 channel，只是没人收。

## 要确认的事

1. **IT-idle-exit 的验收自相矛盾**：空闲的前提是没有客户端连接，所以空闲退出时没有人能收到 `host/shutdown{reason: idle}`。打算改为验证：退出码 0、`meta.last_clean_exit_at` 已写、host.log 里有以 `idle` 为原因的有序退出记录；`Exit::Idle → idle` 的映射另由单元测试覆盖。
2. **孤立时拒绝新会话和新回合的 `reason`**：协议里没有对应取值。打算回 `unavailable`，不带 `reason`；或者新增一个取值（要动协议与生成物）。
3. **"每次拉起子进程之前"的自检**：打算只挂在适配器拉起上（会执行包内运行时的唯一一类子进程），git 和登录 shell 探测不挂。
4. **待升级时 `host.updated.pending_upgrade`**：任务书只要求 `host/status`。索引流里的 `host.updated` 要不要同时更新（由索引 actor 写）？不加的话约少 20 行。
