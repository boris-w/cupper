# T195 修复 1 汇报

E2E-single-instance 已经标上 `@focus`，验收标准三条都满足：PR #6 的 8 个检查全过；分支上的 nightly 跑了 e2e 全集，48 条全过，后台段没有前台命中。同类排查只找到这一条需要改。

还有一个只改了 notes 的提交 77e072a 在本机，没有推送。它把 runner 上的结果补进了 notes，像 T197 的 84de78f 那样随合并一起进 main 就行。

- 执行者：子 agent（工程师；T093、T189、T197 的执行者）
- 分支：`fix/T195-single-instance-focus`（PR #6：https://github.com/boris-w/cupper/pull/6 ）
- 完成日期：2026-10-27

## 做了什么

- **改用例标签**：`tools/e2e/src/app.spec.ts` 的 E2E-single-instance 加了 `{ tag: "@focus" }`，并加一行注释说明原因。
  - 第二次启动时，主进程在 `second-instance` 里调 `showMainWindow()`，对已有窗口执行 `show()`，这一步会激活 App。
  - nightly 40412822285 里的 4 次前台命中，都出在这一步。
  - 这条用例测的正是这个行为，所以要放进 `@focus` 段。
- **同类排查**：对照了后台段的全部用例，结论写进了 `docs/tasks/T195-e2e-no-focus/notes.md` 的"修复 1"一节。结论是只有这一条要加 `@focus`。
- **没有改的**：主进程的激活逻辑和前台检查都没动。

## 关键取舍

- **只改标签，不改代码**：这条用例本来就要激活 App，标 `@focus` 符合它的本意。
- **接受缩进带来的改动量**：prettier 把用例体缩进了一层，所以改动显示为 +30 −27，实际只多了 3 行。这是仓库里长标题用例的通行写法，在停止线 60 之内。

## 同类排查的结论

主进程里会激活 App 的路径，对照后台段的用例：

- **`second-instance` → `show()` / `restore()`**：只有 E2E-single-instance 会触发，已改为 `@focus`。
- **`activate`（macOS 的 reopen）**：走不到。后台模式是 `accessory`，没有 Dock 图标；用例也不经 LaunchServices 启动 App。
- **新建主窗口时的 `show()`**：后台模式下已经改为 `showInactive()`（T195），后台段也没有用例关掉窗口后让它重建。
- **原生对话框**：e2e 构建里由替身代答（`e2e-dialogs.ts`），不弹真对话框。
- **`shell.openExternal`**：目前没有用例触发。

用例代码里会激活 App 的写法：

- **`focusWindow` / `activatePanel` / `press` / `pressDigit` / `type`**：全部只出现在 `@focus` 用例里。
- **新建 `BrowserWindow`**：只有 `gate.spec.ts` 的 `cover()`，T195 已经改过。
- **再启动一个实例**：只有 E2E-single-instance。
- **后台段里的 Playwright 输入**（`locator.press`、`click`）：走 CDP，不激活 App。

## 验证方式

对照 brief-fix1 的验收标准：

1. **E2E-single-instance 在 `@focus` 段里通过，后台段不再有前台命中**：满足。
   - 编排者在本分支上触发了 nightly：https://github.com/boris-w/cupper/actions/runs/40438064169 。
   - e2e 全集 48 条全过：
     - 后台段 25 条，没有前台命中；
     - `@focus` 段 19 条，其中 E2E-single-instance 用时 3.4 s。
   - 我从日志构件核对过它落在 `@focus` 段。
2. **同类排查的结论写进 notes.md**：满足，见上一节和 notes 的"修复 1"一节。
3. **PR 上 GitHub CI 的 8 个检查都通过**：满足。
   - 运行：https://github.com/boris-w/cupper/actions/runs/40438066146 。
   - 第 1 次就全过：protocol、rust、ui、e2e、compat-framework、ts、license、testdata-scan。

本机核对：
- `playwright test --list --grep @focus` 列出 19 条，含 E2E-single-instance；
- eslint、tsc、prettier 都通过。

规模：`cargo xtask size` 显示 +30 −27（净 +3），停止线 60。

## 提交列表

```
77e072a docs(task): T195 fix 1 notes, the runs on the runner
1c778f1 docs(task): T195 fix 1 notes, the paths that activate the app
99d7442 test(e2e): E2E-single-instance is a `@focus` test
6a9f0e4 docs(task): estimate T195 fix 1
```

## 遗留问题 / 范围外发现

1. **`shell.openExternal` 激活的是浏览器，前台检查抓不到**：前台检查只认本 App 的 pid。
   - 浏览器抢走的同样是用户的前台。
   - 现在没有用例触发它：`tools/e2e` 里没有点外部链接、`window.open` 或调 `openExternal` 的地方。
   - 以后加这类用例时，要么标 `@focus`，要么在 e2e 构建里给 `openExternal` 加替身，像对话框那样代答。
2. **PERF-bridge-gate 的 3 条在 runner 上不跑**（`@perf`），只在作者机器的后台段跑。其中 covered 那条在 T195 修过，e2e-repeat 40319605626 核对过；另外两条的前台情况，以作者机器上的运行为准。
