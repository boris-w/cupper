# T257 界面上报在场（client/presence）

- 派发日期：2026-11-13
- 执行者：子 agent（前端，ui-core；T244、T242、T149、T171、T256 的执行者）
- 分支：`fix/T257-client-presence`
- 依赖：T254（已合并）。T179 依赖本任务

## 来由

T254 的真实复核发现：ui-core 从不发 `client/presence`，宿主不知道窗口在不在前台、正在看哪个会话。按设计 5.2.9，前台的 `desktop_ui` 正在看的会话不弹通知；现在用户正看着一个会话时，它的审批和回合完成也照样弹。

## 范围

**做**（计划 T257 一行；设计 5.2.9 的"在场"和"通知意图"，5.3 的 `client/presence {foreground, session_id?}`）：

1. **上报时机**：窗口获得或失去焦点、当前会话变化（切会话、进出设置页或新建会话页）时上报一次，内容是 `{foreground, session_id?}`。
   - 不在会话页时不带 `session_id`。
   - 连接（重新）建立后补报一次当前状态。
   - 同样的状态不重复发。
2. **从哪条连接发**：按设计和宿主现有的实现（T100 的 `IndexActor`）确认。宿主只采信 `desktop_ui` 的在场。
3. **用例**：
   - **E2E-presence-suppresses-notification**（已登记）：record 模式下，窗口在前台看着会话时，这个会话的通知意图不记通知；切到别的会话或窗口失焦后再来一条，就记。
   - 窗口焦点在 e2e 里怎样可靠地制造，先看现有的 `@focus` 用例和 helpers。需要真实焦点就标 `@focus`，只在 runner 上跑。
   - 组件或单元测试：上报的去重、连接重建后补报。

**不做**：宿主侧的判定规则（T100 已有，不改）；CLI 的在场。

## 验收标准

- [ ] E2E-presence-suppresses-notification 通过，并在 runner 上连跑 10 次（由 `main` 触发 e2e-repeat，同时给 `spec` 和 `test`）。
- [ ] 渲染进程的约束都守住；协议不改。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 开工前先估一遍。停止线：200；硬停止线：260。每完成一个模块统计一次。

## 参考资料

- 设计：5.2.9（在场、通知意图）、5.3（`client/presence`）、5.7.6（状态管理）。
- 卷宗：T100（索引的在场与通知意图）、T250、T254（通知的 record 模式、`test/notification_click`）。
- 交接事项：写 e2e 的几行。

## 注意

- 本机不跑会抢焦点的用例（守则第 10 条）。
- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令。

## 工作方式

worktree 是 `.claude/worktrees/T043`，编排者已切到新分支 `fix/T257-client-presence`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
