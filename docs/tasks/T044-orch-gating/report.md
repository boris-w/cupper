# T044 汇报

- 执行者：子 agent（Rust 工程师，宿主安全）
- 分支：`feat/T044-orch-gating`
- 完成日期：2026-10-20

收窄后的 T044 已完成，IT-orch-gating 和 `cargo xtask ci pr` 都通过。规模 1,059 行，在约 1,500 的上限之内。

## 做了什么

1. **发送闸门**（`TurnMachine`，`Input::Gate`）。闸门关着时：
   - 新消息一律排队，队列暂停的原因写闸门的原因；空闲时也不开回合；
   - 处于 Preparing 的回合照常 attach，但不发 prompt；
   - `queue_resume` 解不开闸门造成的暂停；
   - 闸门打开时，只恢复因它而暂停的队列。

   T109 的 `project_config_changed` 直接复用这道闸门。
2. **档位核对**：新的纯核心 `orchestrator/tier_guard.rs`，接进 `SessionCore`。
   - 每次 attach 之后、agent 每次改模式时核对。两种情况进入 `config_unconfirmed`：`full_access` 却没有原生确认的记录（优先判这条），或档位高于设定者按 5.8.2 能设的上限。
   - 进入时：关闸门，写 notice、审计，`session.configured` 带上 `confirmation_required`。
   - attach 时就越权：第一个回合停在 Preparing 不发 prompt，等确认（也可以中断）。
3. **回合中越权**：
   - 先用 `SetConfig` 改回，等 2 s（`config_revert`，经 `Clock`，已登记进 `durations.rs`）；
   - agent 改回了，回合照常跑完；超时，或等待期间又报告越权的档位，就走中断流程；
   - 两种结局之后都进入 `config_unconfirmed`（设计第 3 步）。
4. **合法来源**：
   - 用户选中带 `tier_after`/`mode_after` 的选项后，选择者成为设定者；
   - `session/set_config`（M1b）按裁定预留了同一个入口，`facts` 已给它补上 `mode_tier`；
   - 经原生确认进入 `full_access` 时，记下 `full_access_confirmed_at`；`mode_set_by` 跟着设定者走。
5. **`session/confirm_config`**：
   - 已登记进 `Routes::sessions`，并删掉了允许表里 T044 那一行；
   - `config_digest` 是 `session.configured.config` 的 JCS 摘要，不相符时返回 `conflict/digest_mismatch`；
   - 确认后确认者成为设定者，闸门打开。
6. **`ConfigChanged` 现在会改写 `session.configured.config`**（以前没有处理）。只有别的选项变了、权限模式没变时，不重复核对。
7. **崩溃**：
   - 适配器退出、CLI 崩溃（`process_exited`）、`restartable`（K7）都写 `agent_restarted`；
   - K6：prompt 没有产生任何 Update 就收到 "Session not found" 时，resume 并重发一次，写 `session_resumed`；有过输出就按失败结束。
   - 这些 notice 不属于任何回合，视图会把它放进正在运行或最后一个回合。两个性质模型原来断言"条目都属于某个回合"，改为放行这种 notice。
8. **晚到输出**：逻辑在 `Coalescer` 和 judge 里早已有了，本任务在 IT 里核对整条路径。

## 关键取舍

- **attach 时越权，让回合停在 Preparing**，而不是取消回合。这样不出现"回合已开始又要收尾"，确认后消息照常发出。
- **改回成功也进入 `config_unconfirmed`**，按设计的三步顺序做：agent 自行提档本身就该让用户看一眼。
- **审计记录的连接改成可选**：新增 `AuditEvent::ConfigUnconfirmed`；会话自己发起的审计没有连接，所以 `AuditRecord.conn` 改成 `Option<ConnId>`，网关原有的记录都是 `Some`。这是跨模块的小改动，请知悉。
- **写 IT 时发现并修了一个 bug**：attach 之后核对档位时，审计记的、摘要算的还是旧配置。现在 attach 带来的配置先写进 `session.configured`，再核对。

## 验证方式

- **IT-orch-gating**：5 个用例，全部在 `ManualClock` 上跑，不睡眠。
  - `config_unconfirmed`：attach 在 full access 或高于设定者时触发，核对审计（`conn` 为空）、notice 和两个行字段；空闲发送只排队，`queue_resume` 不放行；摘要不符被拒，确认后队列运行。
  - 回合中越权：改回被接受、到点中断、再次越权中断，三种结局各一遍。
  - 选项让选择者成为设定者，没有改回、没有闸门。
  - 适配器退出后 resume；K7；K6 的重发只发生一次，有输出后不重发。
  - 中断后的 `may_still_run`，以及晚到输出落在自发回合里，`ref` 指回原条目。
- `cargo test -p cupper-host`（不带特性和 `--all-features` 两种）全部通过，含两个性质模型。clippy（`--all-targets`，含 `--all-features`）无警告。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T044-orch-gating/ci/`，全部通过（rust 1555、protocol 185、ts 329、license、testdata-scan），IT-orch-gating 为 pass，摘要 `pr-15015b4d8d90.json` 已提交。
- **磁盘满**：第一次跑 CI 时撞上磁盘满（`ENOSPC`）。我删了自己 worktree 里的增量编译缓存和 release 构建，约 15 GB，之后重跑通过，与您说的情况一致。

## 规模

1,059 行：实现约 720，测试约 340。估计约 1,140。逐提交数字在 `notes.md` 里。

## 提交列表

```
0bd6959 docs(task): T044 pr CI summary
15015b4 docs(task): T044 line counts, choices and handoffs
f758ace fix(host): a change of another option does not recheck the same mode
d17089d test(host): IT-orch-gating
3f6ffb4 fix(host): an attach's configuration is in place before its tier is checked
6a5c942 feat(host): check the session's tier, revert a raise during a turn, confirm_config
8a65300 feat(host): the turn machine's gate, K6 resend and restart notices
c59e0ac docs(task): T044 plan after the split
d8076d5 docs(task): T044 size estimate, split proposal and open questions
```

## 遗留问题 / 范围外发现

- **T055**：改回是经 `AgentPort::send(DriverCommand::SetConfig{..})` 发出的，结果要以带全部选项的 `DriverEvent::ConfigChanged` 回到会话。`AgentPort::send` 的文档目前只列了 Prompt、Cancel、Close、Answer，接线时请一并补上。
- **T109**：
  - 闸门用 `turn::Input::Gate(Some(PauseReason::ProjectConfigChanged))` 关，用 `Gate(None)` 开。
  - 闸门同一时刻只有一个原因；两种原因可能同时出现时，要改成原因的集合。
- **T089（从库恢复）**：设定者的完整 `Principal` 只在内存里，库里只有 `mode_set_by` 的 ID。恢复会话时需要按 ID 找回 principal 的类型和 scope，或者把恢复的会话一律视为由创建者设定。
- **可能的合并冲突**：T055 正在组装会话管理器，`SessionCore::new` 多了一个参数（设定者，以及是否已确认 full access），`manager.rs` 的 `create` 也有改动。

worktree 和分支都保留着。