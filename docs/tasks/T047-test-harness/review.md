# T047 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：
  - 开工前估计约 1,900–2,150 行，执行者停下汇报，拆成三份（计划 v1.3.21）：录制导出归 T116，性能测量与趋势归 T117。5 项待裁定全部认可。
  - 合并 main 时与 T009、T044 有 6 处冲突，编排者保留两边：`AgentRuntime` 同时有 T009 的命令列表、缓存、登录状态和本分支用于 `idle` 的 `participant`；分派测试同时登记 `test/*` 与 `agents/*`；会话 actor 在 `idle` 里等，agent 事件交给 T044/T009 的 `agent()`；宿主用 main 的 `Finder` 加本分支的 `injected.login_env`。
  - 合并后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-d953fcea2454.json`。执行者那次的 `pr-1908c99677c9.json` 一并保留。
- 核对：没有新依赖；`test/*` 只在 test-harness 构建里服务；没有个人信息。
- 审阅：
  - **屏障用唤醒器判定忙闲**：任何唤醒先记为忙，内层 poll 返回 Pending 才记为闲；唤醒发生在 poll 与"记为闲"之间按唤醒计数判定，不会误判。覆盖所有消息来源和定时器，各 actor 只需把等待点包进 `idle`。
  - **会话 actor 和运行时拆开"等"和"做"**：处理中途等写线程或读池时保持忙，屏障不会在处理中途误判静止。
  - **种类枚举用例**：启动真实的 `Host`，用它登记的种类和源码里所有 `ActorKind::new("…")` 比对，新增 actor 却没登记时用例失败。
  - **IT-clock-advance** 用一条只收了 `host/hello` 的连接验证握手期限、推进 49 小时验证账本清理；**IT-seed** 走正常写入路径（视图拒收就报错），并经 test-harness 专用入口让索引重新对账。
- 规模：净增约 946 行（只算 `crates/`），在收窄后约 1,500 的上限以内。
- 转交事项已写进 `handoff-notes.md`：新增 actor 的任务（登记种类、`idle`、`Hold`）、T116（登记 `diag/recording_export`、原始帧不持有 `Hold`）、T117（`test/seed` 先要有项目）。
