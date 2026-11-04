# T231 汇报

- 执行者：子 agent（前端工程师，ui-core 与桌面）
- 分支：`feat/T231-config-selectors`（PR https://github.com/boris-w/cupper/pull/60 ，头提交 2ec6100）
- 完成日期：2026-11-04

输入框底栏的权限按钮和"{模型} {强度} ⌄"选择器都做完了，完全访问经原生确认框开启。PR #60 在 2ec6100 上 8 项检查全部通过；E2E-full-access-native 在 runner 上连跑 10 次全过。

## 做了什么

1. **读能力和发命令**（`composer/config.tsx`）
   - `useOption` 读会话视图里能力给出的选项，当前值取 `session.configured.config`，没有时取能力里的 `current`。只列 agent 能力里有的值，显示 agent 给的名字。
   - `useSetOption`：
     - 完全访问走 `requestPrivileged({kind: "set_config"})`，由主进程经 `policy/describe` 和原生确认框发出；
     - 其余取值，包括 elevated，由窗口直接发 `session/set_config`，失败时 toast"没有改成（{code}）"。
   - `risk` 既不是 `normal` 也不是 `elevated` 的取值都按完全访问处理，与宿主的规则一致。
   - 另有三个小组件：
     - `WithSession`：会话流到了才渲染；
     - `Choices`：带标题的一组取值，当前值打勾；
     - `BarButton`：底栏按钮，高 28、文字 13、圆角 `--radius-full`。
2. **权限按钮**（`extensions/permission.tsx`，底栏左侧）
   - 盾牌图标加当前模式的短名，下拉标题"agent 的操作要怎样批准？"。
   - elevated 的取值附一行说明"由 agent 判断哪些操作需要你批准"；完全访问的取值附"不再请求批准，要在系统确认框里确认"。
3. **"{模型} {强度} ⌄"**（`extensions/model.tsx`，底栏右侧）：一个按钮，菜单分"选择模型""选择推理强度"两组；改成受控打开，打开状态跟随 UI store 的 `menu`。
4. **档位提示**（输入框上方）：完全访问时显示警告框"完全访问权限已开启"；elevated 时显示一行状态文字"低打扰模式已开启：agent 会自行批准它判断为安全的操作"。
5. **`change_model` 入口**：错误框里加"换模型"按钮，点了打开输入框的模型菜单。它是单独的组件，其他错误行因此不需要 `UiProvider`。
6. **共用组件的附加改动**：
   - `Menu` 加了受控的 `open` / `onOpenChange` 和 `MenuLabel`；
   - 图标加了 `ShieldIcon`、`CheckIcon`；
   - UI store 加了 `menu` / `openMenu`。
7. **文案**：新增 11 条，都走 i18n，字符都在截图字体子集里。

## 关键取舍

- **只在已有会话里显示**：拿不到会话流（新建会话页）或能力里没有对应选项时不显示。新建会话页交给 T144。mock 宿主不发能力，所以除 UI-error 外，现有截图基线都不变。
- **不用 `ToolbarButton`**：它带的 Tooltip 需要外层有 `TooltipProvider`，而底栏按钮显示的就是短名，不需要提示框，所以另写了 `BarButton`。
- **共用的 `MenuItem` 保持原样**：第一次推送时我给它加了 `gap-2`，导致 UI-new-session 的四张图里每个菜单的第二段文字都移了位。按你的意见恢复原样，间距改成 `Choices` 里勾号外层的 `pl-2`（c86a4b4）。
- **E2E 先取消再确认**：主进程在取消后，同一种特权操作要等 2 秒（设计 5.7.8 的冷却），用例里等 2.1 秒再试第二次。最后核对审计里只有一条 `session/set_config` 的 `privileged.executed`（`action: full_access`），以此证明取消的那次没有发到宿主。

## 验证方式

- **组件测试**：UNIT-composer-config 三个用例。
  - 权限菜单：显示当前短名和标题、elevated 的说明；选 elevated 发出 `session/set_config`；选完全访问只走 `requestPrivileged`，不直接发命令。
  - 模型按钮：文字为"Opus High"，菜单分两组；选中后分别发出 `model`、`reasoning_effort` 的修改；`openMenu("model")` 能打开菜单；宿主拒绝时 toast"没有改成（agent_error）"。
  - 档位提示：完全访问显示警告框，elevated 显示状态文字；没有能力时不显示按钮。
  - UI-error 加了一条"换模型"按钮的断言。
  - 本机 ui-core 103 个测试全部通过；tsc、eslint、prettier、字体子集检查、trace-ids 都通过。
- **E2E-full-access-native**（nightly，`tools/e2e/src/config.spec.ts`）：
  - 本机按文件只跑这一个，1/1 通过；
  - runner 上的 e2e-repeat 连跑 10 次全过（10 passed）：https://github.com/boris-w/cupper/actions/runs/41759447638 。
- **PR CI**：https://github.com/boris-w/cupper/actions/runs/41762508259 ，在 2ec6100 上 8 项检查全部通过。第一次推送（f370b1e）时 `ui` 失败过：
  - UI-new-session 的四张图，原因是 `MenuItem` 的 `gap-2`，已修复，与原基线对上了；
  - UI-error 的两张图，是预期中多出的"换模型"按钮，见下一条。
- **截图基线**：只用 ui-baseline 运行 41760956018 的产物，只取了 `stream-error-light.png`、`stream-error-dark.png` 两张，看过图后单独提交（2ec6100）；其他基线和 meta.json 都没动。
- **审阅截图**（不进基线）：`docs/tasks/T231-config-selectors/review-permission-menu.png`、`review-model-menu.png`，用 e2e 构建、本机系统字体和假 agent 的 Claude 画像截取，截图用的是一个没有提交的临时 spec。
- **规模**：`cargo xtask size` 为 +464 −7（停止线 550），开工前估算约 435。

## 提交列表

```
2ec6100 test(ui): UI-error baselines with the change_model button
c86a4b4 fix(ui): keep the menu items' spacing; the choices space their tick
f370b1e test(e2e): full access through the native dialog
aa8b5b3 feat(ui): permission button and the model and effort selector
```

## 遗留问题 / 范围外发现

1. **菜单写法统一**：T130（PR #62）合并后会统一菜单写法，用它的 `checked` / `radio` 改写本任务的 `Choices`。你说过这一项不用我做。
2. **模型按钮的文字偏长**：假 agent 的 Claude 画像下按钮写作"Default (recommended) High"，模型名本身就长。要不要截断或改用短名，可以在 T148/T154 的复刻度审阅里一起看。
3. **"换模型"的前提**：它打开的是当前会话输入框里的模型菜单。如果请求面板正好替换了输入框，或者会话没有模型选项，点了不会有反应。
4. **新建会话页**的权限和模型选择交给 T144（它负责第一条消息前显示缓存的能力）。

notes.md（估算、实现记录、取舍）和两张审阅截图已在分支上提交，位置 `docs/tasks/T231-config-selectors/`。
