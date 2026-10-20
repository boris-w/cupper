# T062 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`test/T062-it-gateway-history`（在 T037 的 worktree 里，从 main ffb4bb1 开出）
- 完成日期：2026-10-20

## 做了什么

五个编号都写完了，也都通过。在 `crates/cupper-host/tests/gateway_sync.rs` 里共 12 个测试，名字都以编号开头。另外就地修了两处缺陷。

1. **IT-gw-sync**：把 SP-4a 的 t1–t7 和适配器隔离迁了过来，共 8 个测试，跑在真实的宿主进程上，agent 是 claude 形态的假 agent，客户端用 CLI 库的 `Client` 和 `Mirror`。原型只作蓝本，代码没有依赖它。逐条对照写在 `notes.md`，设计改过的地方摘要如下：
   - **t1 多客户端**：两个客户端的视图相同，晚来的客户端快照也相同。"大工具输出转 blob"归 T175，还没合并，所以 blob 这一条改为：一个客户端用 `blob/put` 上传 `text_blob`，另一个客户端用 `blob/get` 取回。
   - **t2 先到先得**：两个连接同时回答同一个请求，恰好一个成功，另一个收到 `conflict/already_resolved`。只读的 `cli` 身份回答被拒（`forbidden`）。
   - **t3 续传**：断线期间的事件从游标回放，没有缺口，回放里有区间事件；协商了 `coalesce_ms` 的客户端实时收到区间事件。"游标早于保留的日志"不在进程级做：保留规则不可注入，这条已由 T019 的网关层用例覆盖。
   - **t4 慢客户端**：停读的客户端超过 `max_queue_bytes` 后收到 `stream/reset {slow_consumer}`，agent 不受拖累。它恢复后带游标重订走 `replay`，seq 连续追到头。
   - **t5 `kill -9` 恢复**：残留的进程组被新宿主回收；回合是 `interrupted`，请求是 `expired`，再作答被拒。按 T089 的规则，队列以 `turn_interrupted` 暂停，`session/queue_resume` 之后续上原来的 agent 会话（`resumed=true`）。
   - **t6 命令幂等**：用 `replayed: true` 判断是否回放。两个连接并发重复只执行一次；同一 ID 换参数返回 `idempotency_mismatch`；重启后仍回放原结果。
   - **t7 帧上限**：客户端拒发超限帧；3 MB 消息完整到达；100,000 字符的标题被截断；晚来的客户端分页快照一致。500 kB 工具输出这一条见遗留第 1 条，没有迁。
   - **适配器隔离**：适配器只继承 fd 0–2；宿主环境里的 `CUPPER_*` 和白名单外的变量都没有传给它。
2. **IT-NFR2-epoch**：一个数据目录连续启动六次，覆盖设计 5.9.8 的 ①–④：
   - `kill -9` 后同一次开机重启，epoch 不变；正常退出后换开机，epoch 不变；`kill -9` 后换开机，epoch 加 1，客户端多一次全量快照。
   - 同一 epoch、游标超过 head 时走 `cursor_ahead` 快照。
   - 模拟断电：宿主运行中用 `VACUUM INTO` 拍下 seq M 时的库，跑到 N 后换回这个副本、换开机 UUID 重启。客户端带旧 epoch 订阅走 `epoch_changed` 快照；之后从 M+1 起重用的 seq 被 `Mirror` 当作新事件接受，不报重复。
3. **IT-history-from-host**：跑 22 个回合后有序重启。没有任何缓存的新客户端拿到 20 个回合的窗口，再用 `history/page` 取到更早的两个回合，合起来与重启前的实时视图相同。全程没有拉起新的适配器，历史不来自 agent 的回放。
4. **IT-notification-intent**（手动时钟）：
   - 需要处理、回合完成、回合失败各产生一条意图。
   - 100 ms 合并窗口内已答复的请求不产生意图，被中断的回合也不产生。
   - harness 客户端上报的在场不起作用。
   - `dev-auth` 构建里，用 `dev_key` 连上的 desktop_ui 在前台看着这个会话时不产生意图，离开前台后恢复。
5. **IT-host-orphaned**（只在 `dev-auth` 构建里编译）：经 `sh` 拉起宿主，回合等着批准时让父进程退出。宿主拒绝新的会话和回合（`unavailable`），等回合结束后有序退出：退出码 0，审计记录 `host.exiting {reason: orphaned}`，适配器都已结束。宿主不是测试的子进程，所以守卫先按 pid 和启动时间核对再发 SIGTERM，并用 `ExitWatch` 等它退出。
6. **夹具**：
   - `typed`、`events`、`respond`、`audited`、`refused` 从 `interaction_trust.rs` 挪进 `tests/common/`，`interaction_trust.rs` 改为直接用共用版本。
   - 宿主可以按自定的载荷和 `Command` 拉起，客户端可以按指定选项连接，假 agent 的数据目录和 `test_config` 拆成两步，方便重启。
   - `lifecycle.rs`、`interaction_trust.rs` 都照常通过。
   - 新测试目标 `gateway_sync` 只要求 `test-harness` 特性。

规模：只算 `crates/`，增 1,188 行、删 80 行，净增 1,108 行（开工前估计约 1,170，含就地修的预留），没有超过约 1,300 的上限。按用例的数字在 `notes.md`。

### 就地修的缺陷

| # | 现象 | 原因 | 修法 | 行数 |
|---|---|---|---|---|
| 1 | 宿主每次有序退出（SIGTERM、`host/exit`、空闲、孤立、待升级），所有连过适配器的会话都多出一条 `agent_restarted` 提示。IT-history-from-host 修之前 5/5 失败；另用临时用例确认这条提示是在停宿主的过程中写的 | 退出时先停会话，再停适配器。会话 actor 收到 `Stop` 后，把宿主自己停掉的适配器当成崩溃（K7）处理 | actor 收到 `Stop` 后记下 `stopping`，之后的 `AdapterExited` 不再交给回合机。强制退出时回合已经由 `HostExit` 以 `interrupted` 结束；非强制退出只在没有回合运行时发生 | `actor.rs` 增 8 |
| 2 | 孤立退出用的是上一次的观察：父进程退出后，宿主在回合还在运行时就有序退出了。IT-host-orphaned 修之前 3/3 失败。真实时钟下的窗口是 `supervisor_observe`（1 s），也就是这 1 s 里开始的回合会被丢下；安装器送来的 `BundleChanged` 也有同样的问题 | `Watcher::observe` 先送 `Orphaned`，`WatchCore` 当场用旧的观察做决定 | `WatchCore` 收到 `Orphaned` 或 `BundleChanged` 时，关闭新会话和新回合，同时丢掉旧的观察，等下一次观察再决定是否退出。UNIT-orphan-policy 里三处"当场退出"改为"下一次观察时退出"，并新加一条"关闭前的观察不算" | `watch.rs` 增 4，`watch/tests.rs` 增 10 删 4 |

两处都没有改接口。

## 关键取舍

- **SP-4a 里依赖未完成功能的断言没有硬迁**：工具输出转 blob 等 T175 合并；改走 `text_blob` 验证 blob 传输，没有为了让测试通过去断言当前的错误行为。
- **停读的客户端用 testkit 的原始套接字**：CLI 库的读任务总在读，无法造出"不读的客户端"。
- **慢客户端的脚本改为 `flood 5000 1000`**：原来的 `flood_mixed` 太慢，见遗留第 2 条。
- **孤立的修法放在纯核心里**，不只调换 `observe` 里的顺序：这样关闭后读到的工作量才算数，待升级的退出也一并修好。
- **`dev-auth` 的分支**：通知意图里 desktop_ui 那一段用 `cfg!` 判断，孤立用例用 `#[cfg]`，非 `dev-auth` 构建里也能编译、能运行。

## 验证方式

- `cargo test -p cupper-host --features test-harness,dev-auth --test gateway_sync` 连跑 10 次，12/12 全绿（每次约 10–12 s）。跑完检查本 worktree 的宿主、假 agent 和 `/bin/sleep 3600`，都没有残留。
  - 第一轮 10 次里第 9、10 次 IT-NFR2-epoch 失败，是测试的断言写错了，不是产品缺陷：它假设断电后重跑的回合事件数不少于原来，但一次附加有时会连写两条 `session.capabilities`。已改为只断言 seq 从 M+1 起重用，然后重新连跑 10 次全绿。
- 不带 `dev-auth`（只有 `test-harness`）跑 3 次，11/11 通过。
- `cargo test -p cupper-host --features test-harness,dev-auth` 全部通过；workspace 的 fmt 和 `clippy --all-features --all-targets` 都没有问题。
- 在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T062-it-gateway-history/ci/`，提交 23895ec 上通过：
  - rust 1946 项、protocol 185 项、ts 471 项、license、testdata-scan、ui、e2e 全部 pass；
  - 摘要里 IT-gw-sync、IT-NFR2-epoch、IT-history-from-host、IT-notification-intent、IT-host-orphaned、UNIT-orphan-policy 都是 pass；
  - 摘要已随任务提交。跑 CI 期间没有往工作区写任何文件。
- SP-4a 1–7 与新用例的逐条对照、就地修的说明、按用例的行数统计都写在 `docs/tasks/T062-it-gateway-history/notes.md`。

## 提交列表

```
490973f docs(task): T062 CI summary
23895ec docs(task): T062 counts, fixes and the SP-4a comparison
f8eff07 test(host): IT-NFR2-epoch checks reuse from M + 1, not a head past N
2e674d9 test(host): IT-notification-intent and IT-host-orphaned
c719b80 fix(host): the orphaned and upgrade exits wait for a fresh look
62ef664 test(host): IT-NFR2-epoch and IT-history-from-host
e454630 test(host): IT-gw-sync, SP-4a's end-to-end tests on the host process
c1fcca3 fix(host): no agent_restarted notice when the host stops the adapter
d959030 test(host): share more fixtures of the process-level tests
50ce109 docs(task): estimate T062 by case
```

## 遗留问题 / 范围外发现

1. **超过一帧的工具输出会让会话无法订阅**（等 T175）。`tool_done t1 500000` 会写出一条约 500 kB 的 `item.completed`：
   - 在线的客户端收到 `stream/reset {rebuilt}`，重订之后再也收不到这个会话的事件；
   - 新订阅的客户端收到订阅结果后，连接就被关闭（`Eof`）；
   - 原因是"超过内联上限转 blob"归 T175，还没合并，宿主里 `MAX_INLINE_ESCAPED` 没有调用方。

   建议 T175 的 IT-orch-long-output 加一条进程级检查："500 kB 工具输出之后，新客户端能订阅、帧不超限"；或者 T175 合并后，把 SP-4a t1、t7 的 blob 断言补回 `gateway_sync.rs`。
2. **`flood_mixed` 每个条目约 3.5 ms，线性增长，与每块长度无关**：
   - 2,000 个交替的消息与思考要 7 s；
   - SP-4a 用的 `flood_mixed 6000 200` 在这里要 20 s 以上。

   是否达标请交给 NFR-1 的任务（T091）判断。
3. **孤立修法之后仍有极小的窗口**：关闭之前已经受理、但会话 actor 还没把 `running` 置上的回合，在读计数和受理之间有微秒级的空隙，可能看不到。要彻底消除，需要在受理时就计数，属于接口层面的改动，没有做。
4. **一次附加有时连写两条 `session.capabilities`**：没有坏处，只是同一回合的事件数不固定。
5. **`lifecycle.rs` 的 `it_gw_blob_put_and_get_through_the_gateway` 在机器高负载时失败过一次**：SIGTERM 后 30 s 内宿主没有退出。当时其他 agent 也在编译、跑测试。之后单跑 3 次、整个目标连跑 3 次都通过，没有复现，未深查。
6. **"同一段待处理期间第二个请求不再产生意图"在进程级造不出来**：假 agent 的请求是一个接一个发出的。这一条继续由 UNIT-index-intent 覆盖。
