# T195 不需要焦点的 e2e 用例不抢焦点

- 派发日期：2026-10-26
- 执行者：子 agent（前端工程师）。T093、T189 的执行者接着做（机器锁、`@focus` 两阶段、焦点探针都是它做的）
- 分支：`test/T195-e2e-no-focus`
- 依赖：T093（已合并）、T189（改了 `focusWindow`，要先合并）

## 背景

现在每个 e2e 用例都会启动一个 App：窗口在 `ready-to-show` 时 `show()`，App 被激活、抢走前台，作者在跑 e2e 的时候没法同时用这台电脑。只有标了 `@focus` 的用例（22 处）真正需要窗口的焦点；其余用例的按键、点击都是经 `webContents.sendInputEvent` 发的，不需要前台。用户决定：不需要焦点的用例不要再抢焦点（计划 v1.3.63）。

## 范围

**做**：
1. **非 `@focus` 用例的 App 不激活**：
   - 窗口照常创建、照常渲染，但不激活 App、不抢前台、不出现在 Dock。可选的做法有：`showInactive()`、`app.setActivationPolicy("accessory")`、`app.dock.hide()`，或者它们的组合。先实测哪种在 macOS 上真的不抢前台，结论写进 `notes.md`；
   - 只在 e2e 构建里生效，由启动参数或环境变量打开（例如 fixture 的 `launch({ focus: false })`）；发布构建与自用构建的行为不变，`flavors.test.ts` 一类的检查要覆盖到；
   - `@focus` 用例照旧显示并激活，`focusWindow` 照 T189 的写法抢焦点。
2. **证明不抢前台**：e2e 运行时采样前台应用（例如 `lsappinfo front`，不需要额外权限），非 `@focus` 阶段前台一次也不是测试 App。做成一个用例或运行时的检查，失败时报出是哪个用例抢了前台。
3. **不破坏已有的测量**：
   - 被遮挡窗口的降频参数（fixture 里的三个 `--disable-*`）照旧生效，面板的武装、视图的刷新不受影响；
   - `launch({ throttled: true })` 的测量（T078 的 PERF-bridge-gate）照旧可用。如果测量需要窗口可见，就让它显示但不激活，并说明理由。
4. **全集核对**：在屏幕开着的情况下跑一遍 `pnpm test:e2e`，全部通过；非 `@focus` 阶段，作者在用别的应用，输入不会落进测试窗口。

**不做**：在虚拟机里跑 e2e（用户没有选）；改 `@focus` 用例本身的写法。

## 验收标准

- [ ] 非 `@focus` 的 e2e 用例启动的 App 不激活、不抢前台、不出现在 Dock，有前台采样的证据。
- [ ] 发布构建与自用构建不受影响，有检查。
- [ ] `pnpm test:e2e` 全集通过；`ci pr` 的 e2e 冒烟通过。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T195-e2e-no-focus/ci/` 通过，摘要随任务提交。

## 交接事项

- T093：机器锁、`caffeinate -d`、锁屏时报错；`@focus` 两阶段的跑法。
- T189：`focusWindow` 改为先 `app.focus({ steal: true })`、等到 `isFocused()` 再等 1.1 s；焦点事件探针与前台采样的做法（`notes.md`）。
- T092：E2E-SEC-real-input 的写法与被遮挡窗口的降频参数。
- T078：`throttled` 的测量。

## 注意

- 主进程在调用时才取 `dialog.showMessageBox` 这类函数，不在模块加载时解构保存（代码约束）。
- e2e 一律经 `pnpm test:e2e`、`test:e2e:one` 持锁运行。

## 规模

- 停止线：300；硬停止线：350

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计；每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

T189 合并之后，在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b test/T195-e2e-no-focus main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
