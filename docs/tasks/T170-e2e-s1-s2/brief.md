# T170 M1b 端到端：S1 与 S2

- 派发日期：2026-11-23
- 执行者：子 agent（前端，ui-core；T171、T153、T268 的执行者）
- 分支：`test/T170-e2e-s1-s2`
- 依赖：T144、T254（都已合并）。T179 依赖本任务

## 范围

**做**（计划 T170 一行；设计 5.9.4 的"M1b 的端到端"；PRD 的 S1、S2 场景；写法照你做过的 T171）：

1. **E2E-S1**（已登记）：并行开发。
   - 用假 agent，claude 与 codex 两种 persona 各一个会话，各在一个新建的 worktree 里（T144 的新建会话页）；
   - 两个会话同时运行，侧边栏同属一个项目，各带 agent 标识，状态正确：一个停在审批上显示"需要处理"，另一个"运行中"；⌘⌥A 跳到需要处理的那个；
   - 两个回合结束后都是"完成未读"，点开一个变为已读，另一个不受影响；
   - 两边的改动各在自己的 worktree 里（假 agent 的 `write_file`），主工作区干净。
2. **E2E-S2**（已登记）：审批与提问。
   - 请求面板上的审批：Enter 批准、Esc 拒绝，记录行写明结果；
   - 提问（或计划确认）：回答后 agent 继续；
   - 通知意图的记录：窗口不在前台时，请求和回合结束各记一条 `notification.intent`（不弹真实通知）。
3. **写法**：
   - 场景用例串起各功能的主要流程，不重复各功能用例里的细节断言（请求面板、presence、⌘⌥A 各自已有用例）。
   - 复用已有的辅助函数（`activatePanel`、`panelReady`、`asHarness`、`startingPrefs`、`turnRunning` 等）；注意交接事项里写 e2e 的几条（偏好经 shell 设置后要等界面体现；点完 Radix 菜单项先等菜单卸载；`press` 不发 `char`）。
   - 人工检查表 `docs/manual/man-s1.md`、`man-s2.md`：自动用例按同样的顺序走主要步骤，只覆盖假 agent 能做的部分。

**不做**：
- 用真实 agent 的人工检查表（MAN-S1、MAN-S2，T179）；
- 产品代码的修改：用例暴露出产品缺陷时，先报告，由 `main` 决定。

## 注意：nightly 的 focus 用例

T270 正在修 nightly 里挂的 E2E-NFR1-3（`activatePanel` 按十次 Tab 到不了请求面板，可能是顶栏多了可聚焦的按钮）。E2E-S2 要用键盘回答请求面板时，用会抢焦点的写法就只在 runner 上跑；如果碰到同一个问题，先告诉 `main`，不要在本任务里改 `activatePanel`。

## 验收标准

- [ ] E2E-S1、E2E-S2 通过，并在 runner 上各连跑 10 次（由 `main` 触发 e2e-repeat，同时给 `spec` 和 `test`）。
- [ ] 没有为了用例通过而加 sleep，只等界面或宿主确认的状态。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接。

## 规模

- 开工前先按场景估一遍报给 `main`。
- 停止线：300；硬停止线：360。每完成一个场景就跑一次 `cargo xtask size`。

## 参考资料

- 设计：5.9.4、5.9.9（测试替身、假 agent 的脚本命令）、5.7.8（请求面板）、5.7.9（通知）。
- 卷宗：T171（S3、S4 的写法）、T144（新建会话页、worktree）、T153（⌘⌥A、需要处理）、T250、T254（通知意图）、T257（presence）。
- 交接事项：`docs/tasks/handoff-notes.md` 里接收任务含 T170、"写 e2e 的任务"的各行。

## 注意

- 本机不跑会抢焦点、发真实按键的用例（守则第 10 条）。
- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T043`，编排者已切到新分支 `test/T170-e2e-s1-s2`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
