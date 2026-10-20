# T062 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前的规模估计

估计前读过：
- 任务书；`handoff-notes.md` 第一列含 T062 的行（T019 的 `Routes::streams`、T055 的假 agent 路径与参数、T068 的 `tests/common/`）；
- 设计 5.9.8 的"数据代次"一条，5.2.8 的 `data_epoch`、5.2.9 的状态与通知意图、5.3.3 的同步规则；计划里 T062 一行和 T037、T059、T089 涉及 T062 的条目；
- `spikes/SP-4a-host-skeleton/crates/cupper-host/tests/e2e.rs`（只作蓝本）；
- 代码：`store/epoch.rs` 与 `supervisor/recover.rs`、`gateway/history.rs` 与 `store/history.rs`、`gateway/outbox.rs` 与 `send.rs`（慢客户端的重置）、`ledger/`（`replayed`）、`index/status.rs`（意图）、`supervisor/watch.rs`（孤立）、`cupper-cli` 的 `Client` 与 `Mirror`、`gateway/testkit.rs` 的原始客户端与 `key_auth`、`tests/gateway.rs` 的 `start_orphaned`、假 agent 的脚本命令。

行数指 rustfmt 展开后的行，含文档注释和空行；按净增计，只算 `crates/`。

### 夹具

| 项 | 内容 | 估计 |
|---|---|---|
| `tests/common/` | 挪进来两边都用的 `typed`、读会话流事件的 `events`、`respond`（`interaction_trust.rs` 改为用共用的，对冲）；宿主的 `kill -9`；按 `adapters` 表记下适配器进程组及其后代、结束时按 pgid 和启动时间核对后回收的守卫 | 净增约 50 |
| 本文件的辅助 | 带镜像的客户端：harness 客户端加会话、索引两个 `Mirror`，`pump_until` 收消息、喂镜像、收 blob 分片；某回合的文字、回合状态；读 `replayed` 的原始结果；有序停与重启 | 120 |

### 用例

| 用例 | 内容 | 估计 |
|---|---|---|
| IT-gw-sync | SP-4a 1–7 加适配器隔离，按条拆成 8 个测试，名字都以 `it_gw_sync_` 开头。逐条见下面的对照表 | 1 多客户端 45；2 先到先得 45；3 续传 40；4 慢客户端 60；5 `kill -9` 恢复 70；6 幂等 45；7 帧上限 40；隔离 20；**小计 365** |
| IT-NFR2-epoch | 一个数据目录上连续重启：`kill -9` 同一次开机不加；正常退出加换开机不加；`kill -9` 加换开机加 1（记下"多一次全量快照"）；同 epoch 游标超前走 `cursor_ahead`；模拟断电：运行中 `VACUUM INTO` 拍下 seq M 的副本，跑到 N 后正常停，换回副本、换开机 UUID 重启，旧游标走 `epoch_changed` 快照，之后从 M+1 起重用的 seq 被 `Mirror` 整体接受，不报重复 | 110 |
| IT-history-from-host | 22 个回合（超过 20 个回合的窗口），有序重启后，新客户端（没有任何缓存）订阅得到窗口，`history/page` 不带游标取到更早的回合；窗口加历史等于重启前的实时镜像；全程没有拉起适配器（历史不来自 agent 的回放，K8） | 60 |
| IT-notification-intent | 手动时钟。需要处理、回合完成、出错各产生一条意图；同一段待处理期间第二个请求不再产生；100 ms 合并内已答复的 `needs_action` 不写；中断的回合不产生；CLI 的在场压不掉意图；`dev-auth` 构建里用 `dev_key` 连上的 desktop_ui 在前台看着时不产生，离开前台后恢复 | 90 |
| IT-host-orphaned | `dev-auth` 构建。经 `sh` 拉起宿主（`sh` 用 `exec` 换成 `sleep`，测试结束它就等于"父进程退出"）；回合停在待批准时结束父进程：`session/send`、`session/create` 被拒（`unavailable`），回合不受影响；批准后回合结束，宿主以孤立有序退出（退出码、审计 `host.exiting {reason: orphaned}`、`last_clean_exit_at`）。宿主不是测试的子进程：按 pid 和启动时间核对后才发信号的守卫，用 `ExitWatch` 等退出 | 90 |
| `Cargo.toml` | 新测试目标 `gateway_sync` 要求 `test-harness`；两个只在 `dev-auth` 构建里有意义的测试用 `cfg(feature = "dev-auth")` | 5 |
| **用例与夹具合计** | | **约 890** |

按以往偏差乘 1.15，约 **1,020 行**。

### 就地修

开工前没有已知缺陷。过程中发现的按计划 1.2 的上限（单处不超过 100 行、不改接口）就地修，在这里和汇报里单列；预留约 150 行。

### 合计

约 1,020 行加就地修的预留，约 1,170 行，在约 1,300 的上限之内，开工。每完成几个用例统计一次。

## 统计（`git diff --numstat`，只算 `crates/`，相对开工时的 main）

| 项 | 增 | 删 | 估计 |
|---|---|---|---|
| `tests/common/mod.rs`（`typed`、`events`、`respond`、`audited`、`refused` 从 `interaction_trust.rs` 挪来；按载荷和自选的 `Command` 拉起宿主、按选项连接、假 agent 的数据目录和 `test_config` 分开以便重启） | 100 | 12 | — |
| `tests/interaction_trust.rs`（改用共用的） | 1 | 64 | — |
| 夹具净增 | | | 25（估计约 50） |
| `tests/gateway_sync.rs`：共用辅助（`Peer`、回合状态与文字、视图比较） | 173 | 0 | 120 |
| IT-gw-sync 八个测试（含各自的辅助：`put`、`pending`、`follow`、`Leftovers`、`SendRaw`） | 510 | 0 | 365 |
| IT-NFR2-epoch（含 `boot`、`stop`、`kill9`、`back`） | 125 | 0 | 110 |
| IT-history-from-host | 48 | 0 | 60 |
| IT-notification-intent（含 `intents`、`turn`、`presence`） | 102 | 0 | 90 |
| IT-host-orphaned（含 `Adopted`） | 102 | 0 | 90 |
| `Cargo.toml` | 5 | 0 | 5 |
| 就地修 1：宿主停适配器时不写 `agent_restarted` | 8 | 0 | — |
| 就地修 2：孤立与待升级的退出等一次新的观察 | 10 | 4 | — |
| **合计** | **1,188** | **80** | 净增 **1,108**（估计约 1,170，含就地修预留） |

IT-gw-sync 比估计多约 200 行，主要是慢客户端要用原始套接字自己核对 seq、`kill -9` 用例的进程组守卫、读 `replayed` 要另定义一个方法类型；其余用例和估计相近。

## 就地修

### 1. 宿主有序退出时，每个连过适配器的会话都多出一条 `agent_restarted` 提示

- 现象：IT-history-from-host 有序重启后，新客户端的窗口加历史比重启前的实时视图多一条 `notice`（`agent_restarted`），落在 `tail` 里。修之前 5/5 失败。另用一个临时用例确认：停宿主之前日志里没有这条提示，停之后有。
- 原因：有序退出先停会话（`Stop`），再停适配器。会话 actor 收到 `Stop` 后照常处理适配器事件，把自己宿主停掉的适配器当成崩溃（K7），写了 `agent_restarted`。SIGTERM、`host/exit`、空闲、孤立、待升级的退出都会这样，用户下次打开会话会看到一条没有发生过的"agent 已重启"。
- 修法：actor 收到 `Stop` 后记下 `stopping`，之后的 `AdapterExited` 不再交给回合机。强制退出时回合已经由 `HostExit` 以 `interrupted` 结束；非强制退出只在没有回合运行时发生（`host/exit` 有回合时返回 `turn_running`，监护的退出等回合结束），所以不会漏掉回合的结束。
- 行数：`orchestrator/session/actor.rs` 增 8。

### 2. 孤立（和待升级）的退出用的是上一次观察，会在回合进行中退出

- 现象：IT-host-orphaned 手动时钟下，父进程退出后宿主一看到"孤立"就有序退出了，当时回合正等着批准；回合没有结束事件（修 1 之前会被当成适配器崩溃以 `failed` 结束）。修之前 3/3 失败。
- 原因：`Watcher::observe` 先把 `Orphaned` 交给 `WatchCore`，再交新的观察；`Orphaned` 当场用上一次的观察做决定。真实时钟下那次观察最多是 `supervisor_observe`（1 s）之前的，这 1 s 内开始的回合看不到，宿主会在它进行中做非强制退出；手动时钟下上一次观察来自启动时。安装器经收件箱送来的 `BundleChanged` 也一样（上一次观察最多 1 s 旧）。
- 修法：`WatchCore` 收到 `Orphaned`、`BundleChanged` 时，在关上新会话和新回合的同时丢掉上一次观察，退出等下一次观察再定；`observe` 本来就先送 `Orphaned` 后送新观察，所以孤立在同一次观察里就能定。UNIT-orphan-policy 里三处"当场退出"的表改为"下一次观察时退出"，另加一条"关之前的观察不算"。剩下的窗口：关闭之前已经受理、但会话 actor 还没把 `running` 置上的回合（读计数与受理之间的微秒级窗口），在汇报里提出。
- 行数：`supervisor/watch.rs` 增 4，`supervisor/watch/tests.rs` 增 10 删 4。

## 过程中发现的范围外问题（未修）

1. **超过一帧的工具输出会让会话无法订阅**（等 T175）。`tool_done t1 500000` 时宿主写出一条约 500 kB 的 `item.completed`。在线的客户端收到 `stream/reset {rebuilt}`，重订之后再也收不到这个会话；新订阅的客户端收到订阅结果后连接被关（`Eof`）。原因是"超过内联上限转 blob"（设计 5.3.1、协议 `output_state`）归 T175，还没合并；宿主里 `MAX_INLINE_ESCAPED` 没有调用方。所以 SP-4a t1 的"40,000 字节工具输出是 blob"、t7 的"500 kB 工具输出是 blob"没有迁：t1 改用 `text_blob` 验证跨客户端的 blob 传输，t7 的工具输出降到 40,000（仍是内联）。建议 T175 的 IT-orch-long-output 带上一条进程级的"500 kB 工具输出后新客户端能订阅、帧不超限"，或在 T175 合并后补到本文件的 t1、t7。
2. **`flood_mixed` 每个条目约 3.5 ms**：500、1,000、2,000 个交替的消息与思考分别用 1.7 s、3.5 s、7.0 s，与每块长度无关（`flood_mixed 2000 10` 也是 6.3 s），线性。SP-4a 的 `flood_mixed 6000 200` 在这里要 20 s 以上。慢客户端的用例因此改用 `flood 5000 1000`（5 MB，一个条目，约 2.3 s）。是否达标归 NFR-1 的任务（T091）判断。
3. 一个回合里 `session.capabilities` 有时连写两条（同一次附加），条数因此不固定；IT-NFR2-epoch 原先假设"丢失后重跑的回合事件数不少于原来"，已改为只断言从 M+1 起重用。
4. `lifecycle.rs` 的 `it_gw_blob_put_and_get_through_the_gateway` 在机器高负载时（其他 agent 同时编译、跑测试）失败过一次：SIGTERM 后 30 s 内宿主没有退出。之后单跑 3 次、整个目标连跑 3 次都通过，没有复现，未深查。

## SP-4a 端到端测试与新用例的对照

| SP-4a | 新用例 | 迁过来的断言 | 设计后来改过、以设计为准的地方 |
|---|---|---|---|
| t1 两个客户端看到同一条流；晚来的快照一致；大工具输出是 blob | `it_gw_sync_two_clients_see_the_same_stream` | 两个同时订阅的客户端会话视图、索引里的会话摘要相同；晚来的客户端快照与之相同 | 客户端是 CLI 库的 `Mirror`。"大工具输出转 blob"归 T175，未合并（见范围外问题 1）：改为一个客户端 `blob/put` 100,000 字节、以 `text_blob` 发出，另一个客户端在 `user_message` 里看到同一个引用并用 `blob/get` 取回同样的字节 |
| t2 先到先得；只读身份、`allow_always`、过期摘要先被拒 | `it_gw_sync_first_answer_wins` | 两个连接同时回答，恰好一个成功，另一个 `conflict/already_resolved`；两边视图相同 | 只读的 `cli` 身份回答被拒（`forbidden`）。claude 形态的选项是 `allow-once`；`allow_always` 一条不再适用（选项随 agent 而定，T068 的用例覆盖了选项校验）；错摘要（`digest_mismatch`）由 T068 的 IT-interaction-expire 覆盖 |
| t3 断线重连回放无缺口；游标早于保留的日志走快照；协商了合并窗口的客户端实时收到区间事件 | `it_gw_sync_reconnect_replays_without_gaps` | 离开期间的 3,000 多条增量从游标回放（`replay`），镜像逐条核对 seq，视图与在线的相同，回放里有区间事件；`coalesce_ms: 200` 的客户端实时收到区间事件 | "早于保留的日志走快照"不在进程级做：保留规则是设计值（10,000 条、7 天），不可注入，手动时钟推进 7 天要走几十万次观察定时器；T019 的 IT-gw-subscribe（`it_gw_subscribe_cursor_expired_once_the_log_is_pruned`）用注入的保留规则覆盖。进程级的快照路径由 IT-NFR2-epoch 的 `epoch_changed`、`cursor_ahead` 覆盖 |
| t4 不读的客户端不拖慢 agent；它看到重置并从游标追上 | `it_gw_sync_slow_client_does_not_block_the_agent` | 同一回合有无停读客户端的耗时（不超过基线 3 倍加 2 s）；停读的客户端恢复后先按序收到套接字里的事件，再收到重置，带游标重订走 `replay`，seq 连续追到头 | 上限是 `max_queue_bytes`（4 MiB，不可注入），重置原因 `slow_consumer`；脚本从 `flood_mixed 6000 200` 改为 `flood 5000 1000`（见范围外问题 2）；CLI 库的读任务总在读，停读的客户端用 testkit 的原始套接字 |
| t5 `kill -9` 恢复：残留回收、回合中断、请求过期、下一条消息续上原会话 | `it_gw_sync_kill9_recovery_and_resume` | 杀掉宿主后孙进程还在，新宿主启动后整组被回收；带游标重连走 `replay`；对过期请求作答被拒（`expired`）；回合 2 是 `interrupted`、请求 `expired`；之后续上同一个 agent 会话（`resumed=true`） | 补写在会话载入时发生（T089，作答触发载入）；重启后队列以 `turn_interrupted` 暂停，排着的消息要 `session/queue_resume` 才发（SP-4a 是直接发新消息）；索引里没有 `interrupted` 状态了，断言为 `unread`；残留按 `adapters` 表的 pgid 和启动时间核对 |
| t6 同一 `command_id` 重试不重复执行（重复、两个连接并发、重启后） | `it_gw_sync_command_retries_are_idempotent` | 重复发送得到同一结果并带 `replayed: true`；两个连接并发发送，结果相同、恰好一个带 `replayed`；同 ID 不同参数 `idempotency_mismatch`；有序重启后仍回放原结果；最后只有 3 个回合、3 条用户消息 | 结果没有 `turn_id` 时（排队）比 `queue_id`；并发的重复命令等第一次的结果（账本的 `Repeat::Wait`） |
| t7 帧上限：3 MB 消息拆成增量、500 kB 工具输出转 blob、100 kB 标题截断、快照分页；客户端拒发超限帧 | `it_gw_sync_frames_stay_within_limit` | 客户端拒发 100,000 字符的消息（`Encode`）；3 MB 消息完整到达；100,000 字符的标题截到 `max_label_chars`；晚来的客户端分页快照与在线视图相同，过程中没有连接因帧超限被关 | 500 kB 工具输出转 blob 归 T175（见范围外问题 1），工具输出降到 40,000 |
| 适配器只拿到 stdio 和白名单环境 | `it_gw_sync_adapter_gets_only_stdio_and_allowed_env` | 宿主开着库、套接字和三个连接，适配器只继承 fd 0–2；宿主环境里的 `CUPPER_*` 和白名单外的变量不传 | `PATH` 来自登录环境（`login_env`），宿主自身的不传 |
| protocol_errors | 不迁移 | — | 握手顺序、未知方法、只读身份建会话由 T033 的网关层用例覆盖，单实例由 T037 的 IT-lifecycle 覆盖 |

## 其余四个用例

- IT-NFR2-epoch：设计 5.9.8 的 ①（进程级走了"`kill -9` 同一次开机不加""正常退出加重启不加""`kill -9` 加重启加 1"三格，纯函数的全表由 `unit_epoch_decision_*` 覆盖）、②（断电副本用运行中的 `VACUUM INTO` 拍下，副本里的 `last_start_at` 晚于 `last_clean_exit_at`，正是异常退出）、③、④ 都在一个用例里。`host-client`（TS）的整体重置由它自己的 `sync.test.ts` 覆盖，本用例只跑 Rust 的 `Mirror`。
- IT-history-from-host：22 个回合，窗口 20 个（3–22），`history/page` 不带游标从第 2 个往前取到第 1 个；`adapters` 表仍只有第一次运行的一行。
- IT-notification-intent：手动时钟，`notifier` 不涉及（系统通知归 T150）。"同一段待处理期间第二个请求不再产生"在进程级造不出（假 agent 的请求是一个接一个的），由 UNIT-index-intent 覆盖。
- IT-host-orphaned：只在 `dev-auth` 构建里编译。
