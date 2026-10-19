# T087 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：本任务由 T071 拆出，执行者与 T071 是同一位。编排者把 main 合入分支（`4274103`），在任务 worktree 里跑 `cargo xtask ci pr --record`：rust 1073、protocol 176、ts 301、license 1、testdata-scan 2，全部通过，摘要为 `ci/pr-427410392e98.json`。
- 核对：没有新依赖，没有个人信息。
- 审阅：
  - `StreamSync` 是纯核心，规则与 Rust 的 Mirror 逐条对应；视图放在调用方的 `StreamSink` 里，由 T072、T080 实现。
  - 只用 T071 留下的接入点：`stream/subscribe` 经 `call`，结果在通知流里的位置不变；收到 `ready` 后，带着游标重新订阅。
  - 事件和 op 都先当作 `unknown[]` 取出，再逐个 `safeParse`：一个残缺事件只影响它自己。
  - 计数规则：同一 `data_epoch` 下按游标续传时，三种计数都是 0；`epoch_changed` 时 `resets` 记 1，重复使用的 seq 不算 `duplicates`。
  - 订阅结果格式不对时，交给 `sink.failed`，流停在等待状态。渲染进程不持有连接，重连也只会拿回同样的结果，所以不断开。接受。
  - 变异检验：改坏 5 处，每处都有用例失败。
- 规模：609 行（代码 302 行，测试 307 行），在上限以内。
- 转交事项已写进 `handoff-notes.md`：T072、T080。
