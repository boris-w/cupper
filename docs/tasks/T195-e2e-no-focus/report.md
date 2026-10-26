# T195 汇报

- 执行者：子 agent（执行者）
- 分支：`test/T195-e2e-no-focus`（T052 worktree），PR #2
- 完成日期：2026-10-26

T195 已完成：PR #2 的 8 个检查全绿，GitHub runner 上的 e2e 冒烟子集全部通过（17 个后台用例、10 个 `@focus` 用例）。验证中前台检查还抓到了测试代码自己抢前台的一处问题，已修，修好后在 runner 上重跑通过。

## 做了什么

不带 `@focus` 的 e2e 用例启动的 App 现在不进 Dock、不激活、不抢前台；`@focus` 用例和 `focusWindow`（T189）不变。

1. **先实测哪种显示方式不抢前台**。用 Electron 44.9.0 的最小脚本显示窗口 4 s，每 0.2 s 用 `lsappinfo front` 采样一次前台应用：

   | 做法 | 前台是这个脚本的采样 |
   |---|---|
   | `show()` | 17/29 |
   | `accessory` + `show()` | 15/18 |
   | `showInactive()` | 0/20，但 App 仍在 Dock 里 |
   | `accessory` + `showInactive()` | 0/22，应用类型 `UIElement`，不在 Dock |

   结论是两者一起用：`showInactive()` 管不抢前台，`accessory` 管不进 Dock。
2. **主进程**（`apps/desktop/src/main/index.ts`）：只有 e2e 构建、并且 `CUPPER_E2E_BACKGROUND=1` 时，才执行 `app.setActivationPolicy("accessory")`，`ready-to-show` 时用 `showInactive()`。发布和自用构建里这段代码作为死分支被去掉；`flavors.test.ts` 把这个变量名加进入口名检查，确保它只出现在 e2e 构建里。
3. **夹具**（`tools/e2e/src/fixture.ts`）：
   - 按用例标签自动开启后台模式，用例代码不用改。
   - 后台用例运行期间，每 250 ms 采样一次前台应用。只要有一次前台是本用例的 App，用例就失败，信息是 `the app took the foreground in N of M samples; it needs @focus`，记在该用例名下。
   - 用例结束时核对应用类型必须是 `UIElement`，否则报 `the app is in the Dock: …`。
4. **`tools/e2e/src/gate.spec.ts` 的 `cover()`**：建遮挡窗口时加了 `show: false`。原来 `BrowserWindow` 默认 `show: true`，窗口一建好就显示并激活 App，后面的 `showInactive()` 已经晚了。这是前台检查在 runner 上第一次跑时抓到的。T078 写这个用例时还没人管激活。
5. 没有动 `tools/e2e/playwright.config.ts`。`gate.spec.ts` 里 T197 加的 `@perf` 标记是你合 main 时自动合进来的，和我改的地方不冲突。

## 关键取舍

- **用环境变量，不用命令行参数**：夹具本来就在构造 env，而且变量名能直接放进 `flavors.test.ts` 现有的入口名检查里。
- **后台模式由标签决定，不由用例自己声明**：
  - 漏标 `@focus` 却需要焦点的用例，会在 `focusWindow` 那里失败；
  - 没标 `@focus` 却抢了前台的用例，会被采样抓出来。

  两头都不会悄悄漏过。
- **一次命中就判失败，不设容差**：后台模式实测是 0 次命中。
- **采样用异步的 `execFile`，不阻塞用例**；应用类型只在用例收尾时同步查一次。
- **`throttled` 和三个降频参数没有改**：它们是 Chromium 的启动参数，跟激活无关。`showInactive()` 的窗口仍在屏幕上，T078 测量需要的"窗口可见"仍然满足，runner 上的实测也证实了这一点（见下）。

## 验证方式

按你的安排，验证改在 GitHub 的 macOS runner（macos-15）上做。本机开工时屏幕已锁，所以本机的全集核对和带记录的 `ci pr` 都不跑了。

**本机**
- `pnpm build:e2e`、`flavors.test.ts`、tsc、eslint、prettier 都通过。

**PR #2 的 ci（run 40326991029，提交 ea7f1ec）：8 个检查全绿**
- 通过的检查：license、protocol、testdata-scan、ts、ui、compat-framework、rust、e2e。
- e2e 的结果是 `31 passed, 0 failed`，其中 4 个是构建步骤，其余 27 个是测试：
  - `test-e2e` 跑了 17 个不带 `@focus` 的 `@smoke` 用例，全部通过了前台采样和 Dock 检查。只要有一次前台命中或进了 Dock，用例就会失败。
  - `test-e2e-focus` 跑了 10 个 `@smoke` + `@focus` 用例，照常跑完。
- 两组用例数是我在本机用 `playwright test --list` 列出来的。

**反向检查（run 40315681975，`scratch/T195-negative`）：按预期失败**
- 改动：主进程不认开关，App 进 Dock、用 `show()`。
- smoke 失败信息是 `the app took the foreground in 10 of 10 samples; it needs @focus; the app is in the Dock: "ApplicationType"="Foreground"`，记在 `fixture: the app starts the host, …` 名下。
- 失败确实来自这两项检查，构建是正常的。

**gate.spec（`PERF-bridge-gate: the judging load` × 1）**
- 第一次（run 40315527229）：两个用例的测量都完成了，用例体断言也都通过。covered 用例失败在收尾的前台检查：85/93 次采样里 App 在前台，原因就是上面第 4 点的 `cover()`。
- 修好后重跑（run 40319605626，提交 b4c92d9）：两个用例都通过。

  | 用例 | frames | p99 | blob | resync |
  |---|---|---|---|---|
  | on screen | 19 | 4.76 ms | 5 MiB | 5001 个事件 |
  | covered | 0 | 4.99 ms | 5 MiB | 5001 个事件 |

  所以 `throttled` 测量在后台模式下照常工作。runner 上的 p99 会波动（main 的 nightly 里是 8.65 ms），按 T197 的口径不以此判断。

**规模**：`cargo xtask size` 为 +69 −9（净 +60），估计是约 60，停止线 300。

所有结果和测量数字都写在 `docs/tasks/T195-e2e-no-focus/notes.md` 里。

## 提交列表

```
ea7f1ec Merge branch 'main' into test/T195-e2e-no-focus
529a075 docs(T195): record the gate passing on the runner after the cover fix
b4c92d9 docs(T195): record the checks on the macOS runner
e10d827 test(e2e): the gate's covering window no longer activates the app
a30e1a0 Merge branch 'main' into test/T195-e2e-no-focus
69439ad docs(T195): record the size per module and where the checks stopped
3edaa76 test(e2e): tests that need no focus leave the foreground to the user
83fe1c4 feat(desktop): an e2e app can stay out of the foreground
52f5951 docs(T195): record the checks, the probe of the ways to show and the estimate
```

两个合并提交是你合 main 时做的。工作区干净，本地的 `scratch/T195-negative` 已删。远端的那个分支等你删。

## 遗留问题 / 范围外发现

- **`showMainWindow()` 还有一处 `show()`**：窗口已经存在时（`second-instance`、`activate`）仍调 `mainWindow.show()`。现有测试不会走到这里，所以没改。以后如果有后台用例会触发它，前台检查会直接报出来，到时候再改成按开关用 `showInactive()`。
- **全量 e2e 没有在后台模式下跑过**：这次只有冒烟子集、gate 和反向检查在 runner 上跑过。其余非 `@focus` 用例要等 nightly 或 `ci e2e` 跑到时才会经过前台检查。如果有用例还在主进程里建窗口却没写 `show: false`，会在那时暴露。我在 `tools/e2e/src` 里查过，除了 `cover()` 没有别的建窗口或激活的地方。
- **ts 偶发失败**：`apps/desktop/src/main/bridge/testing.ts` 的 `settle()` 只等固定 10 ms，T198 已记下。
- **e2e 缓存问题**：`crates/xtask/src/ci/jobs/e2e.rs` 原来只用 `is_dir()` 判断 runtime 有没有暂存，已由 T197 的 PR #4 修好，在 PR #2 上确认 `runtime-pack` 步骤会照常运行。
