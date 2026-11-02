# T217 汇报

- 执行者：子 agent（TypeScript / React 工程师）
- 分支：`fix/T217-panel-click-focus`（PR #35：https://github.com/boris-w/cupper/pull/35）
- 完成日期：2026-11-02

**交回时 CI 还没出结果**（这次是被要求立即交回）。三处都需要编排者去确认，所以汇报里写不了这两次连跑的结论：
- 推送 b53fea5 后，PR 的 8 个检查重新开始跑，全部还在 pending。上一轮（d25b675）的 8 个检查已经全绿。
- 新的 e2e-repeat 连跑（E2E-SEC-panel-arming × 50，workers 1，提交 b53fea5）：https://github.com/boris-w/cupper/actions/runs/41409995974 ，交回时还在运行。
- main 上的对照连跑（34df1c3，同样参数）：https://github.com/boris-w/cupper/actions/runs/41409631630 ，交回时也还在运行，不知道它有没有复现这个竞态。

## 做了什么

1. **点卡片能操作面板。** 点面板的文字、内容或空白处之后，如果焦点还在面板外，就把它移到选项列表上，Enter、数字键、Esc 随后都作用于面板。
   - 处理放在 click 的捕获阶段。这时按钮、输入框、选项行的默认取焦点已经发生，所以点它们的行为不变。
   - 就绪前按下的鼠标会被 `preventDefault`：不激活，也不移焦点（RP-2）。
2. **Tab 一下进面板。** 已实测：面板替换掉有焦点的输入框后，按一下 Tab 就进到面板的第一个可聚焦元素（"关闭"）。e2e 已改为一下 Tab 后断言 auto。
   - "按两下 Tab"的原因：Chromium 从最后一次点击的位置往后找。点在消息流上时，第一下 Tab 落在流里的按钮上。这是浏览器的默认行为，没有改，只写明原因。
3. **查清并修好自动接管失败。** 按裁定做了 P1、P2（不看 `hasFocus`）、P3：
   - P1：输入框卸载时，在 layout effect 的清理函数里直接读 DOM，判断 `document.activeElement` 是不是输入框；
   - P2：不看窗口在不在前台；
   - P3：面板卸载时记下焦点是否在它里面。下一个面板据此按 RP-3 判定；重新出现的输入框据此决定是否拿回焦点。
4. **单元测试。** 在 UNIT-request-panel 下新增 5 个，都在旧代码上逐个失败、新代码上通过：
   - 点标题后焦点到列表，数字、Esc、Enter 作用于面板；
   - 就绪前的点击不移焦点，之后的按键也无效；
   - P3：键盘答完后焦点回到输入框，下一个请求是 auto；
   - P2：窗口失焦但输入框仍持有页面焦点时，下一个请求是 auto；
   - (c)：答完后焦点移到别处，下一个请求是 manual，焦点留在原处。
5. **e2e。** E2E-SEC-panel-arming 改了三处：
   - 一下 Tab 激活面板；
   - 第二个请求改为点标题后按数字回答；
   - 修了连跑里暴露的竞态，见下一节。

### 第一次连跑那 1 次失败：原有的竞态，与产品改动无关

- **现象。** 运行 41405996706 是 99 过 1 败，失败的是第二个用例。快照里焦点停在侧边栏的"项目菜单"按钮上（[active]），它的 tooltip 也开着，按下的"1"落在了这个按钮上。
- **原因。** Radix 的 FocusScope 在菜单卸载后，用 `setTimeout(0)` 把焦点还给菜单按钮（DropdownMenu 的 onCloseAutoFocus 调用 `triggerRef.focus()`；已核对 react-focus-scope 1.3.1 源码）。用例在菜单数量变为 0 后立即调用 `focusOptions`。如果它比这个 timeout 先执行，焦点随后就被拿回按钮。
- **与本次改动无关。** 三处产品改动都不会把焦点移到侧边栏：preventDefault 只作用于面板内的按下；点击时移焦点只在面板内触发；P3 只在输入框挂载时生效。
- **修法（b53fea5，只改测试）。** 菜单关掉后先 `await expect(menu).toBeFocused()`，再 `focusOptions`。编排者已同意。

### 修之前自动接管失败的条件

- **(a) 请求到达时窗口在后台。** 页面失焦时 Chromium 给输入框派发 blur 和 focusout，但它仍是 `document.activeElement`，所以旧代码把状态记成 false。3b 的两次很可能就是这种情况。
- **(b) 激活窗口的那次点击不进页面**（`acceptFirstMouse` 为 false）。
- **(c) 反方向的缺陷。** React 19 在 commit 期间丢掉了 blur 事件，面板关掉后状态仍停在 true，之后的请求会从侧边栏抢走焦点，违反 RP-3。
- **(d) 点过输入框后又点了页面别处。** 这时是 manual，符合设计。

### 设计 5.7.8 RP-3 建议补的措辞

> "焦点在输入框里"读的是页面里的焦点：面板替换输入框的那一刻，输入框是否是页面的焦点元素（`document.activeElement`），不看窗口是否在前台。窗口在后台时，输入框仍持有页面焦点，窗口回到前台时焦点会回到它上面，所以照样算焦点在输入框里。理由：用户在窗口外时谈不上被抢焦点；回到窗口后的按键有窗口激活后 1 秒的冷却（第 7 条）、500 ms 待命（RP-1）和打字键撤销三道兜底；有草稿或 2 秒内打过字时照旧是 manual。面板关掉时如果焦点在面板里，焦点交还给输入框；连续的请求之间，焦点在前一个面板里也算焦点在输入框里。

## 关键取舍

- **点击后焦点落在选项列表，不给面板根元素加 `tabindex`。** jsdom 不模拟默认取焦点，那样单测测不到；任务书要的也是选项列表。代价：在面板上按下、拖到面板外再松开，不会移焦点。
- **就绪前的点击一律 `preventDefault`。** 待命期内不能在面板上选文字、也不能点进输入框，这与 RP-1 一致。
- **P3 只在焦点确实在面板里时才交还**，不扩大到"焦点停在 body"的情况。
- **不改浏览器自己的 Tab 顺序。**
- **竞态只在测试里修**，等待 Radix 把焦点交还。不去改 `focusOptions` 或菜单组件。

## 验证方式

- **实测。** 本机 Electron 44.9.0 隐藏窗口，用 CDP 模拟。结果写在 `docs/tasks/T217-panel-click-focus/notes.md`，脚本在仓库外，没有提交。
- **本机检查。** ui-core 全部 90 个单测通过；新增的 5 个在旧代码上逐个失败。ui-core 和 tools/e2e 的 tsc 通过，改动文件的 eslint、prettier 通过。`@focus` 用例本机没跑。
- **规模。** `cargo xtask size` 计 +121 −20，共 141 行，在提高后的停止线 150 以内。
- **CI。**
  - 上一轮（d25b675）：PR 的 8 个检查全绿；e2e-repeat 41405996706 是 99 过 1 败，即上面分析的竞态。
  - 本轮（b53fea5）：PR 检查、新连跑 41409995974、main 对照 41409631630 交回时都还没出结果，需编排者确认。
- **对照验收标准。**
  - 点空白处后按键有效：已完成，有单测和 e2e。
  - Tab 一下进面板：已完成，有 e2e，并写明了"两下"的原因。
  - 第 2 条：原因已查清并修复。
  - CI 全绿：待本轮结果。

## 提交列表

```
b53fea5 test(e2e): panel-arming waits for the closed menu to give its button the focus
d25b675 docs(task): T217 notes: what Chromium does with the focus, and why the panel lost it
17525a2 fix(ui): RP-3 reads the page's focus as the composer gives way
c374eab fix(ui): a click on the request panel's text gives it the keys
```

## 遗留问题 / 范围外发现

- 本轮 PR 检查、新连跑 41409995974、main 对照 41409631630 的结果需编排者确认。新连跑如果还有失败，我可以接着查，worktree 还保留着。
- 只能自由输入的提问（没有选项列表）和不支持的请求：自动接管或点击后面板是 active，但没有列表可以放焦点。这是原有限制，没有改。
- P3 交还焦点后，答完面板紧接着连按 Esc，第二下会落在输入框上。中断回合仍要按两下 Esc，但比以前少按一下。我判断风险可以接受，请编排者知悉。
- 其他用到 Radix 菜单之后马上移焦点的 e2e 也可能有同样的竞态。我只查了这个用例，没有逐个排查。
