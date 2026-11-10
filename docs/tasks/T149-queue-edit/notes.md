# T149 notes

## 开工前的估算（2026-11-10）

| 模块 | 估算（新增行） |
|---|---|
| 宿主：路由、actor（解析、op 转换、错误映射、延后回复） | 约 45 |
| 宿主：turn 核心（插话来源分 send 和队列项；插进去之后才取出） | 约 90 |
| 宿主：核心单测 | 约 40 |
| IT-queue-edit | 约 110 |
| 排队消息行：插话按钮、菜单、原行编辑、冲突提示；`useSessionSend` 的提示和回退抽出共用；i18n | 约 170 |
| 组件测试 | 约 110 |
| E2E-queue-edit | 约 60 |
| 合计 | 约 600 |

超过 550，先报告。编排者把停止线放到 620，硬停止线 700，保留核心单测。

"发送消息？"的弹框：T086 已经做了（输入框上方的警告框，"发送"先发出消息、成功后让队列继续，"取消"或 Esc 收起、草稿保留），编排者决定保留 T086 的做法和文案，不重做。

## 规模第二次报告（做完 turn 核心时）

turn 核心和单测做完是 +278 −58，推算到约 790，超过硬停止线，停下来报告。编排者选了不拆（宿主部分单独进 main 会没有调用者），停止线放到 800，硬停止线 880。超出的原因：

- 回复分两种：插话的 send 和 queue_edit 各自要回复（`reply_steered`、`edited`）；
- 回退分两种：send 的消息入队，队列项留在原位（`not_steered`、`stayed`）；
- rustfmt 把几处 `Message`、`Sent` 的结构体字面量拆成了多行；
- 单测要从动作里找出队列项和 queue_edit 的回复，多了几个辅助函数。

## 规模第三次报告（宿主做完时）

宿主做完是 +481。actor 这一层 +91（估的是 45，同样是 rustfmt 换行，再加上错误映射和 op 转换），IT-queue-edit +112。界面照实估还要约 340，推算约 820，又报告了一次，编排者把停止线放到 840，硬停止线仍是 880。

## 做法

- **宿主**：
  - 队列项的插话判定复用 `unsteerable`，只是不比回合（`named` 恒为真）：`unsupported`、`not_prompting`、`steer_pending` 当场回复，消息留在原位。
  - 插话在途时消息还在队列里。回合在插话在途时结束，`turn.completed` 照 T140 推迟，所以队列不会在这期间把它当成下一回合取走。
  - agent 接受（`Injected`）之后才把它从队列取出（`QueueCore::take`），写成这一回合里 steered 的用户消息，带 `queue_id`；`startedNewTurn` 取出后开 `steer_new_turn` 回合，回复带新回合 ID。
  - 超时、agent 出错、`prompt_required`、`unsupported`：留在原位，回复 `queued` 和原因。超时后晚到的 `Injected` 沿用 T140 的 `Late` 取出逻辑。
  - actor：`steer` 和别的 op 一起出现回 invalid params；不认识的 op 回 `unsupported`；archived 或只读的会话拒绝（加进 `WRITES`）。
  - UNIT-unsupported-set 的允许集合：删掉 `session/queue_edit` 那一行。设计 5.3 末尾写的是按类别的规则（M1b、M2、M3 的方法等），没有逐个列方法，不用改。
- **界面**：
  - "插话"按钮只在有运行中的回合、并且 agent 能插话时可用。按下后经 `queue_edit` 的 `steer` 发出，`expected_version` 取当前队列的版本。没插进去（`fallback_reason`）时提示"没能插话，消息还在队列里"，不逐个解释原因。副作用的提示和输入框共用"只提示一次"（抽出 `useHintOnce`）。
  - 原行编辑：文本框里是这条消息的文字；Enter 提交，只替换文字块，图片、文件引用、评论照旧；Esc 取消。从菜单打开时，菜单关闭后把焦点交给文本框（`Menu` 加了 `onCloseAutoFocus`）。
  - 版本冲突（`conflict`）提示"排队消息已经变了，显示的是最新的"，界面跟着宿主的 `queue.updated` 显示，不重试。
  - 菜单的"开启排队"/"关闭排队"切换 `composer.followUpQueue`。
  - 删除和重排（CP-8）没有做按钮。

## 验证

- 本机：host 的 lib 测试 530 条、driver_k 27 条（含 IT-queue-edit 两条）全部通过，clippy、fmt 通过；ui-core 的 vitest 145 条全部通过，tsc、eslint、prettier 通过。
- e2e 只在 runner 上跑（守则第 10 条）。

## 实际规模

`cargo xtask size`：+804 −118，在停止线 840 以内。
