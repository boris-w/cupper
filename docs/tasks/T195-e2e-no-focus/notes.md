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
| `gate.spec.ts` 的遮挡窗口建时不显示 | e10d827 | +69 −9（净 +60） |

## 验证进度

- 两个模块提交后，`pnpm build:e2e` 通过；`flavors.test.ts`、tsc、eslint、prettier 通过。
- 准备跑 `test:e2e:one` 时屏幕已锁（锁屏发生在开跑前，前台是 `loginwindow`）。锁屏时采样前台没有意义，`@focus` 用例也拿不到焦点，所以 e2e 验证、全集核对和带记录的 CI 都停在这里，等屏幕打开后再继续。
- 编排者改为在 GitHub 的 macOS runner（macos-15，有图形会话、没人操作）上验证；本机的全集核对和带记录的 `ci pr` 不再跑，以 PR 的 GitHub CI 为准。

## GitHub 上的验证（2026-10-26，分支合过 main 后的 a30e1a0）

| 运行 | 内容 | 结果 |
|---|---|---|
| e2e-repeat 40315681975 | 反向检查：`scratch/T195-negative`（主进程不认开关，App 进 Dock、`show()` 激活），smoke × 1 | 按预期失败：`the app took the foreground in 10 of 10 samples; it needs @focus; the app is in the Dock: "ApplicationType"="Foreground"`，记在 `fixture: the app starts the host, and the sidebar shows a project the CLI added @smoke` 名下 |
| e2e-repeat 40315527229 | `PERF-bridge-gate: the judging load` × 1，`throttled: true`，后台模式 | "window on screen" 通过（`showInactive()` 的窗口在屏幕上，动画帧照常跑，降频的测量成立）；"window covered" 失败：`the app took the foreground in 85 of 93 samples; it needs @focus` |
| e2e-repeat 40319605626 | 修好 `cover()` 后（b4c92d9）再跑 `PERF-bridge-gate: the judging load` × 1 | 两个用例都通过，前台检查也通过。on screen：frames 19，p99 4.76 ms；covered：frames 0，p99 4.99 ms；blob 5,242,880 B，resync 5001 个事件 |
| ci 40315528900（PR #2） | 8 个检查 | license、protocol、testdata-scan、ui、rust、compat-framework 通过；ts、e2e 失败，都与 T195 无关，见下 |

**gate 的测量本身都完成了**：两个用例的用例体都跑完并通过了断言，失败只出在夹具收尾时的前台检查上，不是 PERF 闸门过不了。日志里的数字：

| 用例 | frames（1 s） | 主进程事件循环延迟 p50 / p99 / max（ms） | blob | resync |
|---|---|---|---|---|
| window on screen | 16 | 1.43 / 4.32 / 24.43 | 5,242,880 B，806 ms | 5001 个事件，506 ms |
| window covered | 0 | 1.44 / 4.72 / 87.16 | 5,242,880 B，431 ms | 5001 个事件，409 ms |

两个用例的 `visibility` 都是 `visible`；on screen 的帧数大于 0，covered 的帧数为 0，和断言一致。对照：main 的 nightly（前台模式）里 on screen 用例的 p99 是 8.65 ms，没过 5 ms 闸门。托管 runner 上 p99 本来就会波动，这次低于 5 ms 不说明什么，只说明测量在后台模式下照常完成、数字照常输出。

**covered 用例为什么抢前台**：`gate.spec.ts` 的 `cover()` 建遮挡窗口时没写 `show`，`BrowserWindow` 默认 `show: true`，建好就显示并激活 App（和上面实测里的 `show()` 一样），之后的 `showInactive()` 已经晚了。前 8 次采样在遮挡之前，之后一直在前台。改成 `show: false` 再 `showInactive()`。这正是前台采样要抓的情况：T078 写这个用例时还没人管激活。

**PR 的 ts 失败**：`apps/desktop/src/main/bridge/contract.test.ts` 的 "a request reaches the host re-serialized, and its response comes back" 收到空数组。`apps/desktop/src/main/bridge/testing.ts` 的 `settle` 只等 10 ms，runner 负载高时回应没赶上。本分支不碰 bridge；main 的 43ee901 上 ts 也是这个用例失败；本机单跑这个文件 10 次全过；PR 的第 2 次运行（attempt 2）里 ts 重跑通过。

**PR 的 e2e 失败**：没跑到测试，`build` 步骤里 `xtask host-bundle` 报 `codesign failed: target/runtime-pack/Contents/Helpers/node: No such file or directory`。rust-cache 整键命中恢复了 target，`target/runtime-pack/Contents` 目录在，`xtask ci e2e` 只查 `Contents` 是不是目录（`crates/xtask/src/ci/jobs/e2e.rs`），于是跳过了 `runtime-pack`，但里面没有 `Helpers/node`。main 的 8df33d4 是冷缓存，跑了 `runtime-pack`，e2e 通过。属于 CI 缓存的问题（T197 的范围）。

## 修复 1：E2E-single-instance 要标 `@focus`（2026-10-27）

10-26 夜里的 nightly（https://github.com/boris-w/cupper/actions/runs/40412822285 ）在 e2e 全集的后台段报 `app.spec.ts:14 › E2E-single-instance` "the app took the foreground in 4 of 15 samples; it needs @focus"。后台段共 26 条，其余 25 条都通过了前台检查；`@focus` 段 18 条全过。

### 规模估计（开工前）

| 模块 | 估计 |
|---|---|
| `app.spec.ts`：E2E-single-instance 加 `@focus`。prettier 会把用例体缩进一层，所以按整段计 | 35 |
| 合计 | 约 **35**，停止线 60 |
