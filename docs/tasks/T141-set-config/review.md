# T141 评审

- 评审人：编排者
- 日期：2026-11-03
- 结论：**通过，合并**（PR #57）

## 核对

- **规模**：+470 −30。开工前估约 760，上浮后 900–990；做完宿主时偏高 38%，推算整个任务会越过 1,000，按事先定的规则拆分：本任务只做宿主，界面与 E2E-full-access-native 移到 T231。
- **CI**：PR #57 https://github.com/boris-w/cupper/pull/57 ，0fd0cbc 上 8 个检查全绿。
- **个人信息**：改动的文件过了敏感词清单，干净；agent 拒绝时回给客户端的 message 已脱敏。

## 逐项意见

1. **档位规则逐格测到**：IT-set-config-tier 把 5.8.2 的表每格都走了一遍，允许的格子核对 `mode_set_by` 和 `full_access_confirmed_at`；SEC-9（remote_device 不能放宽）也在内。
2. **回复晚于流**：会话用回复里的选项写 `session.configured`，再在同一次提交里记账本、回复，客户端拿到回复时流和行已经是新配置，避开了竞态。
3. **未连接时只记下、下次 attach 时应用**，并更新 `config_json`，resume 不会退回创建时的值。
4. **`AgentPort::set_config` 改成带回复**，原来不回复的 `DriverCommand::SetConfig` 只留给 TierGuard 改回档位时用。

## 记下的点

- **设计**（本次合并时改）：02 的 `session/set_config` 一段写明带回复、回复晚于 `session.configured`、未连接时记下并在下次 attach 时应用、成功的值进入会话配置。
- **交接事项**（已写入 handoff-notes）：attach 已发出、还没返回时的修改要等下一次 attach 才应用，概率低，可作后续小修。
- T231 交给同一个执行者。
