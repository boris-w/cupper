# T242 插话的界面

- 派发日期：2026-11-10
- 执行者：子 agent（前端工程师，ui-core）
- 分支：`feat/T242-steer-ui`
- 依赖：T140、T244（都已合并）。T149、T171 依赖本任务

## 范围

**做**（计划 T242 一行；设计 5.7.4 的"跟进消息"、5.7.8 的 RP-12、5.3.4 的 `session/send`、5.3.7 的 `turn.steer`；PRD CP-6 v1.3）：

1. **运行中的默认插话**：
   - 回合运行中按发送键，经 `session/send` 发 `mode: steer`，`expected_turn_id` 取正在运行的回合；按排队键发 `mode: queue`。
   - 发送键和排队键按 T244 的 `sendAction()`：发送键是 Enter，或者偏好为 ⌘Enter 时的 ⌘Enter；排队键是 ⌘Enter，或者 ⌘⇧Enter。
   - 没有运行中的回合时照旧，`mode` 不变。
   - 发送按钮在"运行中、有内容"时的行为同发送键（设计 5.7.4 发送按钮的第 4 种状态）。
2. **偏好"跟进消息默认排队"**：
   - 存在 prefs 里，键名 `composer.followUpQueue`，默认关。
   - 打开后插话和排队对调：发送键排队，排队键插话。
   - 设置页的界面在 T147，本任务不做。
3. **能力**：
   - 能力里 `turn.steer.supported` 为 false 时，运行中按任何一个键都排队。这个会话里第一次这样做时提示一次"已转为排队"。
   - `turn.steer.side_effects` 为 true（Claude）时，这个会话里第一次插话时提示一次副作用：当前的工具调用会被丢弃，正在跑的命令会被移到后台。文案按设计 5.7.4，走 i18n。
   - "第一次"按会话记在内存里，不进 prefs。
4. **回退**：宿主回复里带 `fallback_reason` 时，消息已经进了队列，toast 提示"已转为排队"。原因只决定要不要提示，不在界面上逐个解释。
5. **迟迟没有回复**（交接事项里 T140 那一行）：
   - 原回合的 prompt 挂起时，`steer` 的回复可能一直不来。
   - 输入框不能因此卡住：发出后照常清空、可以继续输入。
   - 你定怎样表示"这条还在等结果"，并在汇报里说明理由。
6. **"拒绝并附一条消息"**（RP-12）：
   - 拒绝成功后发的那条跟进消息，现在一律排队。
   - 改为遵守偏好和插话能力，规则同第 1 至 4 条，`reply_to` 照旧。
   - 拒绝失败（`already_resolved`、`expired`）时文字留在输入框里，这部分不变。
7. **用例**：
   - **E2E-composer-steer-default**（已登记）覆盖：
     - 默认插话；
     - 排队键排队；
     - 偏好打开后对调；
     - 回退（假 agent 的 `--fault no_steering`，或者让宿主回 `fallback_reason`）；
     - "拒绝并附一条消息"走插话路径。
   - **组件测试**：
     - `mode` 的选择（运行与否、两种发送键偏好、跟进偏好、能力）；
     - 两种只提示一次的提示；
     - 回退的 toast。

**不做**：
- 排队消息行上的"插话"按钮、编辑消息、`session/queue_edit`（T149）；
- 设置页"通用"里的偏好开关和副作用的说明（T147）；
- 宿主的 Steer 逻辑（T140 已完成，不改）。

## 验收标准

- [ ] E2E-composer-steer-default 通过，并在 runner 上连跑 10 次（由 `main` 触发 e2e-repeat）。
- [ ] 组件测试覆盖上面列的各项；ui-core 的 vitest、tsc、eslint、prettier 都通过。
- [ ] 渲染进程的安全约束都守住，文案走 i18n。
- [ ] ShellAPI 如果要改，只做附加，`frozen.ts` 的检查通过。
- [ ] 有截图变化时，基线只由 `ui-baseline` 生成。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 开工前先按模块估一遍：mode 的选择、提示、回退、RP-12、用例。估算超过停止线就先报告，不要动手。
- 停止线：450；硬停止线：540。
- 不要靠压缩代码或注释去守线。

## 参考资料

- 设计：5.7.4（跟进消息、发送按钮的三种状态、toast）、5.7.8（RP-12）、5.3.4（`session/send`、`fallback_reason` 的取值）、5.3.7（`turn.steer`）、5.2.6（排队、Steer、中断）。
- 卷宗：T140（宿主侧的 Steer、`steer_pending`、假 agent 的 `no_steering` 和插话脚本）、T244（`sendAction()`）、T083 与 T093（"拒绝并附一条消息"）。
- 交接事项：`docs/tasks/handoff-notes.md` 里接收任务含 T242 的各行，以及 e2e 的提示：`press` 不发 `char`；点按钮用 `click`。

## 注意

- 本机不跑会抢焦点、发真实按键的用例（守则第 10 条）。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T043`，编排者已切到新分支 `feat/T242-steer-ui`，起点是 main（含 T244 和本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
