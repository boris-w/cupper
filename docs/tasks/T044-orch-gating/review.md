# T044 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：
  - 开工前估计约 1,700 行，执行者停下汇报并提出 5 个问题，拆分（计划 v1.3.19）：信任门控归 T109；崩溃与晚到输出并进本任务。
  - 合并 main 时 `orchestrator/mod.rs` 有一处模块声明冲突（main 的 `pub(crate) mod testkit` 与本分支的 `tier_guard`），都保留；与 T055 的会话管理器改动没有语义冲突，`cargo build --all-features --tests` 通过。
  - 合并后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-2fe2ceb05989.json`。执行者那次的 `pr-15015b4d8d90.json` 一并保留（它的第一次运行撞上本机磁盘满，删掉自己 worktree 的增量缓存后重跑通过）。
- 核对：没有新依赖；没有个人信息。
- 审阅：
  - **发送闸门**：闸门关着时新消息一律排队、空闲时不开回合、`queue_resume` 解不开、闸门打开只恢复因它而暂停的队列；T109 直接复用。
  - **档位核对是纯核心**（`tier_guard.rs`）：`full_access` 却没有原生确认的记录优先判；attach 时就越权，第一个回合停在 Preparing 不发 prompt，等确认，避免"回合已开始又要收尾"。认可。
  - **回合中越权**：先 `SetConfig` 改回、等 2 s（经 `Clock`），超时或再次越权就中断；两种结局之后都进入 `config_unconfirmed`（按设计的三步）。
  - **写 IT 时修了一个 bug**：attach 之后核对档位时审计和摘要用的还是旧配置，现在先写 `session.configured` 再核对。
  - **`AuditRecord.conn` 改成 `Option<ConnId>`**：会话自己发起的审计没有连接，网关原有的记录都是 `Some`。跨模块的小改动，T059 落盘审计时要处理 `None`。
  - K6 只在 prompt 没有产生任何 Update 时 resume 重发一次；有过输出就按失败结束。认可。
- 规模：1,059 行（实现约 720、测试约 340），在收窄后约 1,500 的上限以内。
- 转交事项已写进 `handoff-notes.md`：T055 之后的接线（`AgentPort::send` 的文档补 `SetConfig`）、T109（闸门的用法、原因改成集合）、T089（设定者的 principal 只在内存里）、T059（`AuditRecord.conn` 可为空）。
