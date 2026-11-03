# T227 汇报

- 执行者：子 agent（Rust 工程师，宿主网关）
- 分支：`fix/T227-gw-sync-ranges`，头提交 `d1ea34d`，基于 main `f5d4689`
- 完成日期：2026-11-03
- PR：https://github.com/boris-w/cupper/pull/50 。`d1ea34d` 上的 CI 运行是 https://github.com/boris-w/cupper/actions/runs/41615929372 ，交汇报时 7 项通过（ui、protocol、testdata-scan、compat-framework、e2e、ts、license），rust 还在跑。
- 三次 runner 对照，参数都是 kind=cargo、package=cupper-host、args=`--features test-harness --test gateway_sync`、filter=`it_gw_sync_reconnect`、times=150、parallel=4、runner=macos-15：
  - 修前，main：https://github.com/boris-w/cupper/actions/runs/41606374367 。546 通过、54 失败，全部是 `gateway_sync.rs:373` 的 "no range events live"。
  - 修后第一版，d5fc33e：https://github.com/boris-w/cupper/actions/runs/41606378793 。595 通过、5 次挂住，都被 `RUN_TIMEOUT` 切断，没有 panic。
  - 修后第二版，d1ea34d：https://github.com/boris-w/cupper/actions/runs/41615922327 。600 次全部通过。

## 做了什么

### 原因

不是 T224 那一类问题。窗口按用例假定的方式开始和结束，第 3 回合的事件都落在同一个 200 ms 窗口里。问题出在这一批里，会话流的两条 delta 之间夹进了一条索引流的事件。

- 发送端（`gateway/send.rs`）把一批里同一个流连续的 live 事件凑成一组，range 只在组内合并；遇到另一个流的事件，当前这组就先发出去。
- 第 3 回合在 `ManualClock` 上只写两条 delta：d1 是第一个片段，立即写出；其余片段等 40 ms 的合并定时器，手动时钟上这个定时器不到期，所以要到回合结束才作为 d2 写出。两条之间隔着 300 ms 真实等待，range 只能由 d1 和 d2 合并而来。
- T156（632210c，今天合并）在回合开始时取电源断言，索引随即写一条 `host.updated {power_assertion}`。这条事件和 agent 的第一段输出谁先到没有保证：
  - 它落在 d1 之前，d1、d2 仍在同一组，正常合并；
  - 它落在 d1 和 d2 之间，这一批就被拆成三组："会话 [… d1] / 索引 [host.updated] / 会话 [d2 …]"，没有 range，用例失败。
- 两次 CI 失败（41603412092、41603934915）的头提交都包含 T156。构件里只有测试输出，没有更细的时间线，时间线是靠下面的本机复现得到的。

### 修法与第一版引入的挂起

1. **第一版（30fe6c8）**：windowed 客户端变为 live 之后、第 3 回合之前，退订索引流。
   - 这引入了挂起。windowed 先订阅了索引流，一条索引 live 事件（例如别的连接接入后写的 `clients.updated`）可能在退订之前到达。写循环取到 live 事件，就在手动时钟上开 200 ms 的窗口，窗口结束前什么都不发，排在后面的退订应答也发不出去。
   - 而时钟要等第 3 回合结束才推进，第 3 回合又要等退订返回之后才发出，两边互相等住，修后第一版对照里的 5 次挂起就是这样。
2. **第二版（104759d）**：windowed 从一开始就只订阅会话流（新增 `Peer::session_only`），变为 live 之后不再向宿主发任何请求。用例要测的是会话流在窗口里合并成 range，索引流本来就和它无关。
3. 规模：增 16、删 6，停止线 80。

## 关键取舍

- **为什么不用退订**：用例的意图只在会话流上，所以不订阅索引是最直接的隔离办法。退订要在窗口可能已经打开的时候发请求，正好碰上"窗口扣住 RPC 应答"的设计（见遗留问题第 2 条）。
- 没有改写循环和合并窗口的设计（brief 的"不做"）。

## 验证方式

本机确定性复现（实验代码都已撤掉，没有提交）：

- **"no range events live"**：在 `supervisor/power.rs` 的 `Holder::handle` 里，在 `index.power_assertion` 之前插入 100 ms 真实等待。原用例连跑 3 次都在第 373 行失败。打印出的 windowed 批次是：
  ```
  session 34:turn.started 35:item.started 36:item.completed 37:snapshot.recorded 38:item.started 39:item.delta
  index   15:host.updated 16:host.updated
  session 40:item.delta 41:item.completed 42:snapshot.recorded 43:turn.completed
  ```
  正常运行时，这条索引事件落在 39 之前，会话这边出现 `39-40:item.delta` 合并后的 range。
- **第一版的挂起**：在 windowed 变为 live 之后、退订之前，另连一个客户端，再等 200 ms 真实时间。每次都卡在退订：打印了 "unsubscribing"，始终没有打印 "unsubscribed"。
- **第二版**：两种注入同时加上，连跑 3 次都通过；去掉注入后，`gateway_sync` 的 11 个用例都通过；clippy（test-harness）无警告。

## 提交列表

```
d1ea34d docs(task): record the T227 hang and its fix
104759d test(host): let the windowed client take the session only
d5fc33e docs(task): add the T227 notes
30fe6c8 test(host): keep the index out of the coalescing window's batch
```

## 遗留问题 / 范围外发现

两条都是产品层面的观察，没有改动。

1. **窗口内没有按流分组合并**
   - 写循环把一个窗口里的 live 事件按到达先后排；不同流的事件交错时，`stream/events` 被拆成几帧，同一个 item 的 delta 不能跨帧合并。正确性不受影响，事件都按序到达，只是合并少了。
   - 损失有多大：每当一条别的流的事件插进一个流的连续事件里，就多一帧、少一次合并。索引的事件不多：每个回合开始和结束时各有一条 `host.updated`（电源断言，只在第一个回合开始、最后一个回合结束时变化），`session.upserted` 按 100 ms 攒批，`clients.updated` 只在连接变化时出现。单个会话时，每个回合大约多 2 到 4 帧，相对整个回合的 delta 数量可以忽略。多个会话同时运行时，索引大约每 100 ms 写一批，一个 200 ms 的窗口里每个会话流最多被拆成 2 到 3 帧（原本 1 帧），帧数可能翻倍。
   - 谁会受影响：只影响 `coalesce_ms` 不为 0、并且同时订阅了索引流和会话流的客户端。桌面端默认 `coalesce_ms` 为 0，不受影响；CLI 默认也不设。主要是将来为省流量而开窗口的远程客户端，例如 M3 的 iOS 端。
   - 可能的改法：在一个窗口的批次里先按流分组，每组内部保持顺序，再合并。前提是协议不保证跨流的到达顺序，这一点要先在 5.3.3 里确认。这属于合并窗口的设计。
2. **带窗口的客户端，RPC 应答也被窗口扣住**
   - 写循环取到 live 事件之后，先等整个窗口，再把这期间排进来的所有内容整批发出，其中也包括 RPC 应答（`Item::Frame`）。所以只要有 live 事件打开了窗口，这个客户端的请求应答最多会晚 `coalesce_ms`，上限 1 s。
   - 在真实时钟上，这是一段有上限的延迟，不会卡死；在手动时钟上，就成了这次第一版的互相等待。
   - 受影响的同样是开了窗口的客户端，例如 M3 的远程客户端：交互操作（审批、发送）的应答会多出最多一个窗口的时间。
   - 可能的改法：RPC 应答和非 live 的帧不进窗口、直接发出；或者窗口只攒 live 事件，其余插队先发。要留意这会不会打破应答和事件之间的先后约定，比如订阅应答必须先于快照。
3. worktree 保留着，等编排者合并后再清理。
