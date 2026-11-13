# T258 通知授权状态的重新查询

- 派发日期：2026-11-13
- 执行者：子 agent（宿主与桌面；T150、T250、T251、T254 的执行者）
- 分支：`fix/T258-notification-auth-recheck`
- 依赖：T250（已合并）。T179 依赖本任务

## 来由

T254 的真实复核发现：宿主只在启动时查询通知授权。用户在系统设置里打开通知后，`host.updated.notification_auth` 一直停在 `denied`，T147 设置页的"去系统设置打开"提示也一直显示，直到宿主重启。

## 范围

**做**（计划 T258 一行；设计 5.2.9 第 5 条、5.3 的 `host.updated`）：

1. **开工前先定方案并报告**，说明理由再动手：
   - 什么时候重新查询，候选有：桌面窗口回到前台；设置页打开；有客户端连上；要弹通知之前；定时。
   - 由谁触发：宿主自己判断，或者桌面经 control 连接请宿主查。后者要附加协议方法，尽量避免。
   - T257 正在做 `client/presence`，"窗口回到前台"这个信号由它上报，可以考虑复用。T257 还没合并时，怎样不互相等，也一并说明。
2. **实现**：查询结果变化时写 `host.updated`；被拒以后仍不重复申请授权。
3. **用例**：IT-notifier-auth-recheck（已登记）。record 后端模拟授权状态从 denied 变成 authorized，按选定的时机重新查询后，`host.updated` 跟着变。

**不做**：设置页提示的界面（T147 已有）；申请授权的时机（T250 已定）。

## 验收标准

- [ ] IT-notifier-auth-recheck 通过，宿主其余用例照常通过。
- [ ] 宿主的约束都守住：时间只经 `Clock`、`unsafe` 只在 cupper-sys、协议只做附加。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 停止线：200；硬停止线：260。

## 参考资料

- 设计：5.2.9（授权、在场）、5.3（`host.updated`、`client/presence`）。
- 卷宗：T250（notifier、授权）、T254（复核记录）、T257（在场，并行进行中）。

## 注意

- 不在本机碰通知服务和授权，本机一律 record。
- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令。

## 工作方式

worktree 是 `.claude/worktrees/T150`，编排者已切到新分支 `fix/T258-notification-auth-recheck`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
