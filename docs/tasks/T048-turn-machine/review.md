# T048 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：
  - 本任务由 T040 中途拆出。`TurnMachine` 的实现在 T040 时写好（`e03a930`），由同一位执行者补完。
  - 编排者把 main 合入分支（`447039f`），然后在任务 worktree 里跑 `cargo xtask ci pr --record`：rust 567、ts 64、license 1，全部通过。摘要为 `ci/pr-447039fed893.json`。
- 核对：
  - 没有 `unsafe`，没有个人信息；
  - `cupper-protocol` 只附加了 `ResolutionReason::TurnEnded`，并带往返用例；
  - 执行者在设计 5.3.11 的取值表里补了 `turn_ended`，改动是附加性的，接受。
- 代码审阅：
  - `Settle { end, asks }` 把挂着的请求以什么原因结束，直接在纯核心里算出来，不让 T042 从回合结局去推断。5.3.11 的规则由此集中在一处，并由性质测试覆盖。
  - 性质测试找出了第一版的一个反例：Cancelling 中 prompt 返回错误时，请求应记 `turn_cancelled`。执行者已修正。
  - 性质测试的组织：
    - `World` 模型当场检查每个动作并作出回应；
    - 每次运行最后都要求所有回合结束、排队的消息恰好运行一次；
    - 四条 PROP 共用这套环境，各自只断言自己的规则。
  - 加了 `Stall` 操作，并调整了生成器的权重，"截止后先 close 再 resume"这条深路径的覆盖率从约 2% 提高到约 9%。
  - 手工植入 10 处缺陷，全部被抓到，而且是由各自负责的那条性质抓到的。
- 规模：1,561 行（实现约 783 行，测试约 778 行），比上限多约 4%。执行者把多行断言改写成单行，压到这个数，检查一条没少。接受。
- 转交事项已写进 `docs/tasks/handoff-notes.md`：
  - T042：按 `Settle.asks` 结束请求；`CancelAsks`；Cancelling 期间新到的请求。
  - T043：`item.completed` 的补齐、`final_item_ids`、`Attach` 的补全、队列满的映射、从数据库恢复。
  - T041：自发回合的进入点和退出点。
