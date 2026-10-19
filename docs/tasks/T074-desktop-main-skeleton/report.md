# T074 汇报

- 执行者：子 agent（Electron 工程师，桌面主进程）
- 分支：`feat/T074-desktop-main-skeleton`
- 完成日期：2026-10-19

## 做了什么

**结果：** 验收标准都满足了。在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T074-desktop-main-skeleton/ci/` 通过：
- rust 32 项、ts 64 项（其中 desktop 38 项）、license 1 项；
- `results` 里 `UNIT-main-hardening` 为 pass；
- 摘要文件是 `docs/tasks/T074-desktop-main-skeleton/ci/pr-28b63179e4c6.json`。

**规模：** 不含 lock 文件共 1,600 行，其中 CI 摘要 117 行。
- 产品代码约 720 行；
- 测试约 490 行；
- 构建配置约 170 行（Vite、tsconfig、package.json、两份 electron-builder 配置）；
- `notes.md` 94 行。

lock 文件新增约 2,100 行，几乎都是 electron-builder 带进来的依赖。

**交付内容**（`apps/desktop`，包名 `@cupper/desktop`）

1. **依赖与构建**
   - Electron 用 44.9.0。开工时复核过：它仍是 44.x 的最新版本，45 还在 beta。
   - 所有依赖都写在 `devDependencies`，`dependencies` 为空。
   - 一次 `vite build --mode <种类>` 构建三个入口：
     - 主进程打成 ES 模块；
     - preload 打成单个 CommonJS 文件，只 `require("electron")`；
     - 主文档。
   - 审查文档的位置已在配置里留好。
   - `--mode` 只接受 release、local、e2e、dev，其他值直接报错。
   - `__CUPPER_FLAVOR__` 由 Vite 的 `define` 注入。发布构建的产物里搜不到 `e2e` 和 `CUPPER_E2E_DIR`。
2. **`app://cupper` 协议**
   - 注册为 standard、secure，开启 `supportFetchAPI` 和 `codeCache`。
   - 按 5.7.8 下发主文档的 CSP，另加 `X-Content-Type-Options: nosniff`。
   - 路径先解码，再拒绝 NUL 和反斜杠，然后 resolve，最后检查是否以"根目录 + 分隔符"开头。
3. **加固基线**
   - webPreferences：
     - 开启 `sandbox` 和 `contextIsolation`；
     - 关闭 `nodeIntegration`、`nodeIntegrationInWorker`、`nodeIntegrationInSubFrames`、`webviewTag`、`allowRunningInsecureContent`、`experimentalFeatures`、`navigateOnDragDrop`；
     - 发布和自用构建的 `devTools` 为 false。
   - 另外调用了 `app.enableSandbox()`。
   - 每个 webContents 都拦截 `will-navigate`、`will-frame-navigate`、`will-redirect`、`will-attach-webview`。
   - 新窗口一律拒绝。主框架的 http 和 https 链接交给 `shell.openExternal`，判断沿用 shell-api 的 `isExternalUrl`。
   - 权限的请求、检查、设备三个处理器，都只放行 `clipboard-sanitized-write`。
   - 发布和自用构建在主进程第一句检查命令行开关：
     - 白名单为空；
     - 以 `-` 开头的参数都算开关，`--` 也算；
     - 遇到就用 `process.exit(1)` 退出。
4. **窗口**
   - 1280 × 820，最小 480 × 600；
   - `hiddenInset`，交通灯在 (16, 15)；
   - `show: false`，`ready-to-show` 时再显示；
   - `acceptFirstMouse` 保持默认值。
5. **生命周期**
   - 先显式设置 userData，再拿单实例锁：
     - userData 是数据目录下的 `ui/`，目录权限 0700；
     - 数据目录：发布 `cupper/`、自用 `cupper-local/`、开发 `cupper-dev/`；e2e 用 `$CUPPER_E2E_DIR/data`。
   - 再次启动只把已有窗口带到前面。
   - 关掉最后一个窗口时 App 不退出；点 Dock 图标会重新建窗口。
6. **其他**
   - `main.log`：JSON Lines，5 MB × 3 轮转，家目录写成 `~`，开发构建同时输出到终端。
   - prefs：userData 下的 `prefs.json`。键名要过校验，值只接受 JSON，整个文件不超过 256 KiB，先写临时文件再改名，按调用顺序落盘。
   - 菜单骨架：5.7.7 的全部 M1 快捷键都注册为加速键，文案取 i18n 的 `command` 命名空间。点击后用 `webContents.send("shell:command", cmd)` 发给当前焦点窗口，通道名放在 `src/shared/ipc.ts`。
7. **打包与脚本**
   - `electron-builder.e2e.yml`：
     - ad-hoc 签名，关闭 hardened runtime；
     - fuses 与发布构建相同，只有 `enableNodeCliInspectArguments` 保持开启、`enableCookieEncryption` 关闭。
   - `electron-builder.dev.yml`：
     - 独立的身份（`dev.cupper.app.dev`，"cupper Dev"）；
     - ad-hoc 签名，不烧录 fuses。
   - 两份配置都只出 arm64 的 dir 产物，`LSMinimumSystemVersion` 为 14.0。
   - 脚本：`build:dev`、`build:e2e`、`test`，根目录另有 `pnpm build:e2e`。

## 关键取舍

- **`allowBuilds` 没有放行 electron。** Electron 44 的包已经没有安装脚本，二进制在第一次运行时才下载。打包时，electron-builder 自己下载 zip 到默认缓存。所以任务书预期的"放行 electron"并不需要，`pnpm install` 和 CI 也不会下载 Electron。
  - 唯一带安装脚本的是 `electron-winstaller`，它只为 Windows 安装包挑 7-Zip。
  - pnpm 12 遇到没处理的脚本会报错，所以把它显式写成 `false`。
  - **没有放行任何包。**
- **模块只 `import type` Electron。** `security/*`、`window`、`menu` 把 Electron 对象当参数传进来，只有 `index.ts` 真正 import `electron`。原因是在 Vitest 里 import `electron` 会触发下载二进制；这样单元测试不用 mock，也不用 Electron。
- **Vite 用 environments API。** 一个配置、一条命令，就能构建三个入口：`builder: {}`，三个 environment 各自配置。
  - 全局 `root` 设为 `src/renderer`，主文档才会落在 `dist/renderer/` 顶层，也就是 `app://cupper/` 的根。
  - `vite.config.ts` 只引类型，并写 `.ts` 扩展名，否则 Vite 8 会给出 native 配置加载器的警告。
- **e2e 目录布局由我来定。** userData 和 `main.log` 在本任务的范围内，所以读取 `CUPPER_E2E_DIR` 写在了这里，只编进 e2e 构建。
  - 布局是 `<root>/data`（userData 是其中的 `ui/`）加 `<root>/logs`。
  - 没设或不是绝对路径时退出，退出码 1。
  - T075 的宿主数据目录和 T090 的密钥文件可以沿用这个布局。
- **`main.log` 同步写入。** 主进程日志量很小，同步写崩溃前也不丢最后一行；写失败就丢弃。
- **菜单顶层用 Electron 的角色**（`appMenu`、`fileMenu`、`editMenu`、`viewMenu`、`windowMenu`），这样代码里不用写死顶层菜单的标题。
  - 第二个快捷键做成隐藏菜单项，macOS 仍然响应。
  - 开发构建多一个"切换开发者工具"。
  - 没有 Services 菜单。
- **依赖版本**（都只在 `apps/desktop` 声明）：
  - electron-builder 26.17.0（09-26）：GitHub 标为 Latest，但 npm 的 `latest` 标签停在 26.15.3，按发布时间取 26.17.0。
  - @types/node 24.19.1：24.19.2 不满两周。
  - vite 8.3.3、vitest 5.0.3：沿用 T070。

## 验证方式

- [x] **UNIT-main-hardening**（`src/main/hardening.test.ts`，27 个用例），逐项对照：
  - **webPreferences**：四种构建都检查全部字段；发布和自用没有 DevTools；窗口参数符合 5.7.4，`acceptFirstMouse` 未设置。
  - **导航和新窗口**：导航、重定向、webview、新窗口全部被拦；只有主框架的 http 和 https 交给 `openExternal`，`file:`、`javascript:`、`app:` 和自定义 scheme 不会被打开。
  - **权限**：请求和检查都只放行 `clipboard-sanitized-write`，`clipboard-read`、media、geolocation、notifications 等 9 项被拒；设备一律拒绝。
  - **路径穿越**：`..%2f`、`%2e%2e%2f`、大写编码、多级穿越、同前缀的兄弟目录、`%5c`、`%00`、非法编码都返回 404，而且读文件函数一次都没被调用；其他主机、POST、目录、不存在的文件也都拒绝。
  - **CSP 响应头**：每个成功响应都带主文档的 CSP 和 nosniff，Content-Type 正确。
  - **未知开关**：11 种开关（`--inspect*`、`--remote-debugging-port`、`--js-flags`、`--no-sandbox`、`--disable-web-security`、`--user-data-dir`、`--proxy-server`、`-inspect`、`--`、未知开关）都调用 `exit(1)`；没有开关时正常启动。
  - **变异检查**：前缀检查去掉分隔符，或者在白名单里加上 `clipboard-read`，相应用例都会失败。
- [x] **`pnpm build:e2e` 的产物能启动，单实例锁生效**（手动验证，用脚本自动执行）：
  - **启动**：设好 `CUPPER_E2E_DIR` 后，屏幕上出现 1280 × 820 的窗口（按 pid 用 `CGWindowListCopyWindowInfo` 查到）。
  - **单实例锁**：
    - 同一个根目录再启动一次，不到 1 秒以退出码 0 退出，`main.log` 留了记录，第一个实例照常运行；
    - 换一个根目录，可以独立运行。
  - **退出**：SIGTERM 后两个实例的退出码都是 0，没有残留进程。
  - **缺少根目录**：不设 `CUPPER_E2E_DIR` 时退出码 1。
  - **目录权限**：`data`、`data/ui`、`logs` 都是 0700，`main.log` 是 0600。
  - **产物检查**：
    - 用 `@electron/fuses read` 逐项核对，结果与配置一致；
    - Info.plist 的 bundle id、名称、`LSMinimumSystemVersion`、`ElectronAsarIntegrity` 都对；
    - `codesign` 显示为 ad-hoc 签名，没有 hardened runtime；
    - asar 里只有 `package.json` 和三个产物目录。
- [x] **真实产物里的主文档**（额外检查：加 `--remote-debugging-port=0`，经 CDP 求值）：
  - origin 是 `app://cupper`；
  - `require` 和 `process` 都不存在；
  - 内联脚本和 `innerHTML` 被 Trusted Types 拦下，不能建名为 `default` 的策略；
  - 远程 fetch 被拒；
  - `window.open` 返回 null，页面内导航被拦；
  - geolocation、notifications、clipboard-read 都是 denied；
  - 用 `<img>` 做路径穿越加载失败；
  - 没有 preload 错误。
- [x] **发布构建的开关检查**（额外检查）：
  - 构建：`--mode release` 的产物，打包到临时目录，检查后删除。
  - `--inspect`、`--remote-debugging-port`、`--no-sandbox`、`--user-data-dir`、`-psn_…`、`--` 都让进程以退出码 1 退出，约 0.12 秒，而且没有创建任何数据目录。
  - 没有不带开关地启动它，因为那样会用到真实的发布数据目录。
- [x] **每个提交单独通过**：在临时克隆里，对 5 个代码提交逐个跑了 `cargo xtask ci ts`，全部通过。
- [x] **测试标题**：hardening 的用例用 `describe("UNIT-main-hardening")`。paths、prefs、log、menu 的 11 个用例没有编号，与 T070 的做法一致。
- [x] **清理与检查**：
  - 入库文件扫描过不可见字符，结果为 0；
  - 入库内容不含本机路径和用户名；
  - 验证时 macOS 在 `~/Library/Preferences` 下生成的两个 plist 已经删除；
  - 没有残留进程。

## 提交列表

```
352380b docs(task): record the T074 pr CI summary
28b6317 docs(task): add T074 working notes
b880c68 build(desktop): package dev and e2e builds with electron-builder
baa73ad feat(desktop): start the app from a Vite build
4109c39 feat(desktop): add the menu skeleton
4385933 feat(desktop): add app directories, main.log and prefs
b8d1d15 feat(desktop): add the main-process hardening baseline
```

## 遗留问题 / 范围外发现

1. **`apps/desktop` 以外改了哪些**：
   - `pnpm-workspace.yaml`：`allowBuilds` 里加了 `electron-winstaller: false`，注释改成"`false` 表示看过、保持关闭"。
   - 根 `package.json`：加了 `build:e2e` 脚本。
   - `pnpm-lock.yaml`。

   根目录的 `pnpm dev` 要先能构建宿主，留给 T075 以后。
2. **设计和计划里"electron 的下载脚本需要放行"的说法已经过时**（5.1.5、`pnpm-workspace.yaml` 原注释、本任务书）：Electron 44 没有安装脚本了，建议编排者更新。
3. **Electron 自带的 Info.plist 有 5 条用途说明**：相机、麦克风、两条蓝牙、音频采集。
   - electron-builder 原样保留，`extendInfo` 只能增改、不能删除。
   - 设计 5.10.1、5.10.2 要求发布构建不含用途说明，T160 / T180 需要打包后再删（例如 `afterPack` 钩子）。
   - dev 和 e2e 构建暂时保留。
4. **Electron 44 有第 9 个 fuse**，@electron/fuses 1.8.0 不认识，读出来显示为 `undefined`。T161 逐项断言 fuses 时要处理。
5. **ShellInfo 的 `flavor` 只有 `release | e2e | dev`**（T070），没有自用构建的 `local`。T076 填 `init()` 时要决定是映射成 release，还是给接口加 `local`。
6. **菜单的顶层标题和角色项（撤销、复制、退出等）是 Electron 自带的英文。**
   - 建议之后给 `@cupper/i18n` 加 `menu` 命名空间（T153 或 T154）。
   - 菜单项的分组是我暂定的，交 T153 按 Codex 定稿。
7. **留给 T076 的接线**：
   - prefs 还没有接到 ShellAPI（`init` 的快照、`set`、`onChange`）；
   - preload 现在是空模块；
   - 菜单命令走的通道是 `shell:command`。
8. **留给 T152**：
   - 窗口外观：菜单缩放后，交通灯的 y 不会按 `(44 × 缩放 − 14) / 2` 重新计算；
   - 没有设 vibrancy。
9. **e2e 的隔离有一处漏口**：Chromium 会往 `~/Library/Preferences/dev.cupper.app.e2e.plist` 写几项系统默认值，这不在 `CUPPER_E2E_DIR` 下面，内容与用户数据无关。T090 可以考虑每轮清理一次。
10. **本任务没有加的东西**：
    - 渲染进程和 preload 的 ESLint 安全规则（5.7.8，禁止 innerHTML 等）没有加，因为现在没有渲染进程的 TS 代码。建议由第一个写渲染代码的任务（T080 起）和 ui-core 一起加。
    - URL scheme（`cupper-e2e://` 等）没有注册，留给做深链接的任务和处理函数一起加。

详细记录见分支上的 `docs/tasks/T074-desktop-main-skeleton/notes.md`。