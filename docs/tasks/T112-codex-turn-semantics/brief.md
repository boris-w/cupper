# T112 Codex 回合语义集成测试

- 派发日期：2026-11-03
- 执行者：子 agent（Rust 工程师，驱动层与运行时）
- 分支：`test/T112-codex-turn-semantics`
- 依赖：T111（Codex 接入）、T226（`error` 条目）

## 背景

T110 做了 Codex 的提取层，T111 把它接进运行时，T226 让失败的回合写 `error` 条目。Codex 的几处已知行为（06 的 K 表）现在还没有进程级的用例守着：拒绝审批会结束整个回合、取消后命令不停、取消不撤回审批、app-server 被杀后可重启、计划确认时线程已是 idle、文字形式的错误、上游过载重试。T112 用 codex 形态的假 agent 把这些情形逐个走一遍，缺陷按计划 1.2 的上限就地修。

## 范围

**做**（计划 T112 一行，06 的 K 表）：
1. 进程级用例（codex 形态的假 agent），编号都已登记：
   - **IT-K2-reject-ends-turn**：拒绝审批只有 `cancel`，整个回合以 `cancelled` 结束；选项标 `ends_turn: true`；这种取消不暂停队列。
   - **IT-K3-late-output**：中断时还在跑的命令标 `abandoned` 加 `may_still_run`；晚到的输出写成当前回合里的 `late_tool_output` 条目，原回合不改。
   - **IT-K4-cancel-pending-approval**：取消时宿主把挂着的审批标为 `cancelled`，并替用户应答 `Cancelled`。
   - **IT-K7-restartable**：app-server 被杀，返回 `1001`、`restartable: true`；回合标为 `failed` 并写 `error` 条目（guidance `retry`）；下一个 prompt 由适配器自动重启。
   - **IT-K12-plan-confirm-idle**：计划确认请求发出时线程已是 idle；有 prompt 在途时回合以 prompt 响应为准，挂着的交互请求不失效。
   - **IT-K17-text-error**：收到 `systemError` 时，把那段文字转成这个回合的 `Failure`，由回合机的 `end()` 统一写 `error` 条目（T226 的交接：每个回合一条，复用 `failure::guidance()` 和 `orchestrator::instant()`），回合带 `error_code`（如 `model_unsupported`）。
   - **IT-K18-upstream-retry**：上游过载自动重试时写 `notice: upstream_retry` 显示进度。
2. 交接事项里写给 T112 的条目：
   - K14 在会话打开时的改判：codex 形态打开时的认证错误（T063 的交接）；
   - K17 的错误条目（T110、T226 的交接）。
3. 用例跑出来的缺陷按计划 1.2 的上限就地修；超出上限的列进汇报，由 `main` 决定。
4. 需要扩展假 agent 的 codex 形态时，按现有的写法加。

**不做**：真实 Codex CLI 的兼容运行（T113，维护者手动，消耗额度）；界面。

## 验收标准

- [ ] 七个用例通过。
- [ ] 就地修的缺陷各有用例守着，汇报里逐条列出。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：600；硬停止线：750

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：06 的 5.6.9（K 表：K2–K4、K7、K12、K14、K17、K18）、5.2.6（回合状态机、晚到的工具输出、崩溃表）、5.5（时序）。
- 计划：T112 一行，1.2（就地修的上限），1.6。
- 卷宗：T110、T111、T226、T063、T218、T224、T227（`ManualClock` 用例的写法和教训）。
- 交接事项里写给 T112 的条目。

## 注意

- **不运行真实的 Codex CLI 或 Claude Code**；不读不改 `~/.codex`、`~/.claude`。只用假 agent。
- 本机只单份跑（守则第 10 条）；要在负载下验证的用 `test-repeat`，由 `main` 触发。
- `ManualClock` 上的用例，听到中途事件后先等运行时做完那一步再推进时钟（T224 的教训）；合并窗口会扣住 RPC 应答（T227 的教训）。
- 子进程经 `cupper-sys::spawn`；时间经 `Clock`；每条事件流只有一个写者。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
