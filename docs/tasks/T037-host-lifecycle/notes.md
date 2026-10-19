# T037 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前的规模估计

按模块估计，行数指 rustfmt 展开后的行，含文档注释和空行。估计前读过 5.2.13、5.2.16、5.5.6、5.8.1、5.8.6、ADR-09，以及 `main.rs`、`startup/`、`store/`、`gateway/`、`ledger/` 的现有代码和 T020–T039 的笔记、交接事项。

### 实现

| 模块 | 内容 | 估计 |
|---|---|---|
| `cupper-sys` 新增 `runloop.rs` | 主线程的 `CFRunLoop`：加一个 version 0 的 `CFRunLoopSource`，`stop()` 从任意线程 signal 并唤醒，回调里 `CFRunLoopStop`。这样在 run 之前就调用 stop 也不会丢，空的 run loop 也不会立即返回 | 70 |
| `main.rs` 重组 | 发布构建一开始就重定向 stdio；读载荷的构建读完后重定向；多线程 tokio 运行时（worker 数 min(4, 核数)）；启动在运行时上完成，失败时直接返回退出码；成功后主线程跑 run loop，有序退出结束时由宿主停掉 run loop，再取出退出码 | +60 |
| 数据目录（`startup/`） | 不存在时以 0700 创建；已存在时检查是目录、属主是当前用户，再收紧到 0700；不满足以退出码 5 退出 | 45 |
| 宿主组装（启动第 3–10 步） | `host.lock`（3）；host.db：`open_writer` → `migrate` → 读 meta、epoch 判定、写 `last_start_at`、`boot_session_uuid`、`data_epoch` → 登记 `WRITER` → `Writer::spawn`（4、6、5）；raw.db：`open_writer` → `prepare_raw` → `RawWriter::spawn`；读池、`Streams`、`BlobStore`、账本与 `Routes`、网关配置（`dev_key` 带拉起方 ID、`harness_key`）、绑定套接字、`serve`；`Durations`、`Clock` 分发；各错误到退出码的映射 | 230 |
| 启动时清理状态 | 写库：`turns` 中 running 的改为 `interrupted`（`ended_at`、`stop_reason: host_restarted`），`interactions` 中 pending 的改为 `expired`（`reason: host_restarted`），`sessions.state` 从 running 改为 interrupted | 40 |
| 写线程死亡的通知 | `writer.rs` 给写线程挂一个随线程结束（含 panic 展开）而关闭的 oneshot，宿主等它 | 15 |
| 生命周期 actor（有序退出部分） | 邮箱；SIGTERM（tokio `signal` 特性）；`host/exit`；写线程死亡。有序退出的顺序：停止 accept → 向所有连接发 `host/shutdown` → 强制退出时中断运行中的回合（接口，T043）→ 按 ADR-09 关闭适配器（接口，T055）→ 写检查点（接口，T043、T034）→ 提交 `last_clean_exit_at` → abort 清理任务和账本清理任务 → 停原始帧写线程、写线程 → 删除套接字 → 释放锁 → 停主线程 run loop | 160 |
| 网关改动 | 已初始化连接的登记表（连接 ID → `Outbox` 和 `Opened`）：广播 `host/shutdown`、收尾时 `finish` 并在宽限内等发送完；`serve` 可停；连接建立与断开通知监护者（空闲计时、自检） | 70 |
| `host/status`、`host/exit` 的执行者 | `host/status`：版本、待升级、运行中的回合和待处理交互数（会话端口接上前为 0）、客户端（取自网关的登记表）、test-harness 构建的 pid；`host/exit`：不带 `force` 且有运行中的回合时 `conflict/turn_running`，结束账本 ticket，回复后有序退出；从 UNIT-unsupported-set 的允许表删两行 | 80 |
| 监护者的纯核心 | 输入：连接数变化、活动（回合、交互、排队、安装器）、父进程退出、待升级、退出请求、写线程死亡、空闲定时器；输出：设或取消空闲定时器、开始有序退出（原因、是否强制）；查询：是否接受新回合、新会话 | 130 |
| 监护者的其余接线 | 空闲定时器经 `Clock`、时长接 `test_config.durations`；`dev-auth` 构建的父进程监视（`ExitWatch` 放在独立线程，设好之后再核对一次父进程的唯一 ID）；每次新连接做自检，给拉起子进程前的自检留接口 | 120 |
| 自检 | 启动时记下 `current_exe` 和 `own_cdhash()`；比较时在阻塞线程里取 `file_cdhash(exe)`，不一致或读不到就进入待升级 | 45 |
| 按 `adapters` 表清理残留 | 读出未结束的实例，按 `pgid`、`start_us` 还原进程组，`with_descendants()` → 逐组 `Term` → 经 `Clock` 限时轮询 `is_alive()` → 对还在的组 `Kill`；写 `ended_at`；之后才 `Maintenance::spawn` | 85 |
| 审计落盘 | 正式格式：`event` 用 5.8.6 的名字（`conn.accepted`、`conn.rejected`、`conn.initialized`、`conn.closed`），`conn_id`、`principal`、`peer_json`（pid、pid version、签名结论或拒绝类别）、`detail_json`（关闭原因的 snake_case 码，如 `handshake_timeout`，以及时长等）；再加 `host.started`、`host.exited`、`host.pending_upgrade`；接收 channel 的任务经写线程插入 | 110 |
| `durations.rs`、`Cargo.toml`、`lib.rs` | 新时长：空闲退出、清理残留时 TERM 到 KILL 的宽限、轮询间隔；tokio 加 `rt-multi-thread`、`signal` | 20 |
| **实现小计** | | **约 1,280** |

### 测试与测试辅助

| 项 | 内容 | 估计 |
|---|---|---|
| run loop 单元测试 | 另一线程 stop；run 之前先 stop | 35 |
| 数据目录单元测试 | 新建为 0700；收紧权限；不是目录时拒绝 | 35 |
| 启动清理的单元测试 | 在迁移过的库上造 running 的回合、pending 的交互，断言改写结果 | 45 |
| 写线程死亡 | 操作 panic 后通知触发 | 20 |
| 清理残留 | 用真实进程组（新会话里的 `sh`）写进 `adapters`，断言被清理、`ended_at` 已写 | 55 |
| 审计格式 | 每种记录到行的映射，含 `handshake_timeout` | 60 |
| UNIT-orphan-policy 与核心的其余用例 | 孤立后不接受新回合，回合结束后退出；没有回合时立即退出；空闲计时随连接和活动重置；待升级 | 140 |
| IT-lifecycle 辅助 | 拉起宿主二进制（写载荷：`harness_key`、`test_config`）、等套接字可连、CLI 的 harness 客户端、等退出码、直接读写库 | 90 |
| IT-lifecycle 用例 | 启动前写库造数据 → `interrupted`、`expired`；`host/exit`：结果、`host/shutdown`、退出码 0、`last_clean_exit_at`；SIGTERM 同上；空闲退出；退出码 3、4、5、6；与真实宿主握手成功 | 235 |
| IT-self-check | 把宿主复制到临时目录后从那里启动；连接时自检通过；换掉磁盘上的文件后，新连接触发待升级并退出（`host/shutdown{upgrade}`） | 90 |
| **测试小计** | | **约 805** |

### 合计

**约 2,085 行**（实现约 1,280，测试约 805），超出约 1,500 的上限约 40%。即使测试压到 550 行、实现压到 1,100 行，也有约 1,650 行。按任务书和守则第 8 条，开工前停下，见下一节。

## 汇报与拆分建议

**方案 A：拆成两个任务（建议）。** 两半都能独立合并，main 上始终是一个能启动、能有序退出的宿主。

- **T037（约 1,210，关键路径）：启动串联与有序退出**
  - 实现（约 790）：run loop、`main` 重组与 stdio 重定向、数据目录检查、启动第 3–10 步与退出码、启动时清理状态（写库）、写线程死亡的通知、有序退出（SIGTERM、`host/exit`、写线程死亡三种触发）、网关的连接登记表与 `host/shutdown` 广播、`host/status`、`host/exit`。
  - 测试（约 420）：run loop、数据目录、启动清理、写线程死亡；IT-lifecycle 的"启动清理、`host/exit` 与 SIGTERM 的有序退出、退出码 3–6、与真实宿主握手"。
  - T034、T046、T090 需要的"真实宿主进程、`host/status`、`host/exit`"都在这一半里。
- **T037b（约 875）：监护**
  - 实现（约 490）：监护者的纯核心（空闲退出、孤立、待升级、是否接受新回合和新会话）、空闲定时器与连接事件的接线、`dev-auth` 构建的父进程监视、自检、按 `adapters` 表清理残留（之后才启动清理任务）、审计落盘与正式格式。
  - 测试（约 385）：UNIT-orphan-policy、IT-self-check、IT-lifecycle 的空闲退出、清理残留、审计格式。
  - 依赖 T037。T062 的 IT-host-orphaned 依赖它。

计划第 312 行说"IT-lifecycle 需要启动顺序、残留清理、空闲退出和有序退出同时存在"。方案 A 把有序退出留在 T037，空闲退出只是有序退出的又一个触发，放进 T037b 时复用同一条退出路径；残留清理和适配器关闭共用"TERM → 限时等待 → KILL"的例程，这个例程在 T037b 写出，T055 关闭适配器时复用。

**方案 B：不拆，上限放宽到约 2,100（硬停止线 2,200）。**

- 好处：一次把启动、监护和退出接好，少一个依赖节点。
- 代价：单个分支约 2,100 行，审阅面大；T037 的松弛约为 2，T034 要多等 T037b 那部分。

## 待裁定（开工前请编排者一并定）

1. **启动清理只写库**（任务书第 6 条）：`turns`、`interactions`、`sessions` 三张表按上表改写，不追加事件。会话流里于是还没有 `turn.completed{interrupted}`、`interaction.expired` 事件，视图和行不一致，要由 T043 的会话 actor 载入会话时补写事件来对齐。这样分工是否可以？
2. **`sessions.state`**：运行中的回合改为 `interrupted` 时，是否一并把会话的 `state` 从 `running` 改为 `interrupted`？我倾向一并改，否则索引（T034）载入时会看到"运行中"的会话。
3. **数据目录已存在但权限不是 0700**：我倾向在属主是当前用户时直接收紧到 0700，不报错（`mkdir` 建出的 `--data-dir` 默认是 0755）；属主不对、不是目录才以退出码 5 退出。
4. **待升级的"空闲时退出"**：我理解为"没有运行中的回合、待处理的交互和排队消息"就立即有序退出，不看客户端连接，也不等 10 分钟。否则 UI 一直连着时，旧宿主永远不退。
5. **`host/shutdown` 的 `reason`**：SIGTERM 用 `user_quit`（Q22 里桌面端发 SIGTERM 就是用户要重启宿主）；孤立后退出也用 `user_quit`（拉起方已经退出）；写线程死亡用哪一个？协议里只有 `user_quit`、`upgrade`、`idle` 三个取值，我不打算在本任务里加取值，写线程死亡也用 `user_quit`，并以退出码 1 退出。
6. **写线程死亡后的有序退出**：写线程已经不在，`last_clean_exit_at` 写不了，下次启动按非正常退出判定 epoch。其余步骤照做，退出码 1。
7. **`host/status` 的 `clients`**：取自网关新增的连接登记表（广播 `host/shutdown` 本来就要它），由 `Opened` 推出 `ClientInfo`。这部分和 T034 的 `clients.updated` 重复，T034 可以直接复用这个推导函数。或者等 T034 接上前先回空列表？
8. **`host/exit` 的结果先于退出**：分发任务在回复的 future 完成后才把结果推进发送队列，监护者可能先推 `host/shutdown`。两者都在关闭连接（最后一步，先 `finish` 再在宽限内等发送完）之前进入队列，客户端一定能收到结果，只是先后不保证。是否要求"结果先于 `host/shutdown`"？要的话得在分发里给 `host/exit` 加一个"回复之后"的钩子。
9. **自检对象**：启动时 `current_exe()` 规范化后的路径，与 `own_cdhash()` 比较 `file_cdhash`。`own_cdhash()` 本身失败（未签名的构建）时不做自检。arm64 上链接器会做 ad-hoc 签名，CI 上能跑。
10. **清理残留的时长**：没有 ACP 连接，跳过 ADR-09 的 `session/close`、关 stdin 两步，直接 TERM → 2 s → KILL；轮询间隔 50 ms，都登记进 `durations.rs`。资源采样记下的后代进程组还没有表，T055 加表时一并纳入。
11. **网关夹具的审计行**：保留现在的 `Debug` 文本（T039 的用例按子串断言），正式格式只用于 `audit` 表。

## 拆分之后（计划 v1.3.10）

编排者采用方案 A：本任务收窄为启动串联与有序退出；监护者纯核心、父进程监视、自检、清理残留、审计落盘归 T059。11 项待裁定全部认可，写进了 `02-host.md` 5.2.13 末尾"T037 开工前的裁定"。

### 收窄后的估计

| 模块 | 估计 |
|---|---|
| `cupper-sys` run loop（含测试 35） | 105 |
| `main` 重组、stdio 重定向 | 60 |
| 数据目录（含测试 35） | 80 |
| 宿主组装（启动第 3–10 步、退出码） | 230 |
| 启动时清理状态（含测试 45） | 85 |
| 写线程死亡的通知（含测试 20） | 35 |
| 有序退出（监护者 actor：SIGTERM、`host/exit`、写线程死亡；给 T043、T055、T034 的退出接口） | 160 |
| 网关：连接登记表、`host/shutdown` 广播、停止 accept、收尾关闭 | 70 |
| `host/status`、`host/exit` | 80 |
| `durations.rs`、`Cargo.toml`、`lib.rs`、UNIT-unsupported-set | 20 |
| IT-lifecycle（辅助 90，用例 195） | 285 |
| **合计** | **约 1,210** |

### 实际行数

按提交统计（`git diff --numstat`，只算 `crates/`，不含 `Cargo.lock`）。

| 提交 | 内容 | 增 | 删 | 估计 |
|---|---|---|---|---|
| `feat(sys): a run loop …` | `cupper-sys` 的 `RunLoop`、`RunLoopStop`（含测试 2 个） | 130 | 0 | 105 |
| `feat(host): tell when the store writer thread has died` | `WriterThread::ended`（含测试 1 个） | 34 | 2 | 35 |
| `feat(host): gateway client table, …` | `gateway/clients.rs`、`broadcast`、`close_all`、读循环里的 `closing`、关闭原因 `HostExiting`（含测试 1 个） | 224 | 17 | 70 |
| `feat(host): keep the data directory private …` | `startup::prepare_data_dir`（含测试 1 个） | 54 | 1 | 80 |
| `feat(host): assemble the host, …` | `supervisor/`（组装、清理状态、有序退出、`host/status`、`host/exit`）、`main` 重组、UNIT-unsupported-set 的两行 | 584 | 25 | 570 |
| `test(host): IT-lifecycle …` | `tests/lifecycle.rs` 三个用例 | 273 | 0 | 285 |
| `test: register the T037/T059 split …` | 从 main 拣来的 `tests/ids.toml`（见下文） | 6 | 1 | — |
| **合计** | | **1,305** | 46 | 1,210 |

网关超出估计最多：起初只算了"登记表加广播"，实际还要让读循环能被关闭（`closing`）、统计存活的连接任务（`Live`）、由 `Opened` 推出 `ClientInfo`。启动清理没有单独写单元测试，由 IT-lifecycle 在进程级覆盖，省下约 45 行。

### 实现要点

- **`main`**：
  - 先解析命令行，处理 `--build-info`；
  - 发布构建随即把 stdio 指向 `/dev/null`；`dev-auth` 构建接着核对拉起方；
  - 建多线程运行时，worker 数为 min(4, 核数)；
  - 读载荷（经 `SystemClock` 计 2 s）、装故障点，读载荷的构建这时才重定向 stdio；
  - `Host::start` 在主线程上 `block_on`，失败时直接按退出码返回；
  - 成功后 `Host::run` 交给 worker，主线程跑 `RunLoop`；`run` 结束时经 `StopOnDrop` 停掉 run loop，panic 时也停；
  - 最后 `shutdown_background`。
- **启动顺序**（`Host::start`）：
  1. 数据目录（第 3 步）；
  2. `host.lock`（第 4 步）；
  3. host.db：`open_writer` → `migrate` → 在同一个事务里做 epoch 判定、写 `last_start_at` 等 meta、改写上次留下的行（第 5、6 步，`supervisor/recover.rs`）；
  4. 在静止屏障里登记三种参与者 → 写线程；raw.db → 原始帧写线程；读池、`Streams`、`BlobStore`、Maintenance；
  5. 账本另开 `open_reader` 装载，接着启动清理任务；
  6. `Routes::new(ledger)` 加上 `host/status`、`host/exit`，再建网关（`dev_key` 带拉起方 ID）；
  7. 装 SIGTERM 处理，绑定套接字（第 8 步），开始 accept（第 10 步）。
- **退出码**：`host.lock` 被占为 3；`TooNew` 为 4；`Failed` 为 6；数据目录、打开数据库、`Backup`、`Unreadable`、`BadVersion`、meta 值无效、账本装载失败都是 5；读不到启动会话 UUID、起不了线程或信号处理为 1。
- **有序退出**（`Host::exit`）：
  1. abort accept 任务；
  2. 向已初始化的连接广播 `host/shutdown {user_quit, restart_expected: false}`；
  3. 各部件依次 `stop(force)`，现在为空；
  4. abort 并等待清理任务和账本清理任务；
  5. 提交 `last_clean_exit_at`，写线程死亡时跳过，退出码改为 1；
  6. 在阻塞线程里依次停原始帧写线程和写线程；
  7. `close_all`：每条连接先发完队列（宽限 `gateway_close_grace`）再关；
  8. 删除套接字，最后随 `Host` 释放锁。

  与任务书列的顺序有一处不同：清理任务放在写 `last_clean_exit_at` 之前停。这样它在途的写入不会排在 `last_clean_exit_at` 之后，`last_clean_exit_at` 才是写线程停下前真正的最后一个操作。连接放在最后关：`host/exit` 的回复由分发任务在回复 future 完成后才推进队列，晚关可以保证它在连接关闭前进入队列（裁定第 8 条）。
- **强制**：只有不带 `force` 的 `host/exit` 不算强制；SIGTERM 和写线程死亡都按强制处理（Q22：桌面端发 SIGTERM 时，运行中的回合会被中断）。
- **`Part`**：会话（T043）、驱动（T055）、索引（T034）各实现一个，在 `Host::start` 里按这个顺序放进 `parts`。`activity()` 汇总后给 `host/status` 和 `host/exit` 的 `conflict/turn_running`；`stop(force)` 是它们在有序退出里的那一步。

## 给其他任务的接入点

- **T043（会话）**：
  - 实现 `supervisor::Part`：`activity()` 给出运行中的回合和待处理的交互；`stop(force)` 在 `force` 时把运行中的回合结束为 `interrupted`，并写各会话的检查点。放进 `Host::start` 的 `parts`，排第一个。
  - `Host` 里的 `streams` 现在标着 `#[expect(dead_code)]`，接上后删掉这个属性。
  - 启动清理只改了行：`turns.status = interrupted`（`ended_at` 为启动时间）、`interactions.status = expired`（`reason = host_restarted`）、`sessions.state = interrupted`。载入会话时，要补写 `turn.completed{interrupted}`（`error_code: host_restarted`）和交互的 `expired` 事件；有排队消息的会话是否暂停队列（`turn_interrupted`），也在这时决定。
  - 改用 `Routes::start` 的完整形式时，把 `Control::route` 一并调用。
- **T034（索引）**：
  - `Part` 排第三个，`stop` 写索引流的检查点；
  - `clients.updated` 用 `Gateway::clients()`，它由 `gateway/clients.rs` 的登记表推出；
  - 启动第 7 步"加载索引流"放在 `Host::start` 里绑定套接字之前。
- **T055（驱动）**：
  - `Part` 排第二个，`stop` 按 ADR-09 关闭适配器；
  - `raw.sink()` 在 `Host::start` 里取，交给线路分接。
- **T059（监护）**：
  - 空闲退出、孤立、待升级可以作为新的 `Exit` 取值，从 `Host::run` 的等待里出来，复用 `Host::exit`。`host/shutdown` 的 `reason`：孤立按裁定用 `user_quit`，空闲用 `idle`，待升级用 `upgrade`；`Host::exit` 现在只发 `user_quit`，要改成按 `Exit` 取值。
  - 审计：`Gateway::new` 返回的接收端现在在 `Host::start` 里丢弃（`_audit`），由 T059 接到写线程。新的关闭原因 `CloseReason::HostExiting` 建议记为 `host_exiting`。
  - 清理残留要排在 `maintenance.spawn` 之前。
  - `host/status` 的 `pending_upgrade` 现在固定为 `false`（`supervisor/control.rs`）。
  - 自检要挂在每次新连接上，可以加在 `Gateway::serve` 里 accept 之后。
- **T047（测试接口）**：
  - `Quiescence` 在 `Host::start` 里是局部变量；`test/clock_advance` 要用它，还要用到 `injected()` 建的 `ManualClock`，接上时把两者留在 `Host` 里；
  - 账本的清理任务（`sweeps`）还没有登记进静止屏障。
- **T028（日志）**：重定向之后的启动失败原因现在写进 `/dev/null`（`main` 的 `fail`），有了 host.log 后改写日志。

## 其他

- **`tests/ids.toml`**：main 在 `486d67f` 之后提交了 `6d214fd`（把 IT-self-check、UNIT-orphan-policy 挪到 T059 一段，并登记 IT-idle-exit）。本分支的文档已经带上 IT-idle-exit，缺这条登记会让 `testdata-scan` 失败。所以用 `cherry-pick -x` 把它拣了过来，改动与 main 完全相同，合并时不会冲突。
- **新依赖**：tokio 加了 `rt-multi-thread` 和 `signal` 两个特性，后者带来 `signal-hook-registry` 和 `errno`，两者都是 MIT OR Apache-2.0。
