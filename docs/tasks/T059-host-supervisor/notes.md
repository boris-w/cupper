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

## 裁定后的范围（计划 v1.3.31）

T059 = 监护核心与 actor、`Exit` 新取值与 `reason`、会话闸门与排队消息数、父进程监视、自检、给 T129 的待升级入口、清理残留、`routes.streams`。审计整块归 T138。上限约 1,300，硬停止线 1,400。四件事：IT-idle-exit 按退出码、`last_clean_exit_at`、host.log 验；孤立时回 `unavailable` 不带 `reason`，host.log 记原因；自检挂在新连接和适配器拉起之前；`host.updated.pending_upgrade` 一起更新。

## 实现记录

### 监护核心（`supervisor/watch.rs` 的 `WatchCore`，UNIT-orphan-policy）

- 输入：`Observed { clients, activity, installer_busy }`、`Orphaned`、`BundleChanged`、`Expired(Idle)`；输出：`Close(Orphaned | PendingUpgrade)`、`Exit(Idle | Orphaned | Upgrade)`、定时器。
- 规则：
  - 待升级：没有运行中的回合、待处理的交互和会运行的排队消息时立即退出，不看连接；
  - 孤立：没有运行中的回合时退出，不看连接和待处理请求（运行中的回合才有待处理请求）；
  - 空闲：没有连接、没有上述三样、安装器空闲时开始计时，任一活动取消，计满 `supervisor_idle`（10 分钟）退出。
  - 优先级：待升级先于孤立先于空闲。待升级要等待处理的请求，孤立不等，所以"孤立 + 待升级 + 有待处理请求、没有回合"时按孤立退出。
  - 第一次观察之前不做退出判定；退出之后不再有任何动作。
- 排队消息只算会运行的：队列暂停时（等用户恢复）记为 0。否则一条被中断后暂停的排队消息会让空闲退出和待升级退出永远等下去。消息在库里，退出不会丢。

### 监护 actor 与宿主接线

- 新的 actor 种类 `supervisor`，在 `Watching::start` 里登记；一个闹钟、一张定时器表（`Observe` 与核心的 `Idle`），等待点包在 `Participant::idle` 里。每 `supervisor_observe`（1 s）看一次：各 `Part` 的活动之和（`Activity` 实现了 `Sum`，`host/status` 也用它）、`Gateway::clients()` 的个数，`dev-auth` 构建再比较 `parent_id()` 与启动时的父进程。
- 两阶段：`Watch::new()` 在 `Host::start` 前段给出句柄（自检与 T129 用），网关建好后 `Watching::start` 才启动 actor；之前发来的消息排在信箱里。
- 动作：`Close` 写进共享的 `Admission`（会话管理器对 `session/create`、`session/send` 回 `unavailable`；待升级带 `pending_upgrade`），记 host.log，待升级时让索引写 `host.updated{pending_upgrade: true}`；`Exit` 经已有的 `exits` 通道交给 `Host::run`。
- `Host::exit`：`host/shutdown` 的 `reason` 由 `shutdown_reason` 按 `Exit` 取（空闲 `idle`、待升级 `upgrade`，其余 `user_quit`）；host.log 记一行 `orderly exit`（`exit`、`reason` 字段）。监护的三种退出都只在没有运行中的回合时发生，不强制。
- `host/status.pending_upgrade` 取 `Admission`。
- 会话：句柄与 actor 多一个 `queued` 原子量，每步之后写入；`Activity` 多 `queued_messages`。

### 自检

- `SelfCheck`：启动时记下 `current_exe()` 和 `own_cdhash()`（后者失败就没有自检）；`check()` 比较 `file_cdhash(exe)`，读不到或不同就 `Watch::bundle_changed()`。
- 调用点：`Gateway::serve` 每次 accept 之后（`Gateway::check_self_with` 设一次），`driver::runtime::launch::start` 在 `cupper_sys::spawn` 之前（`launch::Context.self_check`）。
- **T129 接在哪里**：安装器在 `Host::start` 里 `let (watch, watching) = Watch::new();` 之后启动，拿 `watch` 的克隆，`Failed(BundleChanged)` 时调 `watch.bundle_changed()`；安装器忙闲由它补进 `Observed.installer_busy`（观察处现在固定为 `false`，注释写明）。

### 清理残留（`supervisor/reap.rs`）

- `previous_run`：读 `ended_at` 为空的 `adapters` 行，取各行的 `(pgid, start_us)` 和 `groups` 里的后代进程组，去重；`reap` 之后把这些行标为 `ended_at = 现在`、`exit_status = 'reaped'`。排在 `ledger` 之后、`maintenance.spawn` 之前。
- `reap(groups, clock, term_wait, poll)`：对仍是记录中那个的组发 SIGTERM，按 `Clock` 每 `poll` 看一次，到 `term_wait` 后对剩下的发 SIGKILL。两个时长 `reap_term_wait`（2 s）、`reap_poll`（50 ms）。T055 可以直接复用。
- 用例并进 IT-lifecycle 的启动用例（计划里清理残留本来属于 IT-lifecycle）：测试自己用 `setsid` 拉起两个进程组，一个当适配器、一个当忽略 SIGTERM 的后代，写进 `adapters` 行；宿主启动后前者死于 SIGTERM、后者死于 SIGKILL，行标为 `reaped`。

### 流订阅

`Host::start` 里调用 `routes.streams(streams, readers)`；IT-idle-exit 里订阅一次索引流。

### 用例与发现

- IT-idle-exit：缩短 `supervisor_idle`、`supervisor_observe`；连着时 900 ms 不退出；断开后退出码 0、`last_clean_exit_at` 已写、host.log 恰有一行以 `idle` 为原因的 `orderly exit`。
- IT-self-check：宿主从测试自己复制出的可执行文件运行；假 agent 的 `wait_cancel` 让回合一直运行；删掉复制件后新连接触发待升级；`session/create` 被拒（`pending_upgrade`），`host/status` 与索引流的 `host.updated` 都报待升级；中断回合后客户端收到 `host/shutdown{reason: upgrade}`，退出码 0。中断的回复与它引起的退出赛跑，用例不要求收到。
- 本机跑进程级用例前要先构建 `cupper-fake-agent` 和 `cupper-cli` 的 `cupper-elevate-probe`（`cargo test -p cupper-host` 不会构建它们）；旧的二进制会让无关的用例失败，失败的进程级用例会留下宿主进程。开发中这样留下的宿主（都在本 worktree 的 target 或测试临时目录里）已经逐个确认后用 SIGTERM 结束。

### 行数（不含 notes）

| 部分 | 新增 / 删除 | 估计 |
|---|---|---|
| 监护核心 | 约 200 / 0 | 195 |
| 监护 actor、宿主接线、会话闸门与排队数、索引、网关与拉起钩子、自检 | 约 330 / 33 | 370 |
| 清理残留（含时长与启动调用） | 约 75 / 0 | 130 |
| UNIT-orphan-policy | 193 / 0 | 155 |
| 进程级用例（IT-idle-exit、IT-self-check、启动用例里的清理） | 约 135 / 6 | 335 |
| **合计** | **933 / 33** | 约 1,230 |
