# T195 工作笔记

## 开工前的核对

- 任务书；交接：T093（持锁、`caffeinate -d`、`@focus` 两段跑法）、T189（`focusWindow` 先 `app.focus({ steal: true })`，等到 `isFocused()` 再等 1.1 s；`lsappinfo front` 采样）、T092（被遮挡窗口的三个降频参数）、T078（`launch({ throttled: true })`）。
- 主进程在 `ready-to-show` 时 `window.show()`；`second-instance`、`activate` 也走 `show()`（测试里不会触发）。

## 实测：哪种显示方式不抢前台

用一个最小的 Electron 脚本（Electron 44.9.0，任务目录外的临时物料，没有提交）显示一个窗口 4 s，同时每 0.2 s 用 `lsappinfo front` 采样前台应用，记下采样里前台是这个脚本的次数；当时前台是用户在用的编辑器：

| 做法 | 前台是测试窗口的采样 | 应用类型 |
|---|---|---|
| `show()` | 17 / 29 | — |
| `showInactive()` | 0 / 20 | `Foreground`（在 Dock 里） |
| `setActivationPolicy("accessory")` + `show()` | 15 / 18 | — |
| `setActivationPolicy("accessory")` + `showInactive()` | 0 / 22 | `UIElement`（不在 Dock） |
| `dock.hide()` + `showInactive()` | 0 / 17 | — |

结论：只有 `showInactive()` 才不抢前台；`accessory` 让 App 不进 Dock（`lsappinfo` 的类型是 `UIElement`）。两者一起用。

## 做法

- 主进程：e2e 构建里，环境变量 `CUPPER_E2E_BACKGROUND=1` 时设 `accessory` 并用 `showInactive()`。其他构建里这段代码随死分支一起去掉，`flavors.test.ts` 检查变量名不进非 e2e 构建。
- 夹具：没有 `@focus` 标签的用例在启动 App 时带上这个变量；运行期间每 250 ms 采样一次前台应用，用例结束时，只要有一次前台是本用例的 App，就让用例失败，并写明采样次数。启动后核对一次应用类型是 `UIElement`。
- 降频参数和 `throttled` 不受影响：它们是 Chromium 的启动参数，与激活无关。`showInactive()` 照样把窗口放到屏幕上（只是不激活），T078 的测量需要的"窗口可见"仍然成立。

## 规模估计（开工前）

| 模块 | 估计 |
|---|---|
| 主进程的后台开关（激活策略、`showInactive`） | 10 |
| `flavors.test.ts` 加一个名字 | 3 |
| 夹具：按标签打开、前台采样与用例结束时的检查、应用类型的核对 | 45 |
| 合计 | 约 **60** |

停止线 300，硬停止线 350。

## 按模块统计

| 模块 | 提交 | `cargo xtask size` 累计 |
|---|---|---|
| 主进程的后台开关 + `flavors.test.ts` | 83fe1c4 | +14 −5 |
| 夹具：按标签打开、前台采样、应用类型核对 | 3edaa76 | +67 −9（净 +58） |

## 验证进度

- 两个模块提交后，`pnpm build:e2e` 通过；`flavors.test.ts`、tsc、eslint、prettier 通过。
- 准备跑 `test:e2e:one` 时屏幕已锁（锁屏发生在开跑前，前台是 `loginwindow`）。锁屏时采样前台没有意义，`@focus` 用例也拿不到焦点，所以 e2e 验证、全集核对和带记录的 CI 都停在这里，等屏幕打开后再继续。
