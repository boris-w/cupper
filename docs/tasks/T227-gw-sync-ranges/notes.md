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

windowed 这个客户端在开始第 3 回合之前退订索引流（`stream/unsubscribe`），窗口里就只有会话的事件。用例要测的是会话流在窗口里合并成 range，索引流本来就和它无关。加了上面的延迟之后，用例连跑 3 次都通过；去掉延迟，`gateway_sync` 的 11 个用例都通过。

## runner 连跑

参数：`kind=cargo`、`package=cupper-host`、`args=--features test-harness --test gateway_sync`、`filter=it_gw_sync_reconnect`、`times=150`、`parallel=4`、`runner=macos-15`。修前在 main，修后在本分支。
