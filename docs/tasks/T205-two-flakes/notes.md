# T205 笔记

## 行数估计（开工前）

停止线 150，硬停止线 200（`cargo xtask size`，含测试）。

| 模块 | 估计新增 | 内容 |
|---|---|---|
| `tests/lifecycle.rs` | 5 | 先等索引里 runtime 的结束状态，再读失败原因 |
| `gateway/dispatch.rs` | 20 | `Call` 带上"应答已入队"的信号，应答 task 推进发送队列后再触发 |
| `supervisor/control.rs` | 8 | 应答入队之后才通知 supervisor 退出 |
| 测试 | 50 | 稳定复现的进程内用例；其他构造 `Call` 的地方补字段 |
| 合计 | 约 85 | |

## 实际行数

`cargo xtask size`：新增 76、删除 8（停止线 150）。

## 第一处：`it_runtime_first_prompt_waits_for_the_install`

### 现象

- main 的 push 运行 40639964185：`lifecycle.rs:1527`，left `["-"]`，right `["hash_mismatch"]`。
- PR #20 的运行 40643620891，第 2 次 attempt（rust 作业 122675033610）：`lifecycle.rs:1572`，失败信息相同。两次的行号不同，是因为这期间文件前面新增了一个用例，断言本身没变。
- 两次都已经过了 `assert_eq!(ended, expected)`，说明回合确实以 `failed`、`runtime_failed` 结束了；没对上的只有 `agents.updated` 里的 `adapter.failure`。

### 根因（在测试）

- 回合结束写在会话流里（会话 actor 的 `turn.completed`），安装失败的原因写在索引流里（索引 actor 的 `agents.updated`）。这是两个写者写的两条流，设计里没有规定它们之间的先后。
- 安装器的 `publish`（`supervisor/mod.rs`）先调 `index.runtime(...)`，给索引 actor 发邮件；再 `published.send_replace(...)`，唤醒等在 `Progress::done` 上的启动 task。
- 两次唤醒都发生在同一个 worker 上。tokio 把后唤醒的启动 task 放进 LIFO 槽，先唤醒的索引 actor 被挤进普通队列。于是"启动失败 → 运行时 actor → 会话 actor → `turn.completed`"这条链常常先跑完。
- 用例看到 `turn.completed` 就立刻读最新一条 `agents.updated`，读到的是 `installing` 那一条，它的 `failure` 为空，所以是 `"-"`。
- 索引稍后会写上 `hash_mismatch`，最终状态是对的：`Mail::Runtime` 和之后运行时 actor 发的 `Mail::Agent` 走同一个邮箱，先后有保证。
- 不是产品问题：两条流之间本来就不保证先后，界面分别订阅两条流。

### 证据

- 本机复现（临时改动，不提交）：在索引处理 `Mail::Runtime` 的地方先阻塞 300 ms，单跑一次。失败信息和 runner 上的一模一样：`left: ["-"]`，`right: ["hash_mismatch"]`。
- 用例改好之后，带着这 300 ms 的延迟再跑一次，通过。去掉延迟，再跑一次，通过。
- runner 上修复前的失败率（main 1dae188，test-repeat run 40647386890，100 次 × 2 份并行）：200 次失败 2 次（copy 2 run 39、copy 1 run 45），失败信息相同。

### 修法

- 断言回合结束之后，先等索引里 runtime 变成 `ready`（装好的那一段）或 `failed`（装失败的那一段），再读 `failure`。
- 结束状态和失败原因在同一条 `agents.updated` 里，所以等到前者，就能读到后者。
- 等用的是现成的 `until`（上限 `PATIENCE`），不用 sleep。

## 第二处：`gold_b_claude_approval`

### 现象

- PR #20 的运行 40643620891 第 1 次 attempt，golden 步骤：`gold_b.rs:394:20`，`Closed(Some(Eof))`。出错的是 `stop()` 里 `host/exit` 的应答：客户端读到了连接的结束，没读到应答。
- GOLD-B 的宿主跑在测试进程里，客户端对令牌的检查一直通过。所以这里读不到应答，就是宿主根本没发。lifecycle 用例里的宿主是独立进程，客户端在宿主退出之后会拒收应答，那是另一回事。

### 根因（在宿主）

- `Control::exit` 的应答 future 先记下结果，再给 supervisor 发 `Exit::Requested`，最后返回应答。
- 应答由网关的应答 task（`Dispatch::request` 里的 `Answer::Later`）在 future 结束之后才推进发送队列。
- supervisor 收到 `Exit::Requested` 后执行有序退出：发 `host/shutdown`，逐个停各部分，然后 `close_all`。连接的读循环看到 closing 就调 `outbox.finish()`；从这一刻起，`push_frame` 不再接收新帧，后到的帧直接丢弃。
- 平时 supervisor 落在发消息那个 worker 的 LIFO 槽里，要等应答 task 这一轮跑完、应答已经入队，才轮到它，所以看不出问题。
- 但在多线程运行时里，supervisor 也可能被别的 worker 拿去，和应答 task 同时运行。例如 LIFO 槽连用 3 次后关闭，或者负载高时应答 task 所在的线程被系统挂起。这时 supervisor 可能先走到 `close_all`，应答就被丢掉了。
- 这违反了设计 5.2.13 的裁定："`host/exit` 的回复与 `host/shutdown`……两者都在连接关闭之前进入发送队列"。代码里没有任何东西保证应答先入队，只是调度上碰巧。客户端看到的是连接断开，而不是"退出已受理"。

### 证据

- 本机复现（临时改动，不提交）：在 `Control::exit` 发出 `Exit::Requested` 之后、返回应答之前，阻塞 300 ms，模拟应答 task 被挂起。单跑一次 `gold_b_claude_approval`，失败信息和 runner 上的一模一样：`gold_b.rs:394:20`，`Closed(Some(Eof))`。
- 修好之后，在同一个位置（应答返回之前）阻塞 300 ms，再跑一次，通过：退出要等应答入队之后才开始。
- runner 上修复前的失败率（main 1dae188，test-repeat run 40647390457，200 次 × 3 份并行）：600 次失败 3 次（copy 1 run 35、copy 2 run 50、copy 2 run 111），三次都是 `gold_b.rs:394:20`、`Closed(Some(Eof))`。

### 修法

- `Call` 新增 `reply_queued`（oneshot 的接收端）。网关的应答 task 把应答推进发送队列之后触发它；连接已经关了、应答进不了队列时也触发；发送端被丢弃时，接收端同样会结束。所以等它的一方不会永远挂住。
- 通知没有应答，给一个发送端已经丢掉的接收端，它会立即结束。
- `Control::exit` 记下结果之后，另开一个小 task：等 `reply_queued` 结束，再给 supervisor 发 `Exit::Requested`。这样 supervisor 开始退出时，应答已经入队；`host/shutdown` 和 `close_all` 都排在它后面。
- 只改 `host/exit`，别的请求还和原来一样：退出时还在路上的应答照旧可能被丢弃。设计只对 `host/exit` 有裁定；别的命令可以按 `command_id` 重试，拿到账本里记下的结果。

### 考虑过、没采用的做法

- **关闭连接时等所有在路上的应答（上限是关闭宽限期）**：能顺带保住别的应答，但会改变所有请求在退出时的行为。手动时钟下宽限期不会自己到期，某个应答不来，退出就会挂住。设计没写这一点，不在本任务范围内。
- **让应答 future 晚一点丢弃，靠 drop 时发退出**：要依赖 async 块里变量的丢弃时机，太隐晦。
- **由 Control 自己把应答推进发送队列**：`Call` 里没有请求的 id，而且网关还会再推一次，会重复。

### 稳定复现的用例

`it_lifecycle_host_exit_starts_once_its_answer_is_queued`（`gateway/dispatch/tests.rs`，属于 IT-lifecycle，不新增编号）：

1. 测试扮演网关：直接调用 `Control::deliver`，等应答 future 结束，但不触发 `reply_queued`。这时 supervisor 的通道里必须是空的。触发之后，supervisor 收到 `Exit::Requested { force: true }`。
2. 网关这一侧：经 `Peer` 发一个 `session/send`，桩端口给出应答。等 `reply_queued` 结束时，应答必须已经在发送队列里（`now_or_never` 读得到）。

修前修后对照：把 `Control::exit` 临时改回"先发退出、再返回应答"，第 1 步断言失败（`exits.try_recv().is_err()`），每次都失败，不靠时序；改回修复后的代码，通过。
