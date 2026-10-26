# T069 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前读到的现状

- **已登记的故障点**只有两个：`store.migration_committed`（两个迁移脚本之间）、`store.before_publish`（追加已提交、流还没应用和扇出）。T024 交接里的 `store.before_commit`、`store.after_commit` 还没登记。
- **设计 5.9.8 的故障点**里属于 M1a、不涉及 git 的有：
  - 命令受理后、事务提交前；
  - 提交后、扇出前；
  - `session/create` 提交后、第一个回合开始前；
  - 两个迁移脚本之间；
  - 写检查点和回合快照的过程中。

  `worktree add` 进行中、`pending_external` 前后归 T121、T124。
- **检查点和回合快照**与它们所在批次的事件在同一个写事务里（`StreamWriter::append` 交给写线程的一个操作）。
- **`session/create`** 的账本记录随第一批事件一起提交；第一个回合由 actor 启动。重启后由 `SessionManager::reopen` 载入，并重开准备阶段的回合（T169）。
- **Seatbelt**：`sandbox-exec` 支持 `(deny network-outbound (remote ip) (with send-signal SIGKILL))`，已在本机实测：沙箱里的 `nc -u` 连 192.0.2.1 时被 SIGKILL，退出码 137。子进程继承沙箱，所以宿主或假 agent 只要往外连一次就会被杀，用例必然失败；Unix 套接字不受影响。测试用的宿主不在 App 包里，不会启动运行时安装器；登录环境也是注入的，不跑登录 shell。
- **准备阶段那条**：会话夹具（T043）的进程内 agent 由测试决定何时应答附加，可以让第一个回合停在准备阶段。让 `turn.started` 已写出、闸门关着的确定做法：先信任，再改项目配置，然后建会话；附加前的重扫发现变化就关闸门。信任恢复后，`session/queue_resume` 触发重扫，闸门打开，回合发出。

## 故障点清单（计划）

| 故障点 | 位置 | 对应设计 5.9.8 |
|---|---|---|
| `store.migration_committed` | 已有 | 两个迁移脚本之间 |
| `store.before_commit` | 新增：`Batcher::commit` 的 COMMIT 之前 | 命令受理后、事务提交前 |
| `store.after_commit` | 新增：`Batcher::flush` 提交之后、回复回执之前 | 提交后、扇出前（写线程一侧） |
| `store.before_publish` | 已有 | 提交后、扇出前（流一侧） |
| `store.snapshot_written` | 新增：追加操作里回合快照已写、检查点未写 | 写检查点和回合快照的过程中 |
| `session.created` | 新增：`session/create` 的批次提交之后、actor 启动之前 | `session/create` 提交后、第一个回合开始前 |

## 断言库

每个故障点重启之后都检查：
- `PRAGMA integrity_check` 为 `ok`；
- 每条流在保留范围内的 seq 连续；
- 崩溃前客户端看到的事件，是崩溃后日志的前缀（同 seq、同内容）；
- 在途命令用同一个 `command_id` 重试：已提交的就回放，没提交的就重新执行；之后这条消息在会话里恰好出现一次；
- 第一条消息不丢：会话恢复后，`session/create` 的消息恰好出现一次，它的回合最终结束。

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行，按净增计。以往的估计偏低，这里乘 1.4。

| 用例 / 模块 | 内容 | 估计 |
|---|---|---|
| 故障点登记 | 四个新点的登记与标记 | 25 |
| 断言库与崩溃夹具 | 收事件的客户端、重启、五项检查、在途命令重试 | 130 |
| IT-NFR9-failpoints-core | 每个点按 hit = 1, 2, 4, 8, … 扫，直到剧本跑完没有触发；迁移点单独起 | 90 |
| IT-NFR9-crash-under-load | 高负载输出中按固定间隔反复 `kill -9`，每次重启后做断言库检查 | 60 |
| IT-six-sessions-shared-adapter | 6 个会话同时 prompt，各自带标识；只起一个适配器；互不串扰、各自完整 | 50 |
| IT-host-egress-seatbelt | 出站即杀的 profile；正对照；普通会话完成、宿主正常退出、适配器没被杀 | 50 |
| 准备阶段那条 | 会话夹具里：第一个回合停在准备阶段，信任恢复后发出 | 50 |
| 夹具挪动 | `Leftovers` 等挪进 `tests/common/` | 20 |
| `Cargo.toml` | 新测试目标 | 5 |
| **小计** | | **约 480，乘 1.4 约 670** |

### 就地修

开工前没有已知缺陷；过程中发现的单列，预留约 100 行。

### 合计

约 770 行，在停止线 1,300 之内，开工。每次提交后跑 `cargo xtask size`。

## 准备阶段那条用例挂住的经过

- **现象**：第一版用例（先信任、再改项目配置、然后建会话）挂了 3 个多小时，最后由编排者用 SIGTERM 结束。重跑时加了逐步输出：`session/create` 返回了，之后一直在等 `turn.started`，这个事件始终没来。
- **原因**：是用例在等一个永远不来的事件，不是产品缺陷。要求重扫的会话里，消息先过一次发送前的重扫（`Purpose::Check`），过了才开回合；没过就进队列，由闸门挡着（`turn.rs` 的模块文档，设计 5.2.6"每次发送 prompt 之前重扫"）。配置在建会话之前就改了，第一条消息过不了这次重扫，进了暂停的队列，根本没有回合开始。这条路径已有 `it_orch_trust_gating_a_change_before_a_prompt_holds_the_message` 覆盖（那条用的是 `send`）。

  会话夹具的等待（`Fixture::events`、`attach`、`command`）没有超时，所以一直挂着。
- **修法**：
  - 要落到"`turn.started` 已写出、附加前的重扫还没做完"这个窗口，就得让附加前的重扫停住。会话 actor 加了一个只在单元测试里编译的挂钩 `actor::held`：按项目路径登记后，附加前的重扫先等测试放行。用例的步骤是：开回合，改配置，放行。之后附加照常进行，但不加载项目配置（`settingSources: ["user"]`），闸门关着，不发 prompt；信任恢复后经 `session/queue_resume` 重扫，闸门打开，prompt 发出。
  - 会话夹具的三处等待都加了 30 s 的真实时间超时，超时报出在等什么（例如 "still waiting for 1 turn.started, 0 so far"）。
- **顺带看到的一点（未改，汇报里提出）**：信任恢复后，这个回合仍跑在不加载项目配置附加的 agent 会话上，`session.configured` 也如实写着 `ignored`。也就是说，用户重新信任了项目，这第一个回合却仍不带项目配置。设计 5.2.6 没有规定闸门在准备阶段重新打开时要不要以加载方式重新附加，这是一个设计问题，不是安全问题（任何时候都不会加载未信任的配置）。

## 故障点清单与每个点上的结果

剧本：添加项目，建会话（第一条消息 `say one`），第一个回合结束后发第二条（`say two`），最后有序退出。每个点依次在第 1、2、4、8……次到达时让宿主退出（退出码 99），直到剧本和随后的有序退出都没再碰到它。每次崩溃后重启宿主，做断言库的五项检查：完整性、seq 连续、前缀、账本重试、两条消息各恰好一次。

| 故障点 | 触发在第几次（崩溃时的状态） | 第一次不再触发 | 断言 |
|---|---|---|---|
| `store.migration_committed` | 1（第一个迁移脚本提交后，宿主还没起来） | 2 | 全部通过 |
| `store.before_commit` | 1、2、4（启动期间，还没受理命令）；8（`session/create` 的批次没提交，会话不存在，重试时新建）；16（第一个回合进行中）；32（剧本跑完，有序退出的写入里） | 64 | 全部通过 |
| `store.after_commit` | 1、2、4（启动期间）；8（会话已提交，重试时回放）；16（第一个回合进行中）；32（有序退出时） | 64 | 全部通过 |
| `store.before_publish` | 1、2（启动期间）；4（`project/add` 已发出）；8（第一个回合进行中）；16（第二个回合进行中） | 32 | 全部通过 |
| `store.snapshot_written` | 1（第一个回合结束的批次里，回合仍是 `running`，重启后补成 `interrupted`）；2（第二个回合结束的批次里） | 4 | 全部通过 |
| `session.created` | 1（会话已提交，第一个回合没开始；重启后由 `reopen` 重开，第一条消息照常发出且只出现一次） | 2 | 全部通过 |

IT-NFR9-crash-under-load：agent 全速输出（`flood 200000 20`）时，每轮看到至少 200 个新的 delta 就 `kill -9` 宿主，共 5 轮。每次重启后完整性、seq 连续、前缀三项都通过；最后一条 `say done` 正常完成。

## 连跑

- `crash_concurrency` 测试目标（四个编号共 9 个测试）连跑 10 次全绿，每次约 58–62 s。跑完查过：本 worktree 的宿主、假 agent 和 `sandbox-exec` 都没有残留进程。
- 准备阶段那条用例连跑 10 次全绿。
