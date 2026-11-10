# T149 排队消息的编辑与插话

- 派发日期：2026-11-10
- 执行者：子 agent（全栈：宿主会话编排与 ui-core 输入框；T244、T242 的执行者）
- 分支：`feat/T149-queue-edit`
- 依赖：T140、T242（都已合并）。T171、T148 依赖本任务

## 范围

**做**（计划 T149 一行；设计 5.2.6（排队）、5.3.4（`session/queue_edit`）、5.7.4（排队消息）；PRD CP-6 v1.3、F3）：

1. **宿主 `session/queue_edit`**：
   - **检查版本**：按 `expected_version`，对不上回 `conflict/version_mismatch`；`queue_id` 不在队列里回 `not_found`。
   - **`replace`**：改写这条消息，`QueueCore` 已有 `replace`（T040）。
   - **`steer`**：
     - 把这条消息从队列取出，立即插进当前回合，规则同 `session/send` 的 `mode: steer`。
     - 复用 T140 的纯核心判定和七个回退原因，含 `steer_pending`（交接事项里 T140 那一行）。
     - 回退时这条消息留在原位，结果的 `steer` 带 `disposition: queued` 和 `fallback_reason`。
     - `steer` 只能单独出现在 `ops` 里。
   - **`remove`、`move`**：回 `unsupported`（属于 CP-8，P1）。
   - **允许集合**：同步 UNIT-unsupported-set 的允许集合（设计 5.3 末尾），写明原因。
2. **排队消息行**（设计 5.7.4 的"排队消息"）：
   - **"插话"按钮**：tooltip"不打断模型，立即提交"，经 `queue_edit` 的 `steer`。失败时留在原位并提示原因；提示和回退复用 T242 的 `useSessionSend`（交接事项里 T242 那一行）。
   - **菜单**：
     - "编辑消息"；
     - "关闭排队"或"开启排队"，切换 prefs 的 `composer.followUpQueue`。
   - **编辑**：在原行展开成文本框，提交后经 `replace` 改写；版本冲突时提示，以宿主的队列为准。
   - 删除和重排的按钮位置预留，不做功能（CP-8）。
3. **队列暂停时发新消息的"发送消息？"弹框**：
   - M1 只有"发送"和"取消"。
   - T086 的汇报里写过"队列暂停时发送：先点取消……再点发送"。先核对这部分是否已经完成：
     - 已经完成就不重做，在汇报里写明；
     - 和设计 5.7.4 不一致的地方再补。
4. **用例**（已登记）：
   - **IT-queue-edit**（crate 内）：
     - 版本冲突；
     - 改写；
     - 插话成功；
     - 回退时留在原位，至少覆盖 `not_prompting` 和 `steer_pending`；
     - `remove`、`move` 回 `unsupported`。
   - **E2E-queue-edit**：
     - 编辑一条排队消息；
     - 用行上的"插话"把它插进运行中的回合；
     - 菜单切换排队偏好。
     - 插话前要等 agent 拿到这一轮的 prompt（交接事项里"写插话用例的任务"那一行）。
   - **组件测试**：行上的按钮、菜单、原行编辑、冲突提示。

**不做**：
- 删除、重排、"清空队列"（CP-8，P1）；
- 设置页的"跟进消息默认排队"开关（T147）。

## 验收标准

- [ ] IT-queue-edit 通过；E2E-queue-edit 通过，并在 runner 上连跑 10 次（由 `main` 触发 e2e-repeat）。
- [ ] 宿主的约束都守住：
  - 每条流一个写者；
  - 有状态的判定写在纯核心里；
  - 时间只经 `Clock`；
  - 协议只做附加（`queue_edit` 的形状在 T016 冻结前已定，这里只实现）。
- [ ] 渲染进程的安全约束都守住，文案走 i18n。
- [ ] 有截图变化时，基线只由 `ui-baseline` 生成。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 开工前先按模块估一遍：宿主 `queue_edit`、排队消息行、编辑、弹框核对、用例。估算超过停止线就先报告，不要动手。
- 停止线：550；硬停止线：650。
- 每完成一个模块就跑一次 `cargo xtask size`（守则第 8 条）。
- 不要靠压缩代码或注释去守线。

## 参考资料

- 设计：5.2.6（排队、Steer、中断）、5.3.4（`session/queue_edit` 的 `ops`、结果里的 `steer`）、5.3 末尾（UNIT-unsupported-set 的允许集合）、5.7.4（排队消息、暂停、"发送消息？"）。
- 卷宗：T040（`QueueCore`、`replace`）、T140（Steer 的纯核心判定、`QueueCore::take`）、T242（`useSessionSend`）、T086（输入框、排队消息、暂停时的发送确认）。
- 交接事项：`docs/tasks/handoff-notes.md` 里接收任务含 T149 的各行。

## 注意

- 本机不跑会抢焦点、发真实按键的用例（守则第 10 条）。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T043`，编排者已切到新分支 `feat/T149-queue-edit`，从 main 开出（含 T242 和本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
