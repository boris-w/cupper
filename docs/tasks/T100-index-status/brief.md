# T100 索引 II：状态计算、已读、在场与通知意图

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主核心）。T037、T034、T047 的执行者接手，因为索引 actor 是它写的
- 分支：`feat/T100-index-status`
- 依赖：T034（已合并）。T046（CLI 命令）、T062 依赖本任务

## 背景

T034 开工前拆成两份（计划 v1.3.19，见 `docs/tasks/T034-index-actor/brief.md` 末尾的补充裁定）：T034 做了索引 actor 的骨架、项目与信任；本任务做会话状态计算、已读与在场、通知意图（设计 5.2.9）。

## 范围

**做**（计划 T100 一行；设计 5.2.9 的"状态""待处理请求的类别""已读""在场""通知意图"）：
1. **状态计算**：会话的状态（含 v1.4 的 `pending_kind`）与 100 ms 合并，写成纯核心；接在 `IndexActor::pending` 旁边；对账时按行补出的会话摘要只是起点（T034 的交接）。
2. **`IndexNotes` 扩充与会话 actor 的接线**：会话创建、回合开始与结束、标题、分支、归档、最近活动，经 `IndexNotes` 通知索引。
3. **已读与在场**：在 `Host::start` 里调用 `routes.presence(..)`（`session/mark_read`、`client/presence` 在那之前对真实宿主回 `unsupported`）；`mark_read` 只前移不后退；在场只采信 `desktop_ui`（安全建议 #16）。
4. **通知意图**：会话进入 `needs_action`、回合结束、出错时，此刻没有前台客户端在看，就写 `notification.intent` 事件（系统通知本身不做，T150）。
5. **`agents` 的接口**：`IndexHandle` 实现 `AgentNotes`，宿主组装时换掉 `NoAgentNotes`，`agents` 状态写进索引流（T009 的交接）。

**不做**：系统通知（T150）；通知意图的进程级用例（T062）。

## 验收标准

- [ ] UNIT-index-status（含 `pending_kind`）；新增 UNIT-index-intent（直接向 IndexActor 输入状态变化）。
- [ ] `mark_read`、`client/presence` 在真实宿主上可用（用例或 IT-lifecycle 里调一次即可）。
- [ ] 新增的等待点包进 `Participant::idle`，脱离发起方的 task 持有 `Hold`（T047 的交接）。
- [ ] 不睡眠：用 `ManualClock`。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T100 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T100-index-status/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T100 的行）

T034、T009、T047。

## 规模

计划估为 M，T034 的执行者估约 780 行（按 1.7 倍约 1,050）。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项；推算超过约 1,100 行（包括贴线）就开工前停下汇报。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

## 参考资料

- 设计：`02-host.md` 5.2.9；`03-protocol-v1.md` 5.3.5 的索引流。
- `docs/tasks/T034-index-actor/`、`T009-agent-capabilities/`、`T047-test-harness/` 的 `notes.md`。

## 工作方式

T047 合并之后，在原来的 T037 worktree 里从最新的 main 开新分支：`git checkout -b feat/T100-index-status main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
