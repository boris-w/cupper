# T107 notes

## 1. 版本

- `electron` 从 44.9.0 升到 **45.2.3**（2026-11-04 发布，45 的最新补丁；45.0.0 发布于 10-21）：`apps/desktop` 和 `tools/e2e` 两处都是精确版本。pnpm 的供应链策略检查通过。
- 45 的底层：Chromium 156（45.2.2 起是 156.0.8078.80），Node 24.22.0，V8 15.6。`vite.config.ts` 的目标从 `chrome152` 改成 `chrome156`；Node 仍是 24。
- 配套依赖都不用跟着升：
  - electron-builder 26.17.0 打包 45 正常；
  - 它自己和我们用的 `@electron/fuses` 都是 1.8.0，45 没有新增 fuse（见第 2 节）；
  - 2.x 版本认得第 9 个 fuse 的名字，但只是改名，读写不受影响，所以没有升。

## 2. fuses

- Electron 45.2.3 的 `build/fuses/fuses.json5` 仍是 9 个，`_version` 为 1，名字和出厂值都与 44 相同：
  - `run_as_node` 1、`cookie_encryption` 0、`node_options` 1、`node_cli_inspect` 1；
  - `embedded_asar_integrity_validation` 0、`only_load_app_from_asar` 0、`load_browser_process_specific_v8_snapshot` 0；
  - `grant_file_protocol_extra_privileges` 1、`wasm_trap_handlers` 1。
- 本机用 45.2.3 不签名构建了一次发布配置（只构建这一次），在它上面跑 T161 的 release-config 检查，3 条都通过：
  - fuse 线前 8 个按 5.7.9 发布一列，第 9 个开启，没有多出来的 fuse；
  - asar 清单、ElectronAsarIntegrity、测试入口、`--build-info` 都正常；
  - 带未知开关启动，以状态 1 立即退出。
- 所以 5.7.9 的两列和扫描的期望值都不用改。
- 写 fuse 后 Electron Framework 的签名失效、要补 ad-hoc 签名，这件事在 45 上照旧（T161）。

## 3. 45 的 breaking changes 逐条对照

来源：electron/electron `docs/breaking-changes.md`（v45.2.3）的 "Breaking API Changes (45.0)"，以及 v45.0.0 至 v45.2.3 的发布说明。

| 变化 | 我们的用法 | 影响 |
|---|---|---|
| 沙箱 preload 去掉了 `events`、`timers`、`url` 的垫片，以及 `Buffer`、`setImmediate`、`clearImmediate` 全局 | preload 是沙箱的；构建出的 `dist/preload/index.cjs` 只 `require("electron")`，没有用到这几样 | 无 |
| 沙箱 preload 里 `ipcRenderer`、`process` 改用原生 `EventEmitter`，`events` 的静态方法没了 | preload 只用实例方法 `ipcRenderer.once`、`ipcRenderer.on` | 无 |
| 子框架创建的 Worker 要 `nodeIntegrationInSubFrames` 才有 Node | `nodeIntegrationInWorker`、`nodeIntegrationInSubFrames` 都关 | 无 |
| DevTools 扩展框架里的 preload | 不用扩展 | 无 |
| `getUserMedia` 的 `chromeMediaSource: 'desktop'` 不再接受 WebContents 来源 | 不用 | 无 |
| 删掉 `contentTracing.enableHeapProfiling()`；录制改为 Perfetto 格式 | 不用 `contentTracing`（perf 的数据来自 e2e 里自己的计时） | 无 |
| 屏幕捕获在权限处理器里报为 `display-capture` | 权限处理器只放行 `clipboard-sanitized-write`，其余一律拒绝 | 无，照旧拒绝 |
| `document.requestStorageAccessFor` 默认关闭 | 不用 | 无 |
| asar 里文件的 fd 只能经 `fs` 使用；`fs.stat` 跟随 asar 里的链接 | 主进程用 `readFile(路径)` 读 asar 里的页面（`app-protocol.ts`），不拿 fd 交给原生代码或子进程；`openSync` 只用在 userData 里的日志 | 无 |
| 非沙箱窗口用 `window.open()` 打开的子窗口进独立的沙箱进程 | `setWindowOpenHandler` 一律 `deny`，外链交给系统浏览器 | 无 |
| 同步的 `safeStorage` 方法弃用 | 不用 | 无 |
| 修复：`runAsNode` 关闭时，`child_process.fork()` 改为抛错 | 主进程拉起宿主用的是 `spawn`，不用 `fork` | 无 |

任务书点名的几项：

- `webUtils.getPathForFile`（T142，preload 里用）：45 没有改动；tsc 通过。
- `sendInputEvent`（e2e 的按键）：没有改动；Q26 复测里照旧触发 `before-input-event`，见第 4 节。
- `will-frame-navigate`（`security/navigation.ts`）：没有改动。
- `utilityProcess`：不用。45 只修了两处崩溃，新增了 `localAIHandler`。
- 窗口和菜单：45 新增 `MenuItem.badge`，另有若干崩溃修复，没有破坏性改动。

本机：`tsc --build`（desktop、e2e）、eslint、desktop 的单元测试（109 个）都通过。

## 4. Q26 在 45 上复测

- 用 T077 的探针（`q26/run.mjs`、`q26/main.cjs`，复制到本目录），在 Electron 45.2.3（electron-builder 缓存里的官方 zip）上跑了一次。
- 唯一的改动：窗口不抢焦点，即 `setActivationPolicy("accessory")` 加 `showInactive()`，与 e2e 的后台模式相同（T195）。这样在本机跑也不抢用户的焦点。
- 结果在 `q26/result.json`，与 T077 在 44.9.0 上的 `result.json` 完全相同：
  1. Playwright 的键盘输入不触发 `before-input-event`，只有 `input-event` 的 keyDown、keyUp；页面的 keydown 是 `isTrusted`。
  2. `sendInputEvent` 触发 `before-input-event`（keyDown、keyUp），页面的 keydown 是 `isTrusted`。
  3. `input-event` 的键盘事件运行时带 `key`、`code`、`isComposing`、`isAutoRepeat`、修饰键。
  4. `page.click()` 产生同一按钮的 `mouseDown`、`mouseUp`。
  5. `focusedFrame` 可靠：主框架的输入框里与 `mainFrame` 相同，iframe 里不同。
- 结论：Q26 的五条结论在 45 上都成立，真实输入规则和 e2e 的测法不用改。

## 5. MAN-keyboard

- 检查步骤在 `man-keyboard.md`：照 T099 的记录在 45 的 e2e 构建上重做一遍，结果填回本目录的那份文件。
- `tests/ids.toml` 里现有的证据是 `electron = "44.9.0"`，`docs/manual/README.md` 规定 MAN-keyboard 的版本粒度是 `electron = "major"`，所以升到 45 后旧证据作废。重做通过后，由 `main` 追加新证据。

## 6. 范围外

- 设计和 CLAUDE.md 里写着 "Electron 44" 的地方要由 `main` 更新：
  - CLAUDE.md 技术栈表；
  - 设计 5.1.x（`01-repo-build-ci.md` 的版本表和 `allowBuilds` 一句）；
  - 5.10.6（`10-release.md` 的起始版本）；
  - 5.7.8、5.7.9 里提到 44 的几句（第 9 个 fuse、`input-event` 的类型定义、GPU helper）；
  - 13 的 Q26（可以补"45.2.3 上复测相同，T107"）。
- `apps/desktop/selfuse/coexist.test.ts` 那句注释，第一次改成了 "Electron 45 runs the GPU process in the plain helper"。但断言用的是 `arrayContaining`，只核对了普通 helper 和 Renderer helper 在跑，并没有证实 GPU helper 不启动。按 `main` 的决定（2026-11-09），注释改为只写断言实际核对的内容，不写版本，也不写 GPU 进程怎么跑；断言本身不动。

## 7. 删掉 T260 的 `dropVisibilityListenersOnClose`（2026-11-13）

- T260 在 44 上加了这个函数：窗口 `closed` 时去掉 show/hide/minimize/maximize/restore 的监听，因为 Electron 自带的 JS 监听在窗口销毁后被 macOS 的遮挡变化触发时，会调 `this.isVisible()` 而抛 "Object has been destroyed"。
- 上游 electron/electron#53971 删掉了这个 JS 监听，回合到 45-x-y 的是 electron/electron#54048（merge commit 197080b8），v45.2.3 已经包含（`gh api compare`：ahead 258、behind 0）。
- 在 45.2.3 的产物里核对：本机 Electron 缓存里的 `electron-v45.2.3-darwin-arm64.zip`（sha256 与 `electron/checksums.json` 一致），解出 `Electron Framework`，检索内置的 JS：
  - `visibilityChanged`：44.9.0 有 1 处，45.2.3 为 0；
  - `-window-visibility-change`（那个监听发给 webContents 的内部事件）：44.9.0 有 1 处，45.2.3 为 0；
  - `windowDidChangeOcclusionState` 两版都在：遮挡变化照旧发 show/hide，只是已经没有会在销毁后出错的 JS 监听了。
- 所以按"不留技术债"删掉：`window.ts` 里的函数和它的常量、`index.ts` 的调用、`hardening.test.ts` 的那条用例。三个文件与 T260 之前逐字相同。
- 同一次回合还从沙箱 preload 里去掉了 `Buffer`、`setImmediate`、`events`、`timers`、`url`，第 3 节的表里已经记过。这次按当前代码重新构建（`vite build --mode release`）后核对：`dist/preload/index.cjs` 只有 `require("electron")`，没有 `Buffer`、`setImmediate`、`clearImmediate`。
