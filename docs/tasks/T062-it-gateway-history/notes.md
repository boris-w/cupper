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

## SP-4a 端到端测试与新用例的对照

开工前的计划，做完后按实际改。

| SP-4a | 新用例 | 设计后来改过的地方（以设计为准） |
|---|---|---|
| t1 两个客户端看到同一条流 | `it_gw_sync_two_clients_see_the_same_stream` | 客户端是 CLI 库的 `Mirror`；大工具输出的阈值是 `max_inline_bytes`（16,000）；blob 经 `blob/get` 分片取（T029） |
| t2 先到先得 | `it_gw_sync_first_answer_wins` | 回答带 `request_digest`；只读的 `cli` 身份被拒 |
| t3 断线重连回放无缺口 | `it_gw_sync_reconnect_replays_without_gaps` | "游标早于保留的日志走快照"不在进程级做：保留规则是设计值（10,000 条、7 天），不可注入；这条路径在 T019 的 IT-gw-subscribe 里用注入的保留规则覆盖。进程级的快照路径由 IT-NFR2-epoch 的 `epoch_changed`、`cursor_ahead` 覆盖 |
| t4 慢客户端不拖慢 agent | `it_gw_sync_slow_client_does_not_block_the_agent` | 积压上限是 `max_queue_bytes`（4 MiB），超了发 `stream/reset {reason: slow_consumer}`；不读的客户端用原始套接字（CLI 库的读任务总在读） |
| t5 `kill -9` 恢复与续上 | `it_gw_sync_kill9_recovery_and_resume` | 补写在会话载入时发生（T089）；重启后队列一律以 `turn_interrupted` 暂停，排着的消息要 `session/queue_resume` 才发；索引里没有 `interrupted` 状态了，回合结束后是 `unread` |
| t6 命令重试幂等 | `it_gw_sync_command_retries_are_idempotent` | 结果里的 `replayed: true` 由账本加；并发的重复命令等第一次的结果（`Repeat::Wait`） |
| t7 帧上限 | `it_gw_sync_frames_stay_within_limit` | 标签上限 `max_label_chars`；客户端拒发超过上限的帧 |
| 适配器只拿到 stdio 和白名单环境 | `it_gw_sync_adapter_gets_only_stdio_and_allowed_env` | 无 |
| protocol_errors | 不迁移 | 握手顺序、未知方法、单实例已由 T033 的网关层用例和 T037 的 IT-lifecycle 覆盖 |
