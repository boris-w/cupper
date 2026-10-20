# T059 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增 933、删除 33（不含 docs，测试约 330），在约 1,300 的上限之内。开工前估全量约 1,650，审计拆给 T138（计划 v1.3.31）。
- **CI**：执行者的摘要 `pr-815c94c59b3c.json` 通过，四个编号都在结果清单里。合并 main（含 T008 修复 1、T081、T135、T137）没有冲突，clippy 干净；之后在本 worktree 重跑 `cargo xtask ci pr --record`，`pr-a10e7fc77b64.json` 通过（rust 1845、protocol 185、ts 424、license、testdata-scan；ui 被路径过滤跳过）。
- 与 T138 的分界守住了：没有碰 `gateway/audit.rs`、审计写入、`store/retention.rs` 和审计接收端的去向。

## 逐项意见

1. **监护核心 `WatchCore`**：纯核心，规则与优先级（待升级 > 孤立 > 空闲）清楚，退出后不再有动作，第一次观察之前不判定。UNIT-orphan-policy 四组表驱动用例覆盖。
2. **两处取舍认可**：
   - 暂停中的排队消息记为 0：否则被中断后暂停的消息会让空闲退出和待升级退出永远等下去；消息在库里，退出不丢。
   - 孤立时不等待处理请求：孤立只出现在 `dev-auth` 构建，父进程已经不在了，没人会去回答。
3. **准入**：共享的 `Admission`，会话管理器对 `session/create`、`session/send` 回 `unavailable`；待升级带 `pending_upgrade`，孤立不带 reason，原因记进 host.log（按裁定）。
4. **自检**：挂在 accept 之后和适配器拉起之前；`own_cdhash()` 读不到时不做。IT-self-check 用测试自己复制出的可执行文件，删掉后：
   - 新连接触发待升级；
   - `session/create` 被拒；
   - `host/status` 和 `host.updated` 都报待升级；
   - 回合结束后以 `upgrade` 退出。
   - 覆盖完整。
5. **清理残留**：
   - 读未结束的 `adapters` 行，取适配器自己的进程组和 `groups`；
   - SIGTERM 后按 `Clock` 轮询，2 s 后 SIGKILL，只发给仍是原组的组；
   - 标为 `reaped`，排在 Maintenance 之前。
   - 用例并进 IT-lifecycle 的启动用例，和计划一致。
6. **给 T129 的接入点**：`Watch::bundle_changed()`；安装器在 `Watch::new()` 之后启动；`Observed.installer_busy` 留给 T129 填。
7. **`routes.streams` 接上了**：IT-idle-exit 期间订阅了一次索引流，可用。

## 记下的问题

- **中断回合的回复可能丢失**：中断命令的回复与它引起的待升级退出赛跑，回复可能来不及发出。命令结果在账本里，客户端重连后用同一个 `command_id` 重试能拿到。可以接受，记交接给 T090（e2e 里不要断言一定收到这条回复）。
- **进程级用例失败时宿主残留**：已交给 T063（T008 修复 1 的交接），执行者开发中留下的残留已自行核对并清理。
