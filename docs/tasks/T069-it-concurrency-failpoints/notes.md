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
