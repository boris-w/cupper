# T019 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：
  - 分支开出之后，main 合进了 T087、T072 和几份文档，没有冲突；
  - 合并后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过：rust 1110、protocol 185、ts 314、license 1、testdata-scan 2，摘要是 `ci/pr-c12f89ca52d8.json`；
  - 执行者合并前的那次 `pr-627439e40278.json` 一并保留。
- 核对：没有新依赖，没有个人信息。
- 审阅：
  - **策略不绕开**：三个方法照常走分发管线，顺序是 `admit`（方法权限、参数校验）→ 在途上限 → 目标。参数校验用的是 serde 把参数解成 `M::Params`，所以分发里 `typed(..).expect("validated")` 不会 panic。
  - **订阅顺序**：每条连接一个订阅 task，subscribe、unsubscribe 按到达顺序生效，断开时随 `Dispatch` 一起清掉。用例覆盖了"紧跟的 unsubscribe 晚于 subscribe 生效"。
  - **流锁内只入队**：快照和补发都只在流锁里入队，编码和读日志放在写循环里做，符合 `Sink`、`start` 只能排队的约束。
  - **缺口与慢客户端**：补发时读到缺口，回 `stream/reset`（`rebuilt`）；积压超过 4 MiB 时结束 feed（`slow_consumer`），带游标续传能补齐。两种情况都有用例。
  - **分帧**：快照的分页预算是帧上限减去最长的信封；客户端解码器拒收超过 64,000 字节的帧，用例全部在上限内收到。
  - **合并**：`stream.coalesce` 只对协商了的客户端生效；没协商的客户端照样一批收到，只是不合成区间事件。
- 规模：1,552 行（实现 858、测试 694），比 L 档的 1,500 上限多 3%。实现比估计少，多出的全是测试，而且是测试写完才超出，接受。
- 转交事项已写进 `handoff-notes.md`：
  - T037：组装时调用 `routes.streams(..)`；
  - T062：测试夹具补 `Routes::streams`；
  - T029：outbox 的项现在是 `Item`；
  - CLI、iOS：合并窗口会拖慢同一批里的响应；
  - 对不存在的会话订阅，会把一条空流留在内存里。
