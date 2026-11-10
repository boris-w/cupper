# T242 汇报

- 执行者：子 agent（前端工程师，ui-core）
- 分支：`feat/T242-steer-ui`
- 完成日期：2026-11-10

**CI 状态（交接时）**：
- PR #97 https://github.com/boris-w/cupper/pull/97 ，运行 https://github.com/boris-w/cupper/actions/runs/42753575205 ：8 个检查全部通过（e2e、rust、ui、compat-framework、protocol、license、ts、testdata-scan）。
- e2e-repeat：
  - E2E-composer-steer-default ×10（修好之后）：https://github.com/boris-w/cupper/actions/runs/42753569005 。共三条用例：composer.spec 两条共 `20 passed`，interaction.spec 的 @focus 一条 `10 passed`。
  - E2E-reject-with-message ×5：https://github.com/boris-w/cupper/actions/runs/42750925711 ，两条用例共 `10 passed`。
  - 第一次连跑失败：https://github.com/boris-w/cupper/actions/runs/42750915471 ，原因见"验证方式"。

## 做了什么

1. **运行中的默认插话**：
   - 判定：`keys.ts` 新增 `steers(action, running, followUpQueue)`。运行中按发送键要插话；"跟进消息默认排队"打开时，改为排队键要插话。空闲时都不插话。
   - 发送：`send.ts` 新增共用的 `useSessionSend`。要插话、并且能力里 `turn.steer.supported` 为 true 时，发 `mode: steer`，`expected_turn_id` 取会话视图里最后一个还没完成的回合；否则发 `mode: queue`。
   - 发送按钮在"运行中、有内容"时同发送键。
2. **偏好**：`composer.followUpQueue`（`FOLLOW_UP_QUEUE_PREF`），默认关，经 prefs 读取。设置页的界面留给 T147。
3. **能力和只提示一次的提示**：
   - 不能插话时排队，这个会话里第一次提示"已转为排队"。
   - `side_effects` 为 true 时，这个会话里第一次插话提示"插话时，当前的工具调用会被丢弃，正在运行的命令会被移到后台"。
   - "第一次"按会话记在 UI store 的 `hinted` 里，只在内存中，不进 prefs。
4. **回退**：回复里带 `fallback_reason` 时，每次都 toast"已转为排队"。原因不在界面上逐个解释。
5. **迟迟没有回复**：
   - 发出时照常立刻清空输入框，发送不等回复，所以回复不来也不会卡住输入框。
   - 插话发出后，按会话记一个"在等回复"的计数（UI store 的 `steering`）。这个计数持续超过 1 秒时，输入框上方显示"插话还在等结果，可以继续输入"。
   - 理由：宿主有 5 秒超时，超时会转为排队并回复，真正一直不回的只有 T140 说的那种挂起。插话成功会出现在消息流里，转为排队会出现在排队条里，这行提示说明的是两者之间这段时间。等 1 秒再显示，免得正常的插话让提示闪一下、输入框上下跳。计数放在 store 里，切换会话再回来时提示还在。
6. **RP-12**：拒绝成功后的跟进消息改走 `useSessionSend`，按发送键处理，`reply_to` 照旧。拒绝失败时文字回到输入框，这部分不变。
7. **用例**：
   - 组件测试：
     - `steers` 的真值表；
     - 运行中发送键和按钮插话（带回合 ID），排队键排队，偏好对调；
     - 不能插话时排队，并且只提示一次；副作用只在第一次插话时提示；
     - 回退每次都提示；
     - 回复不来时输入框照常可用，1 秒后出现等待提示；
     - request panel 里，回合还在跑时 RP-12 的跟进消息走插话。
   - capabilities 测试数据合成一份共用的 `composer/test-capabilities.ts`。
   - e2e：E2E-composer-steer-default 三条：
     - Enter 插话、⌘Enter 排队、偏好对调；
     - `steer_error` 的回退和 toast；
     - 拒绝并附消息走插话（@focus）。

## 关键取舍

- **"运行中"和 `expected_turn_id` 取会话视图，不看索引状态**：插话要的是"客户端看到在跑的那个回合"。而且 RP-12 里 Codex 的拒绝会结束回合，索引的状态可能还没跟上，取视图更准。能力还没到时按不能插话处理。
- **E2E-reject-with-message 打开"跟进消息默认排队"**：Claude 的回合在拒绝后还在跑，跟进消息现在默认插话。原用例断言排队，改成先打开这个偏好，保留对排队路径的验证；插话路径由 E2E-composer-steer-default 覆盖（你已同意）。拒绝的操作步骤抽成 `rejectWith()`；设置偏好的 `setPref()` 放进 helpers，T244 的 E2E-send-key-setting 也改用它。

## 验证方式

- **本机**：
  - ui-core 的 vitest 全部 139 条通过；i18n 的测试、`tsc --build`（ui-core、i18n、e2e、ui-screenshots、desktop）、eslint、prettier 都通过。
  - sidebar、request、new-session 三组截图对着现有基线 25 条全部通过，没有截图变化。ShellAPI 没有改。
  - e2e 只在 runner 上跑（守则第 10 条）。
- **第一次 e2e-repeat 失败，原因在测试**：
  - "Enter 插话、⌘Enter 排队、偏好对调"那条 10 次全挂：页面上 "steer one" 进了排队条，没有插进回合。
  - 测试在 `turn.started` 一出现就插话。宿主的回合要等开始快照拍完、prompt 交给 agent 之后才进入 `Prompting`，在此之前是 `Preparing`，这期间来的插话按 `not_prompting` 转为排队。
  - 同时发现，"回退"那条原来能过是因为同样的原因：插话在 `Preparing` 期间被 `not_prompting` 转成了排队，根本没有到达 agent，所以并没有测到想测的 `steer_error`。
  - 修法（14c029cb）：第二个回合先说 "turn two"，等它出现在页面上再插话。两条 composer 用例共用这一步，回退那条现在真正走到 agent 的错误。修好后连跑 10 次全部通过。
- **对照验收标准**：
  - [x] E2E-composer-steer-default 通过，在 runner 上连跑 10 次全部通过。
  - [x] 组件测试覆盖了 mode 的选择（运行与否、两种发送键偏好、跟进偏好、能力）、两种只提示一次的提示、回退的 toast；ui-core 的 vitest、tsc、eslint、prettier 都通过。
  - [x] 安全约束：没有拼 HTML，文案走 i18n（`queued_instead`、`steer_side_effects`、`steer_waiting`）。
  - [x] ShellAPI 没有改。
  - [x] 没有截图变化。
  - [x] PR #97 的 8 个检查在 14c029cb 上全部通过。
- **规模**：开工前估约 370，实际 +370 −80（`cargo xtask size`），在停止线 450 以内。

## 提交列表

```
14c029cb test(e2e): steer once the agent has the turn's prompt
9a595ec8 docs(task): T242 notes: the estimate, the choices, the late answer
17b9b44d test(e2e): E2E-composer-steer-default
45d36116 feat(ui-core): RP-12's follow-up steers as the send key would
093f9b99 feat(ui-core): the composer steers the running turn
```

## 遗留问题 / 范围外发现

1. **产品说明：回合还在准备时插话会转为排队**。回合刚开始、宿主还在拍开始快照（`Preparing`）时，插话会按 `not_prompting` 排队，界面提示"已转为排队"，排队条上出现这条消息。这是宿主的设计，界面会如实提示。
2. **notes.md 没有记 e2e 的那次修正**：为了不再触发一轮 CI，我没有往分支上补提交，这部分记在本汇报里。
3. **给 T149**：排队消息行上的"插话"按钮可以复用 `useSessionSend` 里的提示和回退逻辑。
4. **给 T147**：设置页的"跟进消息默认排队"开关写 `composer.followUpQueue`（布尔）；副作用说明的文案可以复用 `composer:steer_side_effects`。
