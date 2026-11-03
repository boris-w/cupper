# T227 工作笔记

## 原因

**不是 T224 那一类问题。** 窗口照用例假定的方式开始和结束：第 3 回合的事件全都落在同一个 200 ms 窗口里，由 `test/clock_advance` 结束窗口。真正的原因是：这一批里，会话流的两条 delta 中间夹了一条索引流的事件，range 就合并不成了。

- 发送端（`gateway/send.rs`）把一批里同一个 feed 连续的 live 事件凑成一组（`run`），合并只在组内做。遇到另一个 feed 的事件，当前这组就先发出去（`Send::items` 里 `run_of != feed` 时 `flush`）。
- 第 3 回合 "flood 400 5; sleep 300; flood 400 5" 在 `ManualClock` 上只写两条 delta：第一个片段立即写（d1），其余的等 40 ms 合并定时器，而这个定时器在手动时钟上不会到期，所以回合结束时才一起写出（d2）。两条之间隔着 300 ms 的真实等待，range 只能由 d1、d2 合并而来。
- 用例的 `Peer` 同时订阅了会话流和索引流。T156（632210c，今天合并）开始在回合开始时取电源断言，索引随即写一条 `host.updated {power_assertion}`。这条事件由 supervisor 的 watch actor 取断言之后再交给索引，和会话里 agent 的第一段输出谁先谁后没有保证：
  - 落在 d1 之前，d1、d2 仍在同一组里，合并成 `39-40` 这样的 range（本机正常运行时都是这样）；
  - 落在 d1 和 d2 之间，这一批就成了 会话 [… d1] / 索引 [host.updated] / 会话 [d2 …] 三组，d1、d2 各在一组，没有 range，用例在 `gateway_sync.rs:373` 报 "no range events live"。
- 两次失败的运行（https://github.com/boris-w/cupper/actions/runs/41603412092 、https://github.com/boris-w/cupper/actions/runs/41603934915 ）的头提交都包含 T156。构件里只有测试输出，没有更细的时间线。

**本机确定性复现**：实验时在 `supervisor/power.rs` 的 `Holder::handle` 里，`index.power_assertion` 之前插入 100 ms 的真实等待。用例打印 windowed 收到的批次如下（流、seq、类型），连跑 3 次都在同一行失败：

```
session 34:turn.started 35:item.started 36:item.completed 37:snapshot.recorded 38:item.started 39:item.delta
index   15:host.updated 16:host.updated
session 40:item.delta 41:item.completed 42:snapshot.recorded 43:turn.completed
```

正常运行时，同一位置是 `session … 37 / index 16:host.updated / session 38:item.started 39-40:item.delta …`，索引事件落在 d1 之前。实验代码没有提交。

## 产品行为

没有错：每条事件都按序到达，只是少合并了一些。窗口里的事件按到达先后排，不同流的事件交错时，`stream/events` 就拆成几帧，同一 item 的 delta 不能跨帧合并。桌面端默认 `coalesce_ms` 为 0，这个窗口目前只有主动开启的客户端会用到。要不要在一个窗口里按流分组再合并，属于合并窗口的设计，不在本任务范围内。

## 用例的改法

第一版（d5fc33e）让 windowed 在第 3 回合开始之前退订索引流（`stream/unsubscribe`）。修后对照（https://github.com/boris-w/cupper/actions/runs/41606378793 ）600 次里有 5 次挂住：构件里那 5 次的日志都只有 "has been running for over 60 seconds"，没有 panic，最后被 `RUN_TIMEOUT` 切断。

原因：windowed 一旦订阅了索引流，索引的 live 事件（例如别的连接接入后写的 `clients.updated`）就可能在它变为 live 之后、退订之前到达。写循环取到 live 事件就在手动时钟上开 200 ms 的窗口，窗口结束前什么都不发，排在后面的退订应答也在内。而时钟要到第 3 回合结束后才推进，于是用例等退订应答、窗口等时钟，互相卡住。窗口照样扣住 RPC 应答，这一点是写循环的设计（先等窗口再发整批），不是新问题；只是这个用例第一次在窗口打开时对 windowed 发请求。

确定性复现：在 windowed 变为 live 之后、退订之前，另连一个客户端，再等 200 ms 真实时间，用例每次都停在退订上（打印了 "unsubscribing"，没有打印 "unsubscribed"）。

第二版：windowed 从一开始就只订阅会话流（`Peer::session_only`），不订阅索引，也就不需要退订，变为 live 之后不再对它发任何请求。用例要测的是会话流在窗口里合并成 range，索引流本来就和它无关。两种注入（取断言前等 100 ms；第 3 回合前另连一个客户端）同时加上，连跑 3 次都通过；去掉注入，`gateway_sync` 的 11 个用例都通过。

## runner 连跑

参数：`kind=cargo`、`package=cupper-host`、`args=--features test-harness --test gateway_sync`、`filter=it_gw_sync_reconnect`、`times=150`、`parallel=4`、`runner=macos-15`。修前在 main，修后在本分支。

- 修前（main）：https://github.com/boris-w/cupper/actions/runs/41606374367 ，546 通过、54 失败，全部是 `gateway_sync.rs:373` 的 "no range events live"。
- 修后第一版（d5fc33e）：https://github.com/boris-w/cupper/actions/runs/41606378793 ，595 通过、5 次挂住（见上）。
