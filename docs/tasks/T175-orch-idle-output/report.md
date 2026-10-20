# T175 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T175-orch-idle-output`（没有合并 main）
- 完成日期：2026-10-20

## 做了什么

按裁定（计划 v1.3.45），本任务只做空闲退出和订阅时载入。超长输出拆给了 T177。

1. **actor 空闲退出**（`session/actor.rs`、`manager.rs`、`durations.rs`）
   - actor 有自己的空闲定时器（一个 `TimerTable` 加 `Alarm`），时长登记为 `session_idle`（600 s），只经 `Clock` 计时，`test_config.durations` 可以缩短。
   - 每处理完一件事都重新判断一次：空闲就从当前时刻重新计时，不空闲就取消计时。
   - 到时如果仍然空闲，actor 依次：
     1. 送一个 `Detach`，关闭 agent 会话（`Close`）；
     2. 放下全部部件，包括流写者；
     3. 把收件箱交回管理器。
   - 管理器接着依次：
     1. `forget`：只在邮箱仍是这个会话的那一个时，才从表里撤出；
     2. 关闭收件箱；
     3. 把已经在收件箱里的调用重新投递，经按需载入交给下一个 actor。
   - 先放下写者，新的载入就不会遇到 `WriterTaken`；先撤出邮箱，新调用就不会落进正在退出的收件箱。
   - 静止屏障：actor 等定时器时处在 `Participant::idle` 里，到时被唤醒即算忙，退出时随 actor 放下。重扫照旧持有 `Hold`（T068）。
2. **订阅会话流时按需载入**（`gateway/dispatch.rs`、`routes.rs`、`manager.rs`）
   - `SessionPort` 加了 `wake`，`Routes` 记下会话端口。
   - 网关处理会话流的 `stream/subscribe` 时调用 `wake`：会话已在内存时只取一下邮箱，不在内存就按需载入，T089 的补写会马上进流。
   - 载入失败不影响订阅本身；载入之后照第 1 条的空闲规则退出。
3. **IT-orch-actor-idle**，4 个用例：
   - 空闲到时（差 1 s 时还在）关闭 agent 会话、从表里撤出，再发消息时按需载入，并恢复 agent 会话；
   - 有运行中回合时不退出；回合被中断、队列暂停后照常退出，恢复队列时载入并跑排队的消息；
   - 定时器到时与两个调用同时到达，两个调用结果一致，之后发消息正常，只有一个 actor；
   - 宿主重启后只订阅、不发命令，流里就有补写的 `turn.completed{interrupted}` 和 `interaction.resolved{expired}`。

## 采用的"空闲"定义

以下六条同时满足才算空闲：
1. 没有运行中的回合；
2. 没有待处理的交互请求；
3. 没有会出队的排队消息：暂停中的排队消息不算，它等的是用户，与宿主空闲退出的规则一致（T059）；
4. 会话核心没有计时器，例如合并器的 40 ms、自发回合判定、档位回退；
5. 没有在途的重扫或连接；
6. 没有等信任检查的 `session/send` 回复。

另外两点：
- 订阅者不算活动：会话流的读取不经 actor，窗口开着这个会话也可以退出。
- 宿主层的空闲退出（T059）是另一套计时，两者没有混用。

## 关键取舍

- **退出时关闭 agent 会话**：不关的话，适配器里会一直留着一个没人接收事件的会话。下一个 actor 会照 T089 的恢复规则 `session/resume` 它。
- **交回收件箱，而不是丢弃**：退出那一刻已经投进收件箱的调用照常得到处理，调用方不会看到"会话已停止"。
- **等静止屏障而不是硬等**：测试经 `SessionManager::settled`（只在测试里编译）确认重扫等在途工作都已结束，再推进时钟，用例是确定的。

## 验证方式

- `cargo test -p cupper-host --lib`：全特性 403 个通过。
- `cargo clippy -p cupper-host --all-targets --all-features`：没有警告。
- 变异检查：
  - 去掉订阅处的 `wake`：重启后只订阅的用例等不到补写事件，挂住；
  - 空闲判断不看运行中回合：第二个用例失败。（第一版用例同时有请求和排队消息，抓不到这个变异，已改成只用运行中回合检查。）
- 有一次变异检查挂住，留下了我自己起的测试进程，已用 SIGTERM 结束，没有残留进程。
- 在干净工作区运行 `cargo xtask ci pr --record docs/tasks/T175-orch-idle-output/ci/`，各作业通过，摘要 `pr-568c06a0e6de.json` 已提交：
  - rust 1964、protocol 185、ts 471、license、testdata-scan、e2e 4；
  - ui 按路径过滤跳过；`sensitive-words` 因本机没有词表跳过。
- **规模**：新增 242 行（实现 121、测试 121）。每个模块写完都统计了，记在 `notes.md`。用例比估计多 51：夹具要加 `stream/subscribe` 路由和订阅辅助，管理器加了两个测试辅助。

## 提交列表

```
b449e6b chore(ci): record T175's PR run
568c06a feat(orch): session actors leave memory when idle; a subscription loads
b447014 docs(task): T175 notes and size estimate
```

## 遗留问题 / 范围外发现

1. **`ids.toml`**：本分支没有合并 main，`tests/ids.toml` 里 IT-orch-long-output 还在 T175 段下；main 上已移到 T177。CI 的 trace-ids 检查通过了，合并时以 main 为准。
2. **`notes.md`**：开头写的估计还包含超长输出那一块（约 375），实际数字以末尾的逐模块统计和本汇报为准。
3. **`wake` 对不存在的会话**也会起一次载入，查不到行就回 `not_found` 并撤出，代价是一次数据库读。订阅本来就只针对已有的会话，没有另加判断。
4. **T177 的接入点**：Parts 和 SessionDeps 还没有 blob 存储；合并器的计时器键目前只有 `FlushTimer` 一种，要扩成枚举，加上尾部节拍。这两点在开工时处理。
