# T242 notes

## 开工前的估算（2026-11-10）

| 模块 | 估算（新增行） |
|---|---|
| mode 的选择：`keys.ts` 的判定，send.ts 里共用的会话发送（运行中的回合、`expected_turn_id`、能力），Composer 改传按键 | 约 50 |
| 两种只提示一次的提示：UI store 按会话记在内存里，i18n | 约 35 |
| 回退的 toast | 约 5 |
| "还在等结果"的提示 | 约 20 |
| RP-12 改用共用的会话发送 | 约 10 |
| 组件测试（mode 的选择、两种提示、回退、等结果；request panel 的 RP-12） | 约 140 |
| e2e：E2E-composer-steer-default，加上 E2E-reject-with-message 的调整（见下） | 约 110 |
| 合计 | 约 370 |

在停止线 450 以内，直接开工。

E2E-reject-with-message 现在断言"跟进消息排队"。本任务以后，Claude 的回合在拒绝后还在跑，跟进消息默认插话，这条用例会失败。打算让它先打开"跟进消息默认排队"，保留原来对排队路径的验证；插话路径由 E2E-composer-steer-default 覆盖。

## 做法

- **mode 的选择**：
  - `keys.ts` 加了 `steers(action, running, followUpQueue)`：运行中按发送键要插话，打开"跟进消息默认排队"后改为排队键要插话；空闲时都不插话。
  - 真正发出时（`send.ts` 的 `useSessionSend`），"运行中"和 `expected_turn_id` 都取会话视图里最后一个还没完成的回合，不看索引里的状态。理由：插话要的是"客户端看到在跑的那个回合"；而且 RP-12 里 Codex 的拒绝会结束回合，索引的状态可能还没跟上，取视图更准。
  - 能力里 `turn.steer.supported` 为 true 才发 `steer`，否则排队。能力还没到（视图里没有 capabilities）时按不能插话处理。
- **两种只提示一次的提示**：记在 UI store 的 `hinted`（按会话、只在内存里，不进 prefs）。不能插话时第一次提示"已转为排队"；`side_effects` 为 true 时第一次插话提示副作用。
- **回退**：回复里带 `fallback_reason` 时 toast"已转为排队"，每次都提示（toast 按 id 替换，不会叠起来）。原因不在界面上逐个解释。
- **RP-12**：拒绝成功后的跟进消息改走同一个 `useSessionSend`，按"发送键"处理，`reply_to` 照旧；拒绝失败时文字回到输入框的逻辑不变。
- **发送按钮**：运行中、有内容时同发送键。

## "这条还在等结果"怎么表示

- 发出时照常立刻清空输入框（T086 起就是这样），发送不等回复，所以回复迟迟不来也不会卡住输入框。
- 插话发出后，按会话记一个"在等回复"的计数（UI store 的 `steering`），收到回复或失败时减掉。计数大于 0 并且持续超过 1 秒，输入框上方显示"插话还在等结果，可以继续输入"（`role="status"`，和"再按一次 Esc 停止"同一位置、同一样式）。
- 理由：
  - 宿主对插话有 5 秒超时，超时就转为排队并回复；真正一直不回的只有 T140 说的 `startedNewTurn` 之后原回合 prompt 挂起这一种，很少见。所以不需要阻塞或重试，只需要让用户知道这条消息去了哪里：插话成功会出现在消息流里，转为排队会出现在排队条里，两者之间的这段时间由这行提示说明。
  - 等 1 秒再显示，免得正常的插话（通常几百毫秒内回复）让提示闪一下、输入框上下跳。
  - 计数放在 UI store 而不是组件里，切换会话再回来时提示还在。

## E2E-reject-with-message 的调整

本任务以后，Claude 的回合在拒绝后还在跑，跟进消息默认插话。原用例断言"排队、等回合结束后作为新回合发出"，现在先打开"跟进消息默认排队"，保留对排队路径的验证；插话路径由 E2E-composer-steer-default 的第三条覆盖。拒绝的操作步骤抽成 `rejectWith()`，两条共用；设置偏好的 `setPref()` 放进 helpers，T244 的 E2E-send-key-setting 也改用它。

## 验证

- 本机：ui-core 的 vitest 全部 139 条通过；i18n 的测试、`tsc --build`（ui-core、i18n、e2e、ui-screenshots、desktop）、eslint、prettier 都通过；sidebar、request、new-session 三组截图对着现有基线 25 条全部通过，没有截图变化。ShellAPI 没有改。
- e2e 只在 runner 上跑（守则第 10 条）。

## 实际规模

`cargo xtask size`：+366 −80，在停止线 450 以内。
