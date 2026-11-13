# T149 汇报

- 执行者：子 agent（全栈）
- 分支：`feat/T149-queue-edit`
- 完成日期：2026-11-10

**CI 状态（交接时）**：
- PR #100 https://github.com/boris-w/cupper/pull/100 ，运行 https://github.com/boris-w/cupper/actions/runs/42773140790 ：8 个检查全部通过。
- e2e-repeat：
  - composer.spec 的全部 e2e 用例（`test=E2E-`）×20：https://github.com/boris-w/cupper/actions/runs/42773136092 ，`140 passed`（7 条 × 20）。
  - E2E-NFR1-2 ×10：https://github.com/boris-w/cupper/actions/runs/42764326114 ，通过。
  - 最早的 E2E-queue-edit ×10：https://github.com/boris-w/cupper/actions/runs/42760296728 ，`10 passed`。
- ui-baseline：https://github.com/boris-w/cupper/actions/runs/42760287789 。

## 做了什么

1. **宿主 `session/queue_edit`**：
   - turn 核心（`turn.rs`）：插话的来源分成 send 和队列项两种（`SteerFrom`）。
     - 队列项的插话复用 `unsteerable` 的判定，只是不比回合。`unsupported`、`not_prompting`、`steer_pending` 当场回复 `queued` 和原因，消息留在原位。
     - 插话在途时消息还在队列里；agent 接受（`Injected`）之后才取出（`QueueCore::take`），写成这一回合里 steered 的用户消息，带 `queue_id`。
     - `startedNewTurn` 取出后开 `steer_new_turn` 回合，回复带新回合 ID。
     - 超时、agent 出错、`prompt_required` 时留在原位；超时后晚到的 `Injected` 沿用 T140 的取出逻辑。
     - 回复延后到 agent 应答，send 的插话和队列项的插话一样。
   - actor：
     - 映射错误：`conflict/version_mismatch`、`not_found`；`remove`、`move` 和不认识的 op 回 `unsupported`；`steer` 和别的 op 一起出现回 invalid params。
     - archived 或只读的会话拒绝。
     - 路由已登记。
   - UNIT-unsupported-set 的允许集合里删掉了 `session/queue_edit`。设计 5.3 末尾写的是按类别的规则，没有逐个列方法，不用改。
2. **排队消息行**（`QueueRow.tsx`）：
   - "插话"按钮（tooltip"不打断模型，立即提交"），经 `queue_edit` 的 `steer` 发出，`expected_version` 取当前队列的版本。只在有运行中的回合、并且 agent 能插话时可用。
   - 副作用的提示和输入框共用"只提示一次"（抽出 `useHintOnce`）；没插进去时提示"没能插话，消息还在队列里"。
   - 菜单："编辑消息"，"开启排队"/"关闭排队"（切换 `composer.followUpQueue`）。
   - 原行编辑：Enter 只替换文字块，图片、文件引用、评论保留；Esc 取消。从菜单打开时，菜单关闭后焦点交给编辑框，为此 `Menu` 加了一个 `onCloseAutoFocus`。
   - 版本冲突提示"排队消息已经变了，显示的是最新的"，界面以宿主的队列为准。
   - 删除和重排（CP-8）没有做按钮。
   - 预览加了 `data-queue-preview`，供测试定位。
3. **"发送消息？"的弹框**：T086 已经做了，不重做（你已同意）。现在是输入框上方的警告框："发送"先发出消息、成功后让队列继续；"取消"或 Esc 收起、草稿保留。行为和设计 5.7.4 一致，文案"队列已暂停。发出这条消息，并让队列继续吗？"比设计里的"发送消息？"更清楚，保留 T086 的版本。
4. **用例**：
   - 核心单测 2 条。
   - IT-queue-edit 2 条：改写；插话成功；版本冲突；remove/move 回 unsupported；`steer_pending`、超时、`not_prompting` 时都留在原位。
   - 组件测试 3 条：按钮与回退提示、原行编辑与冲突、菜单切换。
   - E2E-queue-edit：编辑后用行上的"插话"插进运行中的回合；菜单打开排队后 Enter 会排队。插话前先等这一轮的输出出现。
   - 换了 composer-light、composer-dark 两张基线。

## 关键取舍

- **插进去之后才从队列取出**：这样回退时不用把消息放回去，晚到的 `Injected` 也能沿用 T140 的取出逻辑。插话在途时回合如果结束，`turn.completed` 按 T140 推迟，队列不会在这期间把它当成下一回合取走。
- **"插话"按钮在不能插话时禁用，而不是点了再报错**：没有运行中的回合（例如队列暂停）或者 agent 不支持插话时，点了也只会回退。回退只在竞争的情况下出现（`steer_pending`、超时等），统一提示一句，不逐个解释原因。

## 验证方式

- **本机**：
  - host 的 lib 测试 530 条、driver_k 27 条（含 IT-queue-edit）全部通过，clippy、fmt 通过；
  - ui-core 的 vitest 145 条全部通过，tsc、eslint、prettier 通过；
  - 截图整套 72 条无头跑过：70 条和现有基线一致，2 条就是换掉的那两张。
- **ui-baseline 产物逐字节核对**：68 个文件里只有 composer-light、composer-dark 两张不同，看过图后只提交了这两张。
- **第一轮 e2e 失败（E2E-NFR1-2）**：
  - 排队行多了"插话"按钮，行的文字变成了 "say queued插话"，断言写得太宽。
  - 改为对预览本身断言。顺带把 composer.spec 里五处对整行做部分匹配的断言也改成对预览精确匹配。产品没问题。
- **一次空跑的 e2e-repeat**：42764334381 用了默认的 test 参数 `E2E-SEC-real-input`，composer.spec 里一条都没匹配上，加了 `--pass-with-no-tests` 照样判 success。重跑时用 `test=E2E-`。你已立 T255，让没有用例跑过的 e2e-repeat 判失败。
- **第二轮 e2e 失败（42766751852，70 次里挂了 4 次），原因在测试，没有改产品**：
  - E2E-queue-edit 3/10：从菜单打开排队后马上输入。点菜单项返回时菜单还没卸载，它的焦点陷阱把输入和 Enter 吞了，"third" 根本没发出去。
  - E2E-send-key-setting 1/10：T244 时就有的时序问题，和我改的断言无关。偏好到了渲染进程，但 React 还没用新值重新渲染时，Enter 仍按旧的发送键处理。收到的值只剩一个换行，确切过程没能完全推出来，但根源是同一个。
  - 修法（7be28486），只等界面确认的状态，没有加 sleep：
    - 发送键改为启动前写进 prefs.json（新加 `startingPrefs`）；
    - 菜单点完等它消失；
    - 打开排队之后，等行菜单显示"关闭排队"再继续。E2E-composer-steer-default 中途切换偏好的地方同样处理。
  - 修好后 ×20 全部通过。
- **对照验收标准**：
  - [x] IT-queue-edit 通过；E2E-queue-edit 通过，并且连跑 10 次以上全部通过（composer 整套 ×20，140 passed）。
  - [x] 宿主的约束：每条流一个写者；有状态的判定在纯核心里；时间只经 Clock（IT 里的等待也走 SystemClock 的定时器）；协议没有改动。
  - [x] 渲染进程的安全约束：没有拼 HTML，文案走 i18n。
  - [x] 截图基线只由 ui-baseline 生成，只提交了变化的两张。
  - [x] PR #100 的 8 个检查在 7be28486 上全部通过。
- **规模**：+851 −128（`cargo xtask size`），停止线 840，超了 11 行，你已接受（超出部分全是修测试加的等待）。过程中报告过三次（notes.md 里有记录）：
  - 开工前估约 600，停止线放到 620；
  - turn 核心做完推算约 790，停止线放到 800、硬停止线 880（不拆，因为宿主部分单独进 main 会没有调用者）；
  - 宿主做完推算约 820，停止线放到 840。
  - 超出的原因：插话的回复分 send 和 queue_edit 两种、回退分"入队"和"留在原位"两种；rustfmt 把结构体字面量拆成了多行；测试需要几个辅助函数；组件从 QueueList 挪进 QueueRow 的约 25 行也算作新增。

## 提交列表

```
7be28486 test(e2e): wait for the window to have a preference before using it
79411cb4 test(e2e): queued rows checked by their preview
e3c1b279 test(ui-screenshots): UI-composer's queued rows with 插话 and their menu
b8ef50c2 docs(task): T149 notes: the estimates, the size reports, the choices
8f5375d0 test(e2e): E2E-queue-edit
f1e35e13 feat(ui-core): steer and edit a queued message from its row
0fe1850c test(host): IT-queue-edit
ac8cb7d4 feat(host): session/queue_edit
5b1d1a33 feat(host): the turn machine steers a queued message
```

## 遗留问题 / 范围外发现

1. **给写 e2e 的任务**：
   - 经 shell 设置偏好后，窗口要晚一点才拿到。依赖偏好的用例，要么启动前写进 prefs.json（`startingPrefs`），要么等界面上能看出新偏好的东西出现再继续（`setPref` 的说明里写了）。
   - 点完 Radix 菜单项后，先等菜单卸载再输入，否则焦点陷阱会把输入吞掉。
2. **E2E-reject-with-message**：设偏好之后要过好几秒才发跟进消息，而那时请求面板已经替换了输入框，没有可以确认的界面，所以没有改。理论上同样有这个时序问题，但余量很大。
3. **notes.md 没有记两轮 e2e 修正**：为了不再触发一轮 CI，没有往分支上补提交，这部分记在本汇报里。
