# T087 工作笔记

## 设计（开工前）

- **`sync.ts`（`StreamSync`）**：一条流的纯核心，不做 I/O。规则照 Rust `Mirror`（`crates/cupper-cli/src/mirror.rs`），但不持有视图：视图在调用方的 `StreamSink` 里（T072、T080）。
  - 每个入口返回"要不要重新订阅、带什么参数"，与 `Mirror::handle` 相同。
  - sink 收到 `reset()`（丢弃视图），然后是快照 op 或实时事件的 `apply(body, event?)`，再是 `live()`（视图完整，事件接着来：最后一个快照页之后，或者对得上游标的 replay 之后）。`apply` 抛错就当作 `ViewError`。订阅失败交给 `failed(error)`。
  - `events`、`ops` 先当 `unknown[]` 取出（`zStreamEventsParams`、`zStreamSnapshotParams` 用 `.extend` 改掉数组元素的类型），再逐个 `zEvent`/`zEventBody.safeParse`。类型已知、数据残缺的事件记日志，并照常交给 sink（`isKnownEvent` 为 false，显示占位）。
  - 三种计数按 5.7.6 末尾的裁定。`resets` 只在丢弃的是完整视图（已有游标）时加一，快照加载途中出错不算。
- **`HostClient.subscribe(stream, sink)`**：用 T071 留下的接入点，`client.ts` 的现有逻辑不动。
  - 构造时自己先登记一个 `onNotification` 和一个 `onState` 监听，排在使用方之前。
  - `stream/subscribe` 走 `call`；订阅期间退订了的流，迟到的结果忽略。
  - `ready` 时按游标重新订阅全部流；`reconnecting`、`failed`、`closed` 时各流进入等待并保留游标。
  - 退订时发 `stream/unsubscribe`。
  - `counters` 是全部流的合计。

## 行数估计（开工前）

| 模块 | 代码 | 测试 |
|---|---|---|
| `sync.ts` | ~150 | ~230（纯核心，对照 Rust 的 7 个用例，另加计数和逐个解码） |
| `client.ts` 的 `subscribe`、路由、计数，`index.ts` 导出 | ~55 | ~60（经 `connectShell` 和 mock 宿主：订阅、断线重连后按游标重新订阅、退订） |
| 合计 | ~205 | ~290 |

预计约 500 行，在任务书估计的 450–560 行之内，低于约 1,500 行的上限。

## 实际行数

每完成一个模块统计一次（相对分支起点，不含 `pnpm-lock.yaml` 和 `docs/`）。

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 累计 |
|---|---|---|---|
| `sync.ts`、`index.ts` 导出、纯核心的 8 个用例（`b2464c9`） | 455 | 228 | 455 |
| `HostClient.subscribe`、路由、计数、经 mock 宿主的 2 个用例（`1847eb6`） | 154 / 1 | 79 | **609** |

- 合计 609 行：代码 302 行，测试 307 行。比估计（约 500 行）多约 22%，超出任务书估计的 450–560 行，在约 1,500 行的上限以内。
- 多出的主要在 `sync.ts`：估计约 150 行，实际 226 行。文件头逐条写明了与 Rust `Mirror` 对应的规则；`StreamSink`、`SyncCounters` 每个成员都有说明；Prettier 把快照页的四项核对展开成多行。

## 实现时的取舍

- **与 Rust `Mirror` 逐条对应**：
  - `subscribed`：只有游标的 epoch 和 seq 都对得上的 `replay` 才进入 live；其他 mode，包括不认识的，一律丢弃视图和游标，进入加载状态。
  - `snapshot`：只在加载状态接收。等待状态下来的是旧订阅的页，live 状态下来的页不该出现，两者都忽略。页的 `at_seq`、`epoch`、页号对不上就重来。没有 `type` 的 op 跳过。
  - `events`：等待状态下忽略。加载状态下收到事件属于违反协议，重来。信封解不开时，同一条消息里排在它前面的事件照常应用，然后不带游标重新订阅：快照绕过这条坏事件，不会反复要回同一条。
  - `reset`：等待状态下忽略，否则带游标重新订阅。
  - 视图在 sink 里，所以"丢弃视图"就是调用 `sink.reset()`。在 `live()` 之前，sink 可以把快照建在一份暂存视图上；Rust 在最后一页之前，`view()` 都是空的。
- **计数**：
  - 缺口与重复：按事件的第一个 seq（`from`，没有 `from` 就是 `seq`）分类。第一个 seq 不大于游标的算重复，包括与游标重叠的区间事件；其余对不上的算缺口，包括 `seq < from` 这种畸形的区间。
  - `resets`：
    - 等待状态下忽略的 `stream/reset` 不计。
    - 丢弃视图时，只有已经有游标（视图完整）才计；快照加载途中出错重来，不计。
    - 因此按游标续传、`epoch` 不变时，三种计数都是 0；`epoch_changed` 记 1 次 reset，重用的 seq 不记重复（IT-NFR2-epoch 的要求）。
- **订阅结果格式错误**：交给 `sink.failed`（`-32603`），流停在等待状态。没有像 Rust 那样断开连接：渲染进程不持有连接，断开也只会换来同样的结果。
- **`HostClient` 的接线**：
  - 构造时先登记自己的 `onNotification`、`onState` 监听。`Listeners` 按登记顺序调用，所以各流先于使用方的监听方收到通知和状态。
  - `ready` 时，各流按游标重新订阅。它发生在 `attach` 处理剩余字节、重发排队命令之前。
  - 退订：立即从路由表里删除，再发 `stream/unsubscribe`。迟到的订阅结果和数据，因为找不到对应的流而被忽略。同一条流不能订阅两次，重复订阅直接抛错。
- **用例**：
  - 纯核心的 8 个用例对照 Rust 的 7 个，另加计数和"残缺只影响自己"。
  - 经 `connectShell` 和 mock 宿主的 2 个用例：
    - 先走快照，宿主断开后自动重连，再按游标 replay；
    - 只路由给本客户端订阅的流；订阅结果到达之前就退订的流，忽略这个结果。
  - 做过变异检查。以下几种改法，对应的用例都会失败：
    - `stream/reset` 改为丢弃游标；
    - 快照不再丢弃游标；
    - 把重复和缺口的分界挪动一位；
    - 遇到没有 `type` 的 op 时整页重来；
    - 重连后只给没有游标的流重新订阅。
