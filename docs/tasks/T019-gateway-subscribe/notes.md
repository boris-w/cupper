# T019 工作笔记

- 执行者：子 agent（Rust 工程师，宿主网关）

## 开工前的规模估计

行数指 rustfmt 之后的行，含文档注释和空行，不含本笔记和 CI 摘要。实现按草估的 1.7 倍计（计划 1.1 的执行经验）。

### 设计要点（估计的依据）

- **发送队列**（`outbox.rs`）：实时项改存 `Arc<Published>`，发送时才编码，这样才能合并；另加两种标记项：`Snapshot`（快照的 ops，写循环在锁外分页编码）和 `Replay`（补发区间，写循环从读池读）。每个订阅在队列里有一个编号（feed），`push_live`、标记项和"结束订阅"都按编号处理；结束时清掉这个编号排队中的项，需要时再排 `stream/reset`。判断和清理都在队列锁里，所以写循环发现缺口时发的 `rebuilt`，不会排到同一条流新订阅的响应后面。`Push` 改用 `streams::Push`。
- **写循环**（新 `send.rs`，从 `conn.rs` 移出）：`coalesce_ms > 0` 且取到实时项时，按 `Clock` 再等一个窗口；同一流相邻的事件装进同一条 `stream/events`，协商了 `stream.coalesce` 才用 `coalesce::try_merge` 合成区间事件；补发按页读 `read_events`，条数不等于区间长度就是缺口。
- **订阅**（新 `subscribe.rs`）：每条连接一个小 task，按到达顺序处理 subscribe、unsubscribe；`Streams::open` 在读循环里就另起 task 开始，task 按顺序等它们，所以加载可以并行，生效顺序不变。连接断开时 `Dispatch` 被丢弃，channel 关闭，task 退出，订阅随之注销。
- **历史**（新 `history.rs`）：`history/page` 走 `Answer::Later`。

### 实现

| 模块 | 内容 | 草估 | ×1.7 |
|---|---|---|---|
| `gateway/outbox.rs` 改动 | 新的项、feed 编号、`end_feed`、`push_snapshot`、`push_replay`、合并设置、`Feed` 实现 `Sink` | 70 | 119 |
| `gateway/wire.rs` 附加 | 由事件 JSON 原文拼 `stream/events` 帧；`stream/snapshot` 分页（预算是帧负载减去最长的信封） | 55 | 94 |
| `gateway/send.rs`（新） | 写循环：合并窗口、按流装帧、区间合并、补发分页与缺口、快照编码 | 140 | 238 |
| `gateway/subscribe.rs`（新） | 每连接的订阅 task：四种情况的响应与标记、上限 64、替换与退订、会话流 head 为 0 回 `not_found`、Q16 记录 | 140 | 238 |
| `gateway/history.rs`（新） | `history/page` | 45 | 77 |
| 接线 | `routes.rs`（`Routes::streams`、目标）、`dispatch.rs`（分支、订阅句柄）、`conn.rs`（写循环、合并设置）、`audit.rs`（`stream.subscribed`、每连接的审计句柄）、`mod.rs`、`HOST_FEATURES` | 97 | 165 |
| 小计 | | 547 | **931** |

### 测试

| 项 | 内容 | 估计 |
|---|---|---|
| 夹具 | 临时 host.db、写线程、读池、可调保留参数的 `Streams`；挂上 `Routes::streams` 的 `TestGateway`（`ManualClock`）；按协商特性握手的客户端；追加辅助 | 110 |
| IT-gw-subscribe | 无游标（快照，之后实时）；续传；`cursor_expired`、`cursor_ahead`、`epoch_changed`；只带一半游标按无游标处理；快照分页与补发分帧（页号、`last`、帧上限）；慢客户端重置；发送时合并（协商与不协商）；订阅上限、退订之后不再收到、`not_found`；补发时读到缺口；Q16 记录 | 370 |
| `history/page` | 无游标从窗口最早的回合往前、`max_bytes` 截断、`not_found` | 60 |
| 既有测试的改动 | outbox 单元测试；dispatch 测试的 `Peer` 和夹具（`Routes::streams`、允许表删三行）；握手用例的特性 | 20 |
| 小计 | | **560** |

### 合计

约 1,490 行（实现 931 + 测试 560），贴着上限。每完成一个模块统计一次；一旦推算会超过约 1,500 行，就停下来汇报。可以拆出去的是 `history/page`（实现约 77、测试约 60）。

## 实际规模

按 `git diff --numstat` 统计增加的行，不含本笔记和 CI 摘要。

| 提交 | 内容 | 实现 | 测试 | 删 |
|---|---|---|---|---|
| `feat(host): queue subscriptions as feeds …` | `outbox.rs`、`send.rs`、`wire.rs`；`conn.rs` 换写循环、设合并参数 | 456 | 86 | 109 |
| `feat(host): serve stream/subscribe, …` | `subscribe.rs`、`history.rs`、`routes.rs`、`dispatch.rs`、`audit.rs`、`mod.rs`、`HOST_FEATURES`；dispatch 测试的夹具与允许表、握手用例 | 402 | 17 | 30 |
| `test(host): IT-gw-subscribe` | `subscribe/tests.rs` | — | 591 | 0 |
| 合计 | | **858** | **694** | 139 |

合计 1,552 行，超过上限约 50 行（3%）。实现比估计的 931 少；测试比估计的 560 多约 130 行，多在客户端辅助（先到的通知要缓存，等响应时不能丢）和九个场景的断言上。写测试之前的推算是 1,490，写完才超出，所以没有中途停下。可拆的部分（`history/page`）已经写完，拆出去反而要多一次接线，就没有拆。

## 实现要点

- **feed**：每个订阅在发送队列里有一个编号。实时事件、快照、补发标记都带着它；`end_feed` 在队列锁里摘掉编号、清掉排队中的项，需要时再排 `stream/reset`。
  - 慢客户端：流在 `Push::Full` 后调 `Sink::reset`，`Feed` 把它转成 `end_feed(.., slow_consumer)`。
  - 补发缺口：写循环调 `end_feed(.., rebuilt)`。判断"还活着"和排 reset 在同一把锁里，而订阅 task 替换订阅时也先 `end_feed`，所以 `rebuilt` 不会排到同一条流新订阅的响应后面。
  - 写循环本批里已经取走的、属于已结束 feed 的项不再发送。
- **快照在锁外编码**：`start` 只把 `Start.ops` 移进队列（`Item::Snapshot`），写循环再用 `wire::snapshot` 分页编码。预算是帧上限减去最长的信封（页号取 `u32::MAX`、`last` 取 `false`），再加回 `paginate` 计入的那对方括号。
- **补发**：每次读 256 条。`read_events` 读 `(after, end]`，seq 唯一，条数等于区间长度就说明没有缺口；读失败也按缺口处理。
- **装帧**：`stream/events` 由事件 JSON 原文拼接，帧头部用 `serde_json` 写流名。单个事件放不进一帧时（宿主按设计不会产生），之前的照常发出，这个 feed 以 `rebuilt` 结束，免得客户端看到缺口后反复续传。
- **合并**：`coalesce_ms > 0` 且本批有实时项时，按 `Clock` 等一个窗口再取一次。同一个 feed 相邻的事件装进同一条 `stream/events`；协商了 `stream.coalesce` 才用 `coalesce::try_merge` 合成区间事件（合并后不超过 `MAX_MERGED_ESCAPED`）。补发的页同样合并。等窗口时同一批里的响应也一起等，与 SP-4a 相同。
- **订阅 task**：读循环收到 `stream/subscribe` 就另起 task 调 `Streams::open`，把它的 `JoinHandle` 和在途许可一起发给这条连接的订阅 task。订阅 task 按到达顺序等待并处理，所以加载可以并行，subscribe 和 unsubscribe 的生效顺序不变。等到流打开之后先释放在途许可，再在锁里排响应，与 `Dispatch` "先释放再写回复"一致。
- **上限 64**：判断前先去掉已经被流重置的订阅（feed 已结束）。同一条流再次订阅时先结束旧订阅再判断，所以满额时仍可以替换。
- **Q16**：`AuditEvent::Subscribed {stream, mode, reason, lag}`。`lag` 是游标与 head 的距离，只在游标的 epoch 与宿主相同、并且没有超过 head 时给出。经每连接的 `Auditor` 发进审计 channel。
- **`history/page`**：不带游标时，取窗口里最早的 `turn_no`；窗口里没有回合，说明没有已结束的回合（窗口总保留最新一个），这时取 `u32::MAX`，结果为空页。
- **测试**：慢客户端和补发缺口都靠"客户端暂不读"让写循环阻塞在套接字上，补发标记就留在队列里。用 Q16 的审计记录确认订阅已经登记，再触发裁剪。合并用 `ManualClock::next_deadline` 确认写循环正在等窗口。

## 交接

- **T037（组装）**：`Routes::start(..)` 之后调用 `routes.streams(streams, readers)`，`Streams` 和 `Readers` 用宿主的那一份；否则这三个方法仍是 `unsupported`。`AuditEvent::Subscribed` 只写 host.log，不写 `audit` 表。
- **T029**：发送队列里的项现在是 `Item`，blob 分片仍走 `push_frame`。`Item::feed()` 只对订阅的项有值。
- **T062、T090**：可以依赖 `stream.coalesce`、`history.page` 两个特性和上述行为。
