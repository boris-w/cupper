# T062 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：`crates/` 内增 1,188、删 80，净增 1,108（开工前估约 1,170，含就地修的预留），在约 1,300 的上限内；按用例的数字在 `notes.md`。
- **CI**：执行者的摘要（提交 23895ec 上）通过。合并 main（含 T091、T169、T175）时 `actor.rs` 有两处冲突：本任务加的 `stopping` 与 T169 加的 `initial` 两个字段，编排者两边都保留，编译与 fmt 通过。合并后 `ci pr --record` 通过：`pr-1706cb33a927.json`（rust 1987、protocol 185、ts 471、license、testdata-scan、e2e 13；ui 按路径过滤跳过）。
- **验收**：五个编号共 12 个测试，`gateway_sync` 连跑 10 次全绿（第一轮第 9、10 次 IT-NFR2-epoch 失败是断言写错，改为只断言 seq 从 M+1 起重用后重跑 10 次全绿）；不带 `dev-auth` 跑 3 次通过；跑完无残留进程。

## 就地修的两处缺陷（计划 1.2 的上限内）

1. **宿主每次有序退出，所有连过适配器的会话都多一条 `agent_restarted` 提示**：退出时先停会话、再停适配器，会话 actor 把宿主自己停掉的适配器当成崩溃（K7）。actor 收到 `Stop` 后记下 `stopping`，之后的 `AdapterExited` 不再交给回合机。增 8。IT-history-from-host 修之前 5/5 失败。
2. **孤立与待升级退出用的是上一次的观察**：父进程退出后，宿主可能在回合还在运行时就有序退出（真实时钟下窗口是 1 s 的 `supervisor_observe`）；安装器的 `BundleChanged` 同样。`WatchCore` 收到 `Orphaned` 或 `BundleChanged` 时先关闭新会话和新回合、丢掉旧观察，等下一次观察再决定。修法放在纯核心里，待升级的退出一并修好；UNIT-orphan-policy 里三处"当场退出"改为"下一次观察时退出"，新加"关闭前的观察不算"。IT-host-orphaned 修之前 3/3 失败。

## 逐项意见

1. **SP-4a 迁移**：t1–t7 加适配器隔离共 8 个测试，原型只作蓝本、不依赖；与设计不同处逐条写在 notes。依赖未完成功能的断言没有硬迁（工具输出转 blob 等 T177），没有为了通过去断言当前的错误行为。
2. **IT-NFR2-epoch** 覆盖设计 5.9.8 的 ①–④，断电用 `VACUUM INTO` 拍下中途的库再换回，换开机 UUID 重启，做法到位。
3. **IT-history-from-host**：22 个回合、有序重启后新客户端经窗口加 `history/page` 拿全，没有拉起新适配器，证明历史来自宿主。
4. **IT-host-orphaned** 只在 `dev-auth` 构建编译；宿主不是测试的子进程，守卫按 pid 加启动时间核对再发 SIGTERM。
5. **共用夹具**继续收进 `tests/common/`，`lifecycle.rs`、`interaction_trust.rs` 照常通过。

## 记下的点

1. **超过一帧的工具输出会让会话无法订阅**：约 500 kB 的 `item.completed` 让在线客户端重置后再也收不到事件、新订阅者被断开。原因是"超长输出转 blob"还没做，已写进 T177 的验收（进程级检查，并把 SP-4a t1、t7 的 blob 断言补回 `gateway_sync.rs`）。
2. **`flood_mixed` 每条约 3.5 ms、线性增长**：2,000 条要 7 s。性能是否达标交给 T117（宿主性能测量）判断，记交接。
3. **孤立修法后仍有微秒级窗口**：关闭之前已受理、actor 还没置上 `running` 的回合可能看不到；彻底消除要在受理时计数，属接口层面改动，记交接给 T069。
4. **一次附加有时连写两条 `session.capabilities`**：无害，同一回合的事件数因此不固定，记交接。
5. **`lifecycle.rs` 的 `it_gw_blob_put_and_get_through_the_gateway` 高负载下失败过一次**（SIGTERM 后 30 s 没退出），未复现，记交接给 T069 留意。
6. **"同一段待处理期间第二个请求不再产生意图"进程级造不出**，继续由 UNIT-index-intent 覆盖。
