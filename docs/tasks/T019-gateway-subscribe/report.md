# T019 汇报

- 执行者：子 agent（Rust 工程师，宿主网关）
- 分支：`feat/T019-gateway-subscribe`
- 完成日期：2026-10-19

## 做了什么

任务书第 1 到 7 项全部完成，行为都由 IT-gw-subscribe 覆盖。

- **`stream/subscribe`**：
  - 四种情况都用 `sync::subscribe_mode` 判断。
  - 快照在流锁里只做入队（`Item::Snapshot`），由写循环用 `wire::snapshot` 分页编码。分页预算是帧上限减去最长的信封，信封按页号 `u32::MAX`、`last: false` 计。没有 ops 时也发一页空页，`last` 为 true。
  - 补发通过 `Item::Replay` 标记交给写循环，写循环每次读 256 条 `read_events`。读出的条数少于区间长度，说明遇到缺口（读失败也按缺口处理）：摘掉订阅，清掉这条流排队中的实时项，发 `stream/reset`，`reason` 为 `rebuilt`。
  - 会话流的 head 为 0 时回 `not_found`。
  - 每条连接最多 64 个订阅，超出回 `rate_limited`。对同一条流再次订阅会替换旧订阅。
- **`stream/unsubscribe`**：每条连接有一个订阅 task，按到达顺序处理 subscribe 和 unsubscribe。流的加载在读循环里就另起 task 开始，所以多个加载可以并行，生效顺序不变。连接断开时，这些订阅随 `Dispatch` 一起注销。
- **发送时合并**：写循环移到新文件 `gateway/send.rs`。
  - `coalesce_ms > 0` 且本批有实时项时，先按 `Clock` 等一个窗口再发。
  - 同一订阅相邻的事件装进同一条 `stream/events`，一帧装多少就装多少。
  - 只有协商了 `stream.coalesce`，才用 `coalesce::try_merge` 合成带 `from` 的区间事件。
  - `coalesce_ms` 和是否协商了该特性，在 `initialize` 成功后写进 outbox。
- **慢客户端**：每个订阅在 outbox 里有一个 `Feed` 编号，`Feed` 实现了 `streams::Sink`。`push` 返回 `Full` 时，流会调用 `reset`，对应 `end_feed(.., slow_consumer)`。outbox 的 `Push` 改用 `streams::Push`，不再单独定义。
- **Q16**：新增 `AuditEvent::Subscribed {stream, mode, reason, lag}`，经每条连接的 `Auditor` 发进审计 channel。
- **`history/page`**（新文件 `gateway/history.rs`）：
  - 不带游标时，用 `StreamHandle::read` 取窗口里最早的 `turn_no`。
  - `max_bytes` 截到 48,000。
  - 会话流的 head 为 0 时回 `not_found`。
- **能力与接线**：
  - `HOST_FEATURES` 加了 `stream.coalesce` 和 `history.page`。
  - 新增 `Routes::streams(streams, readers)`，把三个方法登记为网关自己处理。`Target::Streams` 占位已删除，UNIT-unsupported-set 允许表里对应的三行也删了。

## 关键取舍

- **按 feed 编号管理订阅**：outbox 里的项都带订阅编号。判断订阅是否还在、清掉它的排队项、排 reset，三件事在同一把队列锁里完成。订阅 task 替换订阅时也先结束旧 feed，所以写循环发出的 `rebuilt` 不会排到同一条流新订阅的响应之后。
- **快照和补发都放到流锁外**：`start` 回调里只入队，编码由写循环做，满足"`Sink` 和 `start` 只能排队"的约束。
- **用一个订阅 task 保证顺序**，没有用 `Opening`、`Live` 两态表：顺序由 task 自然保证，断开时的清理也不用另写。
- **放不进一帧的单个事件**：宿主按设计不会产生这种事件。万一出现，前面的照常发出，这个订阅以 `rebuilt` 结束，避免客户端看到缺口后反复续传。
- **共享文件只做附加**：
  - `Dispatch::new` 多了一个 `Auditor` 参数。
  - `Gateway` 多了 `auditor()`，原来的 `audit()` 没有改动。
  - `Routes::start` 的签名没有改，三个方法另由 `Routes::streams` 登记。

## 验证方式

- IT-gw-subscribe 有 9 个用例，都以 `it_gw_subscribe_` 开头，跑在真实套接字和真实 host.db 上，时钟用 `ManualClock`。客户端的解码器会拒绝超过 64,000 字节的帧，所以每帧都在上限内。
  - **无游标**：先收到快照，之后是实时事件。
  - **续传**：游标在日志范围内时补发，之后接实时事件。
  - **游标规则**：`cursor_ahead`、`epoch_changed`；只带 epoch 或只带 after_seq 都按无游标处理；每次订阅替换上一次；Q16 记录中 `lag` 的取值正确。
  - **`cursor_expired`**：检查点裁剪日志之后，旧游标得到 `cursor_expired` 快照。
  - **补发时读到缺口**：先让客户端暂不读，把写循环阻塞在套接字上，补发标记就留在队列里；用 Q16 记录确认订阅已登记，再触发裁剪。最后收到 `stream/reset`（`rebuilt`），重新订阅得到 `cursor_expired`。
  - **分页与分帧**：快照分成多页，页号连续、`last` 只在最后一页；补发跨多个帧；用快照页能还原出原文。
  - **慢客户端**：积压超过 4 MiB 后收到 `slow_consumer`，之后没有新事件；带游标续传能补齐剩余事件。
  - **发送时合并**：窗口内到达的 delta，协商了 `stream.coalesce` 的客户端收到一个 `from: 3, seq: 6` 的区间事件；未协商的客户端收到同一条 `stream/events` 里的 4 个事件。
  - **上限、退订与 `not_found`**：第 65 个订阅 `rate_limited`；满额时再订阅已有的流可以替换；紧跟在 subscribe 后的 unsubscribe 晚于它生效；退订后不再收到事件。
  - **`history/page`**：没有游标时从窗口之前开始；结果截到 48,000 字节，用 `op_index` 续页；`not_found`。
- 本地连跑 8 遍，`--all-features` 下也通过。
- 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T019-gateway-subscribe/ci/` 全部通过：rust 1109、protocol 176、ts 291、license、testdata-scan。摘要已提交，文件是 `pr-627439e40278.json`。
- 编号 IT-gw-subscribe 已在 `tests/ids.toml` 的 T019 段登记，testdata-scan 通过。

## 提交列表

```
73b24d8 docs(task): T019 pr CI summary
627439e docs(task): T019 notes: estimate, actual size, design points and handoffs
4bdf83e test(host): IT-gw-subscribe
2a08900 feat(host): serve stream/subscribe, stream/unsubscribe and history/page
b62f396 feat(host): queue subscriptions as feeds and send them from a writer of their own
```

## 遗留问题 / 范围外发现

1. **规模超出上限约 3%**：合计增加 1,552 行，其中实现 858、测试 694，比约 1,500 的上限多 52 行。实现比估计的 931 少，超出的部分全在测试：估计 560，实际多出约 130 行。开工前推算是 1,490，测试写完才超出，所以中途没有停下。明细在 `notes.md`。
2. **需要 T037 接线**：组装时要在 `Routes::start(..)` 之后调用 `routes.streams(streams, readers)`，否则这三个方法仍是 `unsupported`。`AuditEvent::Subscribed` 只写 host.log，不写 `audit` 表。
3. **测试夹具不提供这三个方法**：`TestGateway::start` 和 `cupper-gateway-fixture` 都用 `Routes::default()`。之后进程级测试（T062 等）如果需要订阅，要在这些夹具上加 `Routes::streams`。
4. **等合并窗口会拖慢同批的响应**：同一批里的响应也要等这个窗口，与 SP-4a 相同，最多 1 s。桌面默认 `coalesce_ms = 0`，不受影响。如果 CLI 或 iOS 端要用大窗口，可以改成先发窗口前的响应帧。
5. **不存在的会话流会留在内存里**：对不存在的会话订阅或取历史，会把一条空流载入内存，直到空闲淘汰（10 分钟）。影响很小，记录在这里备查。
6. **交接给 T029**：outbox 的项现在是 `Item`，blob 分片仍走 `push_frame`。

相关文件都在仓库内：`crates/cupper-host/src/gateway/` 下的 `subscribe.rs`、`subscribe/tests.rs`、`send.rs`、`history.rs`、`outbox.rs`、`wire.rs`、`routes.rs`、`dispatch.rs`、`audit.rs`，以及 `docs/tasks/T019-gateway-subscribe/` 下的 `notes.md` 和 `ci/`。