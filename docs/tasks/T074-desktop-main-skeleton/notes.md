# T074 工作笔记

## 依赖版本（2026-10-19 复核）

原则同 T001、T070：取发布满约两周的版本；Electron 按设计取 44.x 的最新补丁版本。全部只在 `apps/desktop` 的 `devDependencies` 里声明，`dependencies` 为空。

| 包 | 版本 | 发布日期 | 说明 |
|---|---|---|---|
| electron | 44.9.0 | 10-17 | 开工时复核：仍是 44.x 的最新版本；45 还在 beta（45.0.0-beta.5）。自带 Chromium 152.0.7977.167、Node 24.22.0（releases.electronjs.org） |
| electron-builder | 26.17.0 | 09-26 | GitHub 上标为 Latest。npm 的 `latest` 标签停在 26.15.3，`v26` 标签是 26.17.0，像是 27 的 alpha 发版扰乱了标签；按 GitHub 和发布时间取 26.17.0 |
| @types/node | 24.19.1 | 10-01 | 24.19.2 不满两周 |
| vite / vitest | 8.3.3 / 5.0.3 | — | 沿用 T070 |

## 安装脚本与 `allowBuilds`

- **Electron 44 已经没有安装脚本**：`electron` 包的 `package.json` 里没有 `postinstall`，二进制在第一次运行 `electron` 命令时才下载（或者手动跑 `install-electron`）。所以任务书预期的"放行 `electron`"不需要。
- electron-builder 打包时自己下载 Electron 的 zip，放在默认缓存目录（`~/Library/Caches/electron/`），不依赖 `node_modules/electron/dist`。
- 单元测试不加载 Electron（见下文），也不需要二进制。
- 引入 electron-builder 后只有一个包带安装脚本：`electron-winstaller`（`select-7z-arch.js`，为 Windows 的 Squirrel 安装包挑 7-Zip）。pnpm 12 遇到未处理的脚本会直接报错退出，所以在 `allowBuilds` 里显式写成 `false`，表示看过、保持关闭。**没有放行任何包。**

## 构建

- **一次 `vite build --mode <种类>` 构建三个入口**，用 Vite 的 environments（`builder: {}`）：
  - `main`：ES 模块（`dist/main/index.js`），`electron` 和 Node 内置模块不打包，其余依赖全部打包（`noExternal: true`）；
  - `preload`：单个 CommonJS 文件（`dist/preload/index.cjs`），只 `require("electron")`；
  - `client`：主文档（`dist/renderer/index.html`）。全局 `root` 设为 `src/renderer`，产物才会落在 `dist/renderer/` 顶层，即 `app://cupper/` 的根目录。审查文档在 M1b 作为另一个 environment 加入。
- `--mode` 不是四种构建种类之一（`release`、`local`、`e2e`、`dev`）时直接报错，避免默认的 `production` 混进来。
- **`__CUPPER_FLAVOR__`**：e2e 专用的代码直接写在 `if (__CUPPER_FLAVOR__ === "e2e")` 里。实测发布构建的 `dist/main/index.js` 里搜不到 `e2e` 和 `CUPPER_E2E_DIR`：`appDirs` 和 `e2eDirs` 分成两个函数，正是为了让后者在发布构建里整个被摇掉。
- `vite.config.ts` 从 `src/shared/flavor.ts` 只引类型，并写 `.ts` 扩展名：不写扩展名时 Vite 8 会警告"native 配置加载器不支持"。纯类型的 import 允许带 `.ts`。
- 产物放在 `dist/` 下（根 `.gitignore` 已经忽略）：`dist/main`、`dist/preload`、`dist/renderer` 是 Vite 产物，`dist/tsc` 是 `tsc --build` 的输出，`dist/pack/<种类>` 是 electron-builder 的输出。asar 里只有 `package.json` 和前三个目录。
- 打包后 `app.name` 取自 bundle 的 `CFBundleName`（经 inspector 实测是 "Cupper E2E"），不需要往 asar 的 `package.json` 里写 `productName`。

## 关键取舍

- **模块只 `import type` Electron**：`security/*`、`window`、`menu` 都把 Electron 对象作为参数传进来，只有 `index.ts` 真正 import `electron`。原因：在 Vitest 里 import `electron` 会执行它的 `index.js`，触发下载二进制。这样单元测试不需要 mock 模块，也不需要 Electron。
- **`app://` 路径检查**：先 `decodeURIComponent`，拒绝 NUL 和反斜杠，再 `resolve(root, "." + path)`，最后检查是否以 `root + 分隔符` 开头。所以 `..%2f`、`%2e%2e%2f` 被真正拒绝（不读文件），而不是被"夹"回根目录；`renderer-x/` 这种同前缀的兄弟目录也被拒。`%2e%2e/` 这种不含编码斜杠的写法，URL 解析器本身就会把它当成 `..` 规范化掉（Chromium 和 Node 的 WHATWG 解析器都是），结果落在根目录内。CSP 响应头加在这个主机的所有响应上，另加 `X-Content-Type-Options: nosniff`。
- **导航**：`will-navigate` 只在主框架、并且是 http 或 https 时转给 `shell.openExternal`；`will-frame-navigate`、`will-redirect`、`will-attach-webview` 一律阻止。`loadURL` 不触发 `will-navigate`，所以主文档照常加载。
- **命令行开关**：只在发布和自用构建里检查，白名单为空。凡是以 `-` 开头的参数都算开关（Chromium 也认单横线），`--` 也算；不以 `-` 开头的参数不管。检查放在主进程脚本的第一句，用 `process.exit(1)` 立即退出，此时还没有创建任何目录。
- **e2e 的目录**：`CUPPER_E2E_DIR` 是每个用例的根目录，`data/`（宿主数据目录，userData 是其中的 `ui/`）和 `logs/` 放在它下面；T075 的宿主数据目录和 T090 的 harness 密钥文件可以沿用这个布局。没设或不是绝对路径时打印原因、退出码 1。单实例锁跟着 userData 走，所以不同用例的 App 可以并行。
- **userData 先 `mkdir` 成 0700** 再 `setPath`，不让 Electron 用默认权限创建；数据目录和日志目录同样是 0700，`main.log` 是 0600。
- **main.log**：同步写入。主进程日志量很小，同步写入在崩溃前的最后一行也不会丢；写入失败直接丢弃，不让日志拖垮主进程。家目录在每一行里写成 `~`（只在路径边界上替换，`/Users/uv` 不会被误改）。默认级别 info，没有做 debug 级别和 `diagnostics.verbose`（没有使用方）。
- **prefs**：一个 JSON 文件（userData 下的 `prefs.json`，0600）。键要匹配 `^[A-Za-z][\w.-]{0,127}$`（挡掉 `__proto__` 之类），值按 JSON 存储，整个文件上限 256 KiB（渲染进程不可信），写入先写临时文件再改名，并按调用顺序落盘；文件缺失或损坏时从默认值开始。**还没有接到 ShellAPI**：`init` 的快照、`prefs.set` 和 `onChange` 的转发属于 T076。
- **菜单**：顶层菜单用 Electron 的角色（`appMenu`、`fileMenu`、`editMenu`、`viewMenu`、`windowMenu`）加自己的子菜单，命令项的文案取 `command` 命名空间。第二个快捷键（⌘⇧O、⌘⇧S、⌘⌥←、⌘⌥→）做成隐藏的菜单项，macOS 默认仍然响应隐藏项的加速键。点击后把 `ShellCommand` 用 `webContents.send("shell:command", cmd)` 发给当前焦点窗口，通道名在 `src/shared/ipc.ts`，preload 一侧的接收属于 T076。开发构建在"视图"里多一个"切换开发者工具"。没有 Services 菜单。
- **窗口**：没有设 `vibrancy` 和透明背景（T152 的半透明侧边栏）；`acceptFirstMouse` 不设，保持默认 false。`<meta name="color-scheme" content="light dark">` 让空白页在暗色模式下是暗色。

## 验证记录

环境：macOS 26.4，Apple Silicon；Electron 44.9.0。临时目录都在会话的 scratch 目录里，用完删除。

**e2e 构建的产物**（`pnpm build:e2e`）

- `@electron/fuses read`（1.8.0）：RunAsNode、EnableCookieEncryption、EnableNodeOptionsEnvironmentVariable、LoadBrowserProcessSpecificV8Snapshot、GrantFileProtocolExtraPrivileges 为 Disabled；EnableNodeCliInspectArguments、EnableEmbeddedAsarIntegrityValidation、OnlyLoadAppFromAsar 为 Enabled。另有第 9 个 fuse 显示为 `undefined is Enabled`：Electron 44 比 @electron/fuses 1.8.0 多一个 fuse。
- Info.plist：`CFBundleIdentifier` 为 `dev.cupper.app.e2e`，`CFBundleName` 为 "Cupper E2E"，`LSMinimumSystemVersion` 为 14.0，有 `ElectronAsarIntegrity`。`codesign -dv`：`Signature=adhoc`，flags `0x2(adhoc)`，没有 runtime 标志。
- asar 清单：`package.json`（已去掉 scripts 和 devDependencies）、`dist/main/index.js` 与 `.map`、`dist/preload/index.cjs`、`dist/renderer/index.html`。

**启动与单实例锁**（直接运行 `Contents/MacOS/Cupper E2E`）

| 步骤 | 结果 |
|---|---|
| 不设 `CUPPER_E2E_DIR` | 退出码 1，stderr："an e2e build needs CUPPER_E2E_DIR, an absolute path" |
| 第一个实例，根目录 A | 屏幕上出现一个 1280 × 820 的窗口（用 `CGWindowListCopyWindowInfo` 按 pid 查到）；`main.log` 有 `started`、`flavor: e2e` |
| 第二个实例，同一根目录 A | 不到 1 秒退出，退出码 0；`main.log` 记下 "another instance is running; quitting"；第一个实例仍在运行 |
| 第三个实例，根目录 B | 独立运行，有自己的窗口 |
| 两个实例都收到 SIGTERM | 退出码都是 0，没有残留进程 |
| 目录 | 根目录下只有 `data/ui/`（Chromium 的各种存储）和 `logs/main.log`；`data`、`data/ui`、`logs` 都是 0700，`main.log` 是 0600 |

**主文档的实际行为**（同一个产物加 `--remote-debugging-port=0`，经 CDP 在页面里求值）

| 检查 | 结果 |
|---|---|
| `location.href` / `origin` / `title` | `app://cupper/index.html` / `app://cupper` / "Cupper" |
| `typeof require`、`typeof process` | 都是 `undefined`（沙箱、上下文隔离） |
| 插入内联脚本 | 被 Trusted Types 拦下（"This document requires 'TrustedScript' assignment"），脚本没有执行 |
| `document.body.innerHTML = "<b>x</b>"` | 抛 TypeError（`require-trusted-types-for 'script'`） |
| `trustedTypes.createPolicy("default")` | 抛 TypeError（CSP 只允许 `cupper-worker`） |
| `fetch("https://example.com/")` | 被拒（`connect-src 'none'`） |
| `window.open("app://cupper/index.html")` | 返回 null，没有新窗口 |
| `location.href = "app://cupper/other.html"` | 1 秒后仍在 `index.html` |
| geolocation、notifications、clipboard-read 的权限查询 | 都是 denied |
| `<img src="app://cupper/..%2fmain%2findex.js">` | 加载失败 |
| `main.log` | 没有 `preload failed`、`renderer gone` |

为了不打开浏览器，没有在 e2e 里测 http 链接转给 `openExternal`，这一条由单元测试覆盖。

**发布构建的开关检查**：把 `--mode release` 的产物用开发构建的打包配置打到临时目录（只为这次检查，没有保留），分别带 `--inspect`、`--remote-debugging-port=9222`、`--no-sandbox`、`--user-data-dir=/tmp/x`、`-psn_0_12345`、`--` 启动：全部退出码 1，stderr 写明被拒的开关，耗时约 0.12 s（第一次启动 1.8 s，含系统的首次检查）；`Application Support/cupper` 和 `Logs/cupper` 都没有被创建。没有不带开关地启动它，因为那样会用到真实的发布数据目录。

**变异检查**：把前缀检查改成不带分隔符的 `startsWith(base)`，"refuses paths that leave the root" 失败；在允许的权限里加上 `clipboard-read`，两条权限用例失败。

## 发现

- **Electron 自带的 Info.plist 有 5 条用途说明**：`NSCameraUsageDescription`、`NSMicrophoneUsageDescription`、`NSBluetoothAlwaysUsageDescription`、`NSBluetoothPeripheralUsageDescription`、`NSAudioCaptureUsageDescription`。electron-builder 原样保留，`mac.extendInfo` 只能增改、不能删除。设计 5.10.1、5.10.2 要求发布构建的 Info.plist 不含用途说明，T160 / T180 需要在打包后删掉（例如 `afterPack` 钩子）。dev 和 e2e 构建暂时保留。
- **Electron 44 有第 9 个 fuse**，@electron/fuses 1.8.0 不认识。T161 逐项断言 fuses 时需要处理。
- **`~/Library/Preferences/dev.cupper.app.e2e.plist`**：e2e 产物启动后，Chromium 往这个 bundle id 的用户默认值里写了几项（`NSTreatUnknownArgumentsAsOpen` 等），它不在 `CUPPER_E2E_DIR` 下面。内容与用户数据无关；验证结束后已删除。
- **缩放后的交通灯位置**：Codex 按 `(44 × 缩放 − 14) / 2` 计算 y。本任务只按默认缩放设成 15，菜单里的缩放角色不会重新计算，留给 T152（窗口外观的细节）。
