# R04 来源索引与原始摘录

访问日期均为 2026-10-16。编号规则：

- `L*`：本机 `/Applications` 下已安装 App 的 bundle 只读检查（拆解方法见 L0）；
- `A*`：编码 agent 客户端；
- `B*`：ACP 与多 agent 客户端；
- `C*`：可作参照的开发者桌面工具与框架迁移案例；
- `D*`：Rust 原生 UI 框架；
- `E*`：新一代 Web 外壳与其他跨平台框架；
- `X*`：横向资料（类型共享工具、语言趋势等）。

标注：

- "一手"指官方文档、官方博客、开源仓库文件、release 元数据、本机 bundle；"二手"指第三方文章、论坛帖子；"搜索摘要"指只拿到搜索结果摘要、没有打开原文。
- 外发请求都不带个人信息：curl 统一用 `-A cupper-research`；crates.io 等要求 User-Agent 的 API 也用这个占位写法。仓库都克隆到 worktree 之外的临时目录，只读，没有执行、编译或安装依赖。没有下载或运行任何第三方安装包；需要安装包大小时只看 release API 的 `size` 字段或 HEAD 请求的 `Content-Length`。

---

## L：本机 bundle 检查

### L0 拆解方法

只读查看 `/Applications/<App>.app`，没有运行任何 App，没有读取任何用户数据目录。用到的命令：

- `ls Contents/Frameworks`、`ls Contents/Resources`、`du -sh`：看是否带 `Electron Framework.framework`、Helper 进程、资源体积（下文的 MB、GB 都是 `du -sh` 的输出，按 1024 进制）；
- `plutil -p Contents/Info.plist`：`CFBundleIdentifier`、版本、`LSMinimumSystemVersion`、`NSPrincipalClass`（Electron 应用是 `AtomApplication`，Chromium 系是 `BrowserCrApplication`，AppKit 原生是 `NSApplication`）；
- `plutil -p "Electron Framework.framework/Resources/Info.plist"`：Electron 版本；
- `lipo -archs`：单架构还是通用二进制（通用二进制体积约翻倍）；
- `otool -L`：主可执行文件链接了哪些系统框架（Metal、WebKit、SwiftUI 等）；
- `grep -a -o` 在框架二进制里搜 `Chrome/x.y.z Electron/x.y.z` 这类版本串；
- 用一个只读的 Python 脚本解析 `app.asar` 的头部（JSON 目录），读出其中的 `package.json`（名称、版本、依赖名）和前端资源文件名。没有解包、没有执行其中任何代码。

App 的安装路径都是 `/Applications/<名称>.app`，下文省略。

### L1 ChatGPT.app（含 Codex 视图）

| 项目 | 观察结果 |
|---|---|
| 版本 | `CFBundleShortVersionString` 26.1007.21159，`CFBundleVersion` 20052；`CFBundleIdentifier` = `com.openai.codex`；`CFBundleName` = ChatGPT |
| 系统要求 | `LSMinimumSystemVersion` 13.0；`DTSDKName` macosx15.5 |
| 主类 | `NSPrincipalClass` = `BrowserCrApplication`（Chromium 浏览器进程的类名，不是 Electron 的 `AtomApplication`） |
| Frameworks | 只有 `Codex Framework.framework` 和 `Sparkle.framework`，**没有** `Electron Framework.framework` |
| Codex Framework | `CFBundleIdentifier` = `com.openai.codex.framework`，版本 155.0.8059.40；主二进制 272,897,824 字节，arm64 单架构；二进制中有版本串 `Chrome/155.0.8059.40`，以及 `OwlElectronApplicationMenuController`、`OwlElectronMenuCommandTarget`、`OwlElectronDockMenuController`、`OwlElectronTrayMenuController`、`OwlElectronProgressBar`、`owl_electron_permission_profile_data` 等类名或符号 |
| Helpers | `Codex (GPU).app`、`Codex (Renderer).app`、`Codex (Service).app`、`Codex (Alerts).app`、`Codex (Aperitif).app` 及其 GPU/Renderer/Alerts 变体（`com.openai.codex.helper`，`LSUIElement`）；`browser_crashpad_handler`、`app_mode_loader`、`web_app_shortcut_copier`（都是 Chromium 浏览器自带的辅助程序名） |
| OWL 配置 | `Resources/owl-app.ini`：`[Owl] UserDataDirectoryName=Codex`、`AppVersion=26.1007.21159`。`Resources/owl-electron-app.json`：`"runtimeName": "owl"`，`"packagedFrom"` 指向 CI 机器上的 `.../codex/codex-apps/electron/out/ChatGPT-darwin-arm64/ChatGPT.app` |
| asar 的 package.json | `name` = `openai-codex-electron`，`productName` = Codex，`main` = `.vite/build/early-bootstrap.js`。依赖含 `@tanstack/query-core`、`better-sqlite3`、`node-pty`、`capnweb`、`yjs`、`zod`、`vscode-jsonrpc`、`objc-js`、`@sentry/electron`、`app-server-manager`、`app-server-types`、`git-worker` 等；devDependencies 含 `"electron": "42.3.0"`、`@electron-forge/*` 7.11.2、`@electron/fuses`、`@electron/notarize`、`vite` 8.2.0、`vitest` 4.1.10、`playwright` 1.61.1、`typescript` 7.0.2 |
| 前端资源（asar 内 `webview/assets`，共 19,442 个文件） | 文件名中可见：`mermaid.core`、`mermaid-parser.core`、`mermaid-diagram`、`chatgpt-mermaid-preview-actions`；`pierre-light`、`pierre-dark` 等多套主题、`register-pierre-themes`、`pierre-file-editor`；`shiki-highlight-provider`；`xterm-output-panel`、`xterm-display-helpers`；`chatgpt-markdown-view`、`conversation-markdown`；`code-diff`、`diff-comment-card`、`editor-diff-page`；KaTeX 字体 |
| 原生扩展 | `Resources/native/` 下有 `sparkle.node`、`remote-control-device-key.node`、`devicecheck.node`、`input-monitoring-permission.node`、`usb_webauthn.node` 等；`PlugIns/CodexDockTilePlugin.docktileplugin` |
| 更新 | Info.plist 有 `SUPublicEDKey`，并带 `Sparkle.framework`，说明走 Sparkle 而不是 Electron 自带的 Squirrel.Mac |
| 体积 | 安装后 1.6 GB；其中 Frameworks 367 MB、`app.asar` 541 MB、`Resources/codex-cli` 322 MB |

解读（推测）：Codex 的应用代码是按 Electron API 写的 Electron 应用（Forge 打包、开发依赖 Electron 42.3.0），但发布时没有带 Electron 运行时，而是装进 OpenAI 自己基于 Chromium 155 的 "OWL" 运行时，由它提供一层 Electron 兼容 API（`OwlElectron*` 类）。外部证据见 A 组。

### L2 Claude.app

| 项目 | 观察结果 |
|---|---|
| 版本 | 1.46388.4；`com.anthropic.claudefordesktop`；`LSMinimumSystemVersion` 12.0；`NSPrincipalClass` = `AtomApplication` |
| 运行时 | `Electron Framework.framework` 42.10.0，二进制中版本串 `Chrome/148.0.7778.280 Electron/42.10.0`；通用二进制（x86_64 + arm64），Frameworks 491 MB；带 `Squirrel.framework`、`Mantle.framework`、`ReactiveObjC.framework`（Electron autoUpdater 的依赖） |
| asar 的 package.json | `name` = `@ant/desktop`，`main` = `.vite/build/index.pre.js`；依赖含 `@ant/claude-native`、`@ant/claude-swift`、`node-pty`（可选）；devDependencies 含 `@electron-forge/*` 8.0.0-alpha.10、`@electron-forge/plugin-fuses`、`@electron/fuses`、`@electron/notarize`、`@formatjs/intl`、`@tailwindcss/postcss`、`@vitejs/plugin-react`、`@anthropic-ai/claude-agent-sdk` 0.3.260 |
| 前端 | `Resources/ion-dist`（164 MB，2,945 个文件），vendor 分块名有 `vendor-base-ui`、`vendor-tiptap`、`vendor-prosemirror`、`vendor-markdown`、`vendor-intl`、`vendor-query`、`vendor-router`、`vendor-motion`、`vendor-zod` |
| 其他 | `app.asar.unpacked/node_modules/@ant/claude-swift`（原生 Swift 模块）；`Helpers/` 下有 `chrome-native-host`、`app-cu-helper` 等 |
| 体积 | 安装后 833 MB（通用二进制） |

### L3 Cursor.app

- 3.19.13；`com.todesktop.230313mzl4w4u92`（用 ToDesktop 打包分发）；`LSMinimumSystemVersion` 12.0；`AtomApplication`。
- `Resources/app/product.json`：`vscodeVersion` = 1.128.0，`date` = 2026-09-04。
- Electron 42.10.0，arm64；`Electron Framework` 二进制 190,963,632 字节；Frameworks 275 MB；安装后 871 MB。

### L4 Visual Studio Code.app

- 1.137.0；`com.microsoft.VSCode`；`LSMinimumSystemVersion` 12.0；`DTSDKName` macosx26.5；`AtomApplication`。
- Electron 42.10.0，通用二进制；Frameworks 492 MB；安装后 1.4 GB。

### L5 Warp.app

- 0.2026.09.16.08.27.02；`dev.warp.Warp-Stable`。
- `Contents/MacOS/stable` 是单个 413,742,736 字节的 arm64 可执行文件；Frameworks 里只有 `Sentry.framework`；没有 Electron，也没有链接 WebKit。
- `otool -L`：链接 `Metal`、`MetalKit`、`QuartzCore`、`CoreText`、`AppKit`。
- 安装后 451 MB。

### L6 Paseo.app

- 0.2.3；`sh.paseo.desktop`；`LSMinimumSystemVersion` 12.0；Electron 41.2.0，arm64；Frameworks 263 MB；安装后 404 MB。
- asar：`@getpaseo/desktop`，依赖 `@getpaseo/server`、`@getpaseo/cli`、`electron-updater`；`node_modules` 中有 `@agentclientprotocol`、`@xterm`、`@codemirror`、`@opencode-ai`、`node-pty`。
- 前端在 `Resources/app-dist`（15 MB），是 Expo 的 Web 构建产物（`_expo/static`、`expo-router`、`@react-navigation`），即 React Native for Web。

### L7 Lody.app

- 0.105.0；`ai.lody.desktop`；`LSMinimumSystemVersion` 13.0；Electron 44.7.0，arm64；Frameworks 242 MB（同时带 `Sparkle.framework` 和 `Squirrel.framework`）；安装后 461 MB。
- asar：`@lody/electron`，依赖 `react`、`@tanstack/react-router`、`jotai`、`electron-sparkle-updater`、`electron-updater`、`convex`；`node_modules` 中有 `@agentclientprotocol`、`acp-extension-core`。

### L8 体积参照：Tauri、Wails、原生 + WebView

| App | 框架（依据） | 观察 |
|---|---|---|
| CC Switch 4.0.5（`com.ccswitch.desktop`） | Tauri 2（仓库 `farion1231/cc-switch` 的 `src-tauri/Cargo.toml`：`tauri = 2.8.2`） | 单个 75,830,320 字节的通用二进制，前端打包在二进制里；`otool -L` 链接系统 `WebKit`；安装后 73 MB |
| Tiny RDM 1.2.7（`com.tinycraft.tinyrdm`） | Wails v2（仓库 `tiny-craft/tiny-rdm` 的 `go.mod`：`wailsapp/wails/v2 v2.13.0`） | 单个 27,113,472 字节的 arm64 可执行文件，链接系统 `WebKit`；安装后 26 MB |
| Otty 1.4.1（`io.appmakes.otty`） | 原生（`NSPrincipalClass` = `NSApplication`） | `otool -L` 链接 `SwiftUI`、`AppKit`、`Metal`、`WebKit`；`Resources` 里有 `Grid.metallib`（终端网格用 Metal 绘制）和 `settings-ui.html`（设置页用 WebView）；`LSMinimumSystemVersion` 14.0；安装后 54 MB。面向跑编码 agent 的终端（第三方目录站描述，二手） |

### L9 Electron 运行时自身的体积（安装后、arm64）

| App | Electron 版本 | `Electron Framework` 主二进制 | Frameworks 目录 |
|---|---|---|---|
| Paseo | 41.2.0 | 178,909,088 字节 | 263 MB |
| Cursor | 42.10.0 | 190,963,632 字节 | 275 MB |
| Lody | 44.7.0 | 201,521,216 字节 | 242 MB |
| Claude（通用二进制） | 42.10.0 | 399,744,928 字节 | 491 MB |
| ChatGPT（OWL，非 Electron） | Chromium 155 | 272,897,824 字节 | 367 MB |

结论：一个 arm64 的 Electron 应用，光运行时就占安装后约 240–275 MB；做成通用二进制约 490 MB。上一轮引用的 Electron 44.8.0 darwin-arm64 压缩包为 124 MB（desktop-engineer.md S1）。
