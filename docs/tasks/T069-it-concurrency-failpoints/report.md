# T069 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`test/T069-it-concurrency-failpoints`（T037 的 worktree；main 已由编排者在 112d77f 合入）
- 完成日期：2026-10-26

T069 完成：四个编号全部通过，连跑 10 次全绿、无残留进程。带记录的 CI 跑了三次，前两次各有一条与本任务无关的用例失败，第三次全部通过，摘要已提交。没有就地修的产品缺陷。

## 做了什么

1. **登记 M1a 的故障点**（设计 5.9.8，不含 git 和 worktree 的点）。新增四个：
   - `store.before_commit`：写线程的批次已执行、COMMIT 之前；
   - `store.after_commit`：已提交、回复回执之前；
   - `store.snapshot_written`：结束回合的追加事务里，回合快照已写、检查点未写；
   - `session.created`：`session/create` 提交之后、第一个回合开始之前。

   加上原有的 `store.migration_committed`（两个迁移脚本之间）和 `store.before_publish`（提交后、扇出前），共六个点。T121、T124 以后只需登记新点，复用本夹具。
2. **断言库和崩溃夹具**，在 `tests/crash_concurrency.rs`。每次崩溃后重启宿主，检查五项：
   - `PRAGMA integrity_check` 为 ok；
   - 每条流的 seq 连续；
   - 崩溃前客户端实时看到的事件，在日志里同 seq、同内容（前缀）；
   - 在途命令用原 `command_id` 重试时符合账本语义：已提交的回放，建会话的重试返回唯一那个会话；没提交的重新执行；
   - 第一条和第二条消息都在会话里恰好出现一次，并且没有回合停在 `running`。
3. **IT-NFR9-failpoints-core**：
   - 剧本：添加项目，建会话（第一条消息），第一个回合结束后发第二条，最后有序退出。
   - 每个点依次在第 1、2、4、8……次到达时让宿主以 99 退出，直到剧本和随后的有序退出都不再碰到它；有序退出中的崩溃也照样检查。
   - 每个点上命中的时刻和结果都写在 `notes.md`，全部通过。
4. **IT-NFR9-crash-under-load**：agent 全速输出时，每轮看到至少 200 个新 delta 就 `kill -9` 宿主，共 5 轮。每次重启后完整性、seq 连续、前缀三项都通过；最后会话还能正常完成一条新消息。
5. **IT-six-sessions-shared-adapter**：6 个会话同时 prompt，只起了一个适配器进程。每条流里只有自己那个 agent 会话的两次 `whoami` 和完整的输出，6 个 agent 会话两两不同。
6. **IT-host-egress-seatbelt**：
   - profile：`(deny network-outbound (remote ip) (with send-signal SIGKILL))`，宿主和它拉起的适配器都在沙箱里。
   - 正对照：沙箱里的 `nc` 一连 192.0.2.1 就被 SIGKILL。
   - 普通会话正常完成，宿主以 0 有序退出，适配器没有被信号杀掉。
   - 测试用的宿主不在 App 包里，不会启动运行时安装器，登录环境也是注入的，所以没有需要指向本机的安装。
7. **补 T068 留下的那条**：`it_orch_trust_gating_a_first_turn_in_preparation_goes_out_once_trusted`，在会话夹具里。
   - 场景：第一个回合已开始、开回合前的重扫已通过，附加前的重扫还没做时，项目配置变了。
   - 结果：回合停在准备阶段，闸门关着，agent 以不加载项目配置的方式附加，不发 prompt；信任恢复后经 `session/queue_resume` 发出。
   - 要落在这个窗口，需要让附加前的重扫停住。为此在会话 actor 里加了一个只在单元测试里编译的挂钩 `actor::held`。
8. **超时**：
   - 会话夹具的三处等待（`events`、`attach`、`command`）加了 30 s 真实时间超时，超时时报出在等什么；
   - 进程级用例的请求、连接、等表、退出都以 30 s 为限。

## 关键取舍

- **故障点用"按命中次数倍增扫描"，不逐点写剧本**。同一个剧本覆盖了每个点的各种时刻：启动中、建会话的批次、回合中、有序退出时。代价是整个测试目标跑一次约 60 s；6 个测试并行跑，所以墙钟时间不长。
- **收事件的客户端不开合并**：`features` 为空，这样它收到的每个事件都和日志里的一行一一对应，前缀可以逐 seq 比对。
- **Seatbelt 用"出站即杀"，不靠读系统日志找拒绝记录**：只要有一次出站，进程就被杀，用例必然失败，判断最确定。

## 挂住的那条用例（已查清，不是产品缺陷）

- **现象**：第一版用例挂了 3 个多小时。它停在 `session/create` 之后，一直等一个始终没来的 `turn.started`。
- **原因**：用例在建会话之前就改了配置。按设计，第一条消息过不了发送前的重扫，就会进入被闸门挡住的队列，根本不开回合；而夹具的等待没有超时。
- **修法**：用上面的测试挂钩，把配置变化放到正确的窗口里；同时给夹具的等待加上超时。

## 规模

`cargo xtask size`：+575 −6，净 +569。停止线 1,300，硬停止线 1,450。

| 文件 | 增 | 删 |
|---|---|---|
| `tests/crash_concurrency.rs` | 447 | 0 |
| `tests/common/mod.rs`（`connect_once`、`Raw<M>`） | 23 | 2 |
| `orchestrator/session/actor.rs`（测试挂钩） | 37 | 0 |
| `orchestrator/session/tests.rs`（夹具超时） | 17 | 4 |
| `orchestrator/session/trust_tests.rs` | 27 | 0 |
| 故障点登记与标记（`failpoint.rs`、`writer.rs`、`append.rs`、`manager.rs`） | 19 | 0 |
| `Cargo.toml` | 5 | 0 |

## 验证方式

| 验收标准 | 结果 |
|---|---|
| 四个编号全部通过，连跑 10 次全绿，跑完没有残留进程 | `crash_concurrency` 测试目标共 9 个测试，连跑 10 次全绿，每次约 58–62 s；准备阶段那条用例连跑 10 次全绿。跑完查过，本 worktree 的宿主、假 agent、`sandbox-exec` 都没有残留进程 |
| 故障点清单与每个点上的断言结果写进 `notes.md` | 已写 |
| 就地修的缺陷单列 | 无就地修 |
| 测试名以用例编号开头 | 四个编号都在 `tests/ids.toml` 的 T069 段。准备阶段那条沿用同主题的 IT-orch-trust-gating |
| 干净 worktree 里 `cargo xtask ci pr --record docs/tasks/T069-it-concurrency-failpoints/ci/` 通过，摘要随任务提交 | 在合并提交 112d77f 上跑了三次，第三次全部通过，摘要已提交（f925a03）。见下面"CI 三次"的说明 |

第三次 CI 各作业结果：

| 作业 | 结果 |
|---|---|
| rust | 2089 项通过 |
| protocol | 185 项通过 |
| ts | 473 项通过 |
| license | 通过 |
| testdata-scan | 通过 |
| ui | 通过 |
| e2e | 30 项通过 |

另外：
- `cupper-host` 库测试 410 个通过；
- fmt 通过，clippy 在 `--all-features`、不带特性、`dev-auth` 三种构建下都没有问题；
- 跑 CI 期间没有写工作区；
- 摘要里没有本机路径。

### CI 三次

两次失败都与本任务无关，本任务没有改桌面端，也没有改假 agent。

1. **第一次**：`e2e` 的 `test-e2e-focus` 里，E2E-SEC-real-input 失败，是在等请求面板的"手动"提示时超时。
2. **第二次**：`rust` 里 `cupper-fake-agent` 的 `unit_fake_scripts_faults_per_segment` 失败，报 "the agent did not exit"：`exit_on_start` 的假 agent 在 10 s 内没有退出。当时机器负载约 9；这条用例单跑 20 次全过。
3. **第三次**：全部通过。

每次失败后都删掉了那份失败的摘要再重跑。

## 提交列表

```
f925a03 docs(task): T069 CI summary
112d77f Merge branch 'main' into test/T069-it-concurrency-failpoints
88b1dee docs(task): T069 failpoint results and runs
82bd3de test(host): a time limit on every reply the crash scenarios wait for
f0966da test(host): a first turn held in its preparation goes out once trusted
ea9e12b test(host): failpoints, crashes under load, six sessions and egress
9cea49e feat(host): register M1a failpoints of the store and session/create
9c19785 docs(task): estimate T069 by case
```

## 遗留问题 / 范围外发现

1. **准备阶段信任恢复后的设定**：信任恢复后，这个回合仍跑在不加载项目配置附加的 agent 会话上，`session.configured` 如实写着 `ignored`。已按裁定（计划 v1.3.53，设计 5.2.6 新增一条）保持现状，是否以加载方式重新附加留给 T141。用例按现状断言。
2. **两条疑似不稳定的用例**：E2E-SEC-real-input（`test-e2e-focus`）和 `unit_fake_scripts_faults_per_segment`，在本机高负载时各失败过一次，重跑通过，没有深查。后者的 10 s 退出等待在负载下可能偏紧。
3. **测试挂钩 `actor::held`**：只在单元测试里编译，按项目路径登记。以后需要"让某次重扫停住"的会话测试可以复用它。
