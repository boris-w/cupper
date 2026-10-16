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

---

## A：编码 agent 客户端

| 编号 | 标题 | URL | 资料日期 | 要点摘录 | 获取方式 |
|---|---|---|---|---|---|
| A1 | OpenAI：How we built OWL, the new architecture behind our ChatGPT-based browser, Atlas（Ken Rockot、Ben Goodger） | https://openai.com/index/building-chatgpt-atlas/ | 2025-10-30（日期来自搜索摘要） | "OWL is our integration of Chromium … running Chromium's browser process outside of the main Atlas app process"；"Atlas is built almost entirely in SwiftUI and AppKit"；"Chromium boots asynchronously … pixels hit the screen nearly instantly"；"If its main thread hangs, Atlas doesn't"；通过 Mojo 通信，"custom Swift (and even TypeScript) bindings for Mojo"；"OWL ships internally as a prebuilt binary" | 原站 403；读 Wayback 快照（2026-09-17）（一手） |
| A2 | OpenAI 开发者论坛帖（新 ChatGPT macOS App 26.707.x 缺少 Chat 模式） | https://community.openai.com/t/…/1386324 | 2026-07-10 至 07-14 | 用户贴出关于页 "Version 26.707.72221 (Powered by Codex & OWL)"（2026-07-14） | WebFetch（二手，用户转述） |
| A3 | OpenAI 招聘：Software Engineer, Web Layer（Codex Web Layer 团队） | https://jobs.thrivecap.com/companies/openai/jobs/82487109-software-engineer-web-layer（openai.com/careers 原页 403） | 2026-06-12 | 团队负责 "from the Electron-like application framework that powers the application" 到 "the user-facing in-app browser"；"backend C++ layer and Chromium" | WebFetch（招聘镜像站，内容为官方招聘文案） |
| A4 | mkreminskii/codex-web README | https://github.com/mkreminskii/codex-web | 2026 | 用 "stock Electron 42.3.0" 跑官方 26.1007.21159（"the official October 8, 2026 download"）；"Native Owl shell features that are unavailable in stock Electron are disabled" | WebFetch（第三方） |
| A5 | ilysenko/codex-desktop-linux README | https://github.com/ilysenko/codex-desktop-linux | 2026 | 从官方 Linux .deb 复用 "The official Electron runtime, native modules … and Owl metadata" | WebFetch（第三方） |
| A6 | Michael Tsai：ChatGPT Work and ChatGPT Classic | https://mjtsai.com/blog/2026/07/10/chatgpt-work-and-chatgpt-classic/ | 2026-07-10 | Troughton-Smith："replacing the native AppKit ChatGPT app with the Electron Codex app"；Tony Arnold："a slow, RAM hungry shell"；也有人说 "the non-native version feels much snappier"；新版支持 Intel Mac | WebFetch（二手汇总） |
| A7 | TidBITS：OpenAI shuffles ChatGPT apps, kills Atlas browser | https://tidbits.com/2026/07/13/openai-shuffles-chatgpt-apps-kills-atlas-browser-improves-voice/ | 2026-07-13 | 新 App "1.45 GB"；"the Codex app is 723.7 MB"；Classic 是 "a 159 MB native Mac app"；体积变大 "probably due to ChatGPT Work incorporating code from ChatGPT Atlas" | WebFetch（二手） |
| A8 | 9to5Mac：OpenAI is discontinuing ChatGPT Atlas | https://9to5mac.com/2026/07/09/openai-is-discontinuing-chatgpt-atlas-its-standalone-desktop-browser/ | 2026-07-09 | "we are going to be sunsetting Atlas"；"targeted date for deprecation is 8/9" | WebFetch（二手） |
| A9 | Spyglass：The ChatGPT "Super App" Sort of Super Sucks | https://spyglass.org/chatgpt-gets-to-work/ | 2026-07-10 | "a bloated Electron package"；"I understand the cross-development and deployment trade-offs" | WebFetch（二手） |
| A10 | OpenAI 员工在 X 上的帖子 | https://x.com/Javi/status/1790074965112328538 | 2024-05 | "ChatGPT for Mac is a fully native macOS app. Not Electron. Not even Catalyst." | 搜索摘要 |
| A11 | OpenAI 下载链接 HEAD | `persistent.oaistatic.com/codex-app-prod/Codex.dmg`；`persistent.oaistatic.com/sidekick/public/ChatGPT.dmg` | — | Codex.dmg 787,927,533 字节（Last-Modified 2026-10-15）；sidekick 的 ChatGPT.dmg 78,575,566 字节（2026-07-09，是否为最后一版 Classic 未核实） | curl -I |
| A12 | HN 评论 bcherny（父帖 "Why is Claude an Electron app?"） | https://news.ycombinator.com/item?id=47106368 | 2026-02-21 | "Some of the engineers … worked on Electron … preferred building non-natively. … share code … same look and feel. Finally, Claude is great at it."；"may change in the future" | HN Firebase API（匿名）（一手发言） |
| A13 | Drew Breunig：Why is Claude an Electron app? | https://www.dbreunig.com/2026/02/21/why-is-claude-an-electron-app.html | 2026-02-21 | coding agent 降低了原生开发门槛，但 "the last mile of dev and the support surface area remains a real concern"；"For now, Electron still makes sense" | WebFetch（二手） |
| A14 | Daring Fireball：Claude's Criminally Bad Electron Mac App Is an Inside Job | https://daringfireball.net/2026/07/claudes_criminally_bad_mac_app_is_an_inside_job | 2026-07-03 | Felix Rieseberg 是 "one of the principal people responsible for creating Electron"，负责 Claude 的 Mac 和 Windows App | WebFetch（二手） |
| A15 | anthropics/claude-code 桌面内存 issue（#40633、#77560 等，经镜像站） | https://claudeissues.com/issue/40633-… | 2026-03 至 07 | Windows 空闲 9 个进程共 1.18 GB；M3 上一个 Claude Code 会话约 2.4 GB、CPU 约 70%；18 小时测试稳定在约 2,066 MB | 搜索摘要 |
| A16 | Claude 桌面更新源 | https://downloads.claude.ai/releases/darwin/universal/RELEASES.json | 2026-10-15 | currentRelease 2.42304.0；universal zip 390,094,688 字节（与本机 1.46388.4 版本线不同，原因不明） | curl + HEAD |
| A17 | Cursor 下载 HEAD | `api2.cursor.sh/updates/download/golden/darwin-arm64/cursor/3.19` → `downloads.cursor.com/…/Cursor-darwin-arm64.dmg` | 2026-09-08 | 279,493,837 字节 | curl -I |
| A18 | Cursor 论坛：Why is Cursor not an Extension to VS-Code? | https://forum.cursor.com/t/why-is-cursor-not-an-extension-to-vs-code/2523 | 2024-01-25 | "VSCode extensions have very limited control over the UI of the editor"（回答者身份未确认） | WebFetch（二手） |
| A19 | Cursor 论坛内存帖（168692、168061 等） | https://forum.cursor.com/t/memory-cpu-leak-in-cursor-agents-window-renderer-after-long-running-sessions-multi-day/168692 | 2026 年中（3.5–3.18 版） | Agents 窗口跑 1–2 天后 2.4 GB、CPU 636%；官方人员称 Glass 布局长会话内存增长是 "known issue" | 搜索摘要 |
| A20 | Devin Desktop FAQ | https://docs.devin.ai/desktop/devin-desktop-faq | 2026-06 | "Devin Desktop is the new name for Windsurf"，2026-06-02 生效；提到 "VS Code-derived user data directory"、"Chromium/Electron disk and GPU caches"；windsurf.com 308 跳到 devin.ai | WebFetch + HEAD（一手） |
| A21 | VS Code 下载 HEAD | `update.code.visualstudio.com/latest/darwin-arm64/stable` | 2026-10-15 | `VSCode-darwin-arm64.zip` 316,862,344 字节 | curl -I |
| A22 | VS Code：Migrating VS Code to Process Sandboxing | https://code.visualstudio.com/blogs/2022/11/28/vscode-sandbox | 2022-11-28 | "started in early 2020"，计划 2023 年初完成；为此给 Electron 贡献了 utility process API，引入 MessagePort | WebFetch（一手） |
| A23 | VS Code 起源（TheNextWeb 2015-04-30；The Register 2021 年对 Erich Gamma 的访谈） | — | 2015 / 2021 | 由浏览器编辑器 Monaco 发展到 Electron 桌面版 | 搜索摘要 |
| A24 | Zed：Leveraging Rust and the GPU to render user interfaces at 120 FPS（Antonio Scandurra） | https://zed.dev/blog/videogame | 2023-03-07 | "8.33ms per frame"；"only way … was to build our own UI framework: GPUI"；只画 "rectangles, shadows, text, icons, and images"；用 Metal | WebFetch（一手） |
| A25 | Zed：We Have to Start Over: From Atom to Zed | https://zed.dev/blog/we-have-to-start-over | 2024-02-12 | "we created Electron to create Atom, but I can't imagine a worse application for Electron than a code editor"；最初设想 Rust 核心 + Electron 展示层，后改为全 GPU | WebFetch（一手） |
| A26 | Zed：Zed is 1.0（Nathan Sobo） | https://zed.dev/blog/zed-1-0 | 2026-04-29 | "Web technology offered an easy path … but it also imposed a ceiling"；"writing our own UI framework, GPUI, from scratch in Rust" | WebFetch（一手） |
| A27 | Zed 下载 HEAD | `zed.dev/api/releases/stable/latest/Zed-aarch64.dmg` | 2026-10-14 | v1.24.2，118,780,925 字节 | curl -I |
| A28 | Warp：How Warp Works（Aloke Desai） | https://www.warp.dev/blog/how-warp-works | 2021-07-12 | 试过 Electron；"lack of a stable UI framework for Rust"；"chose Metal over OpenGL"；只画 "rectangles, images, and glyphs"；"loosely inspired by Flutter" | WebFetch（一手） |
| A29 | Warp is now open-source；warpdotdev/warp README | https://www.warp.dev/blog/warp-is-now-open-source ；https://github.com/warpdotdev/warp | 2026-04-28 | README："Warp's UI framework (the warpui_core and warpui crates) are licensed under the MIT license"，其余 AGPL v3 | 博文为搜索摘要；README 用 curl（一手） |
| A30 | JetBrains：The Future of Fleet | https://blog.jetbrains.com/fleet/2025/12/the-future-of-fleet/ | 2025-12 | "Fleet has evolved into Air"；"Starting December 22, 2025, Fleet will no longer be available"；"Based on the Fleet platform … The technology, team, and long-term direction continue" | 原站 403；Wayback 快照（2026-08-12）（一手） |
| A31 | Fleet Below Deck, Part VI – UI With Noria | https://blog.jetbrains.com/fleet/2023/02/fleet-below-deck-part-vi-ui-with-noria/ | 2023-02 | 只用 AWT 拿窗口，绘制用 Skia；"When we started working on Fleet, however, Jetpack Compose didn't exist, and thus Noria was born" | Wayback（一手） |
| A32 | JetBrains 发布服务 API（code=AIR） | https://data.services.jetbrains.com/products/releases?code=AIR&latest=true&type=release,preview,eap | 2026-10-08 | 262.1037.6 preview；macOS aarch64 dmg 297,841,397 字节，x64 359,685,550 字节 | curl（一手） |
| A33 | Air 第三方库清单 | https://resources.jetbrains.com/storage/third-party-libraries/air/air-262.1037.6-third-party-libraries.json | 2026-10-08 | 共 560 项：skiko-awt 0.150.1、kotlin-desktop-toolkit 0.0.90、JetBrains Runtime、Rust `alacritty_terminal`；没有 Compose、JCEF、Chromium | curl（一手） |
| A34 | GitHub：How Four Native Developers Wrote An Electron App | https://github.blog/engineering/architecture-optimization/how-four-native-developers-wrote-an-electron-app/ | 2017-05-16 | "building native apps for multiple platforms doesn't scale"；"feedback cycle went from minutes to seconds"；"Rewriting on Electron does mean swapping one set of tradeoffs for another" | WebFetch（一手） |
| A35 | GitHub：Everyone's moving to GitHub Desktop | https://github.blog/2018-05-02-everyones-moving-to-github-desktop/ | 2018-05-02 | 1.0 发布不到半年，Electron 版用户超过两个旧版之和 | 搜索摘要 |
| A36 | GitHub Desktop 下载 HEAD | `central.github.com/deployments/desktop/desktop/latest/darwin-arm64` | 2026-10-08 | 3.6.7 `GitHubDesktop-arm64.zip` 481,763,140 字节 | curl -I |
| A37 | GitHub Changelog：GitHub Copilot app generally available | https://github.blog/changelog/2026-06-17-github-copilot-app-generally-available | 2026-06-17 | macOS、Windows、Linux 都 GA；未说明框架 | WebFetch（一手） |
| A38 | github/app README 与 dmg HEAD | https://github.com/github/app | 2026-10-13 | v1.1.29 `GitHub-Copilot-darwin-arm64.dmg` 351,801,627 字节 | curl |
| A39 | github/app issues #3822、#4586、#4137、#450、#4636 | https://github.com/github/app/issues/3822 | 2026-05 至 10 | #3822："version 1.0.84-4 (bundled Tauri + WebKitGTK, libwebkit2gtk-4.1)"；Linux 上 WebKitGTK 不重绘、输入法预编辑文字不可见、关闭 DMA-BUF 后 CPU 打满；#450："smooth … better than any electron app … takes very less ram"；#4636："Electron slop vibes" | curl 匿名 HTML（二手，用户报告） |

### A 组未能核实或有矛盾的地方

- OWL：没有官方说明讲 ChatGPT 桌面 App 改跑 OWL，证据是本机包结构（L1）、用户转述的关于页（A2）、招聘文案（A3）和第三方移植项目（A4、A5）。26.519（约 2026-05）还是 Electron 42.1.0（R01 T2），26.707.72221（2026-07-14）已经是 OWL，中间哪个版本切换不清楚。A5 说 Linux 版 .deb 里仍是 Electron 运行时，未核实。
- Claude：更新源的版本（2.42304.0）与本机（1.46388.4）对不上；内存数字只来自 issue 的搜索摘要；没有工程博客说明选型。
- GitHub Copilot app 用 Tauri 只有一个用户拆 Linux 包的报告（A39），官方没说，macOS 包未核实。
- Windsurf / Devin Desktop 是 Electron 只能从 FAQ 提到的缓存目录推断。
- Warp 的 dmg 大小没拿到；Warp、Zed 没有官方内存数字。
- JetBrains Air 用 Noria 只是推测（第一方组件不进第三方库清单）。

---

## C：可作参照的开发者桌面工具与框架迁移案例

说明：标"三方扫描"的数据来自 desktopinsights.com。该站用 package.json、bundle 字符串、otool 推断框架和版本，每项带置信度，属于第三方拆解，只作旁证。"dmg/pkg"大小来自官方下载链接 HEAD 请求的 `Content-Length`；"装后"是三方扫描的安装后大小。

| 编号 | 标题 | URL | 资料日期 | 要点摘录 | 获取方式 |
|---|---|---|---|---|---|
| C1 | 1Password 8: The Story So Far（Michael Fey） | https://1password.com/blog/1password-8-the-story-so-far | 2021-08-12 | Rust "1Password Core" 负责 UI 之下的全部逻辑；Linux/macOS 用 Electron，iOS 用 SwiftUI，Android 用 View；"made the painful decision to stop work on the SwiftUI Mac app"；"still found ourselves building separate implementations of certain components"；AppKit 会"significantly behind schedule"；团队从按平台分组改为按功能分组 | WebFetch（一手） |
| C2 | Welcoming Linux to the 1Password family（Dave Teare） | https://1password.com/blog/welcoming-linux-to-the-1password-family | 2021-05-18 | "the user interface (UI) is written in React with Neon bindings to the Rust backend"；加密用 ring；开源 electron-hardener | WebFetch（一手） |
| C3 | Typeshare for Rust；github.com/1password/typeshare | https://1password.com/blog/typeshare-for-rust | 2022-11-22 | Rust 核心通过 FFI 与各前端通信；typeshare 基于 serde/syn 从 Rust 生成 TS、Swift、Kotlin 等类型 | WebFetch（一手）+ 搜索摘要 |
| C4 | 1Password 社区员工回复 | https://1password.com/community/discussion/123298/electron-version-question | 2021 | "most of our app is not in the frontend runtime"；"will probably use more system resources than 1Password 7" | 搜索摘要（原帖 404） |
| C5 | Macworld：New version of 1Password for Mac is flooded with criticism | https://www.macworld.com/article/677513/new-version-of-1password-for-mac-is-flooded-with-criticism.html | 2021-08-13 | "I pay for 1Password to get a native macOS experience"；"Overall performance feels sluggish" | WebFetch（二手） |
| C6 | desktopinsights：1Password | https://desktopinsights.com/apps/1password | 约 2026-10 | Electron 42.11.1；8.12.40（2026-10-05）；装后 913 MB（arm64）；检出 ring、tokio、serde 等 Rust crate | WebFetch（三方扫描） |
| C7 | 1Password 官方下载 HEAD | https://downloads.1password.com/mac/1Password.pkg | Last-Modified 2026-10-05 | pkg 398,077,993 字节（universal） | curl -I |
| C8 | Raycast：A Technical Deep Dive Into the New Raycast | https://www.raycast.com/blog/a-technical-deep-dive-into-the-new-raycast | 2026-05-14 | 新架构：macOS 用 Swift/AppKit 外壳 + WKWebView，Windows 用 C#/.NET/WPF + WebView2，共用 React+TS 前端；常驻 Node 后端；Rust 负责文件索引、数据层（可与 iOS 共享）和同步。评估过 Electron、Tauri、自研三条路：不想在 macOS 上捆绑 Chromium，Tauri "less control on the native side…still young"；"two separate UI stacks would mean twice the work"；"AppKit was getting in our way"；SwiftUI 在 v1 "never quite cleared our bar for performance and control"。"interfaces are declared in one place and typed clients are generated for every side"。内存：v1 约 200–300 MB，v2 约 350–450 MB；主窗口隐藏时 WebContent 120–200 MB、Node 150–200 MB、Swift 外壳约 40 MB、GPU 进程约 18 MB、网络进程约 12 MB；空 WebView 基线约 50 MB，空 Node 约 12 MB。列出的 WKWebView 问题：隐藏时被节流（用 alpha=0 加关闭 occlusion 检测绕过）、闪烁、输入法、无障碍、拖放。采用 Liquid Glass。"Apps like VS Code, Linear, or Superhuman prove you can build excellent products with it"（指 Electron） | curl 全文 + WebFetch（一手） |
| C9 | Raycast Windows Changelog v2.0 | https://www.raycast.com/changelog/windows/2-0 | 2026-08-25 | "Raycast 2.0 is out of Beta!" | curl（一手） |
| C10 | Raycast 下载 HEAD | https://www.raycast.com/download | Last-Modified 2026-10-13 | `Raycast_2.8.0.0_…_arm64.dmg` 145,342,749 字节 | curl -I |
| C11 | desktopinsights：Raycast | https://desktopinsights.com/apps/raycast | 2026-10 | Swift + React + Rust crate + napi；装后 234 MB | WebFetch（三方扫描） |
| C12 | MacG：Raycast 2 … fâche les puristes | https://www.macg.co/logiciels/2026/08/raycast-2-une-mise-jour-majeure-qui-fait-le-plein-dia-et-fache-les-puristes-310469 | 2026-08-20 | "une architecture hybride, intégrant des vues web et du React"；用户担心流畅度和内存 | WebFetch（二手） |
| C13 | Ghostty README | https://github.com/ghostty-org/ghostty | main 分支，2026-10-16 快照 | "The macOS app is a true SwiftUI-based application"；"true Metal renderer with CoreText"；Linux 版用 GTK；libghostty-vt 支持 macOS/Linux/Windows/WebAssembly，API 尚未定版 | curl raw（一手） |
| C14 | About Ghostty | https://ghostty.org/docs/about | 无日期 | macOS："Swift and uses AppKit and SwiftUI"；Linux："Zig and uses the GTK4 C API" | WebFetch（一手） |
| C15 | Mitchell Hashimoto：Ghostty is coming | https://mitchellh.com/writing/ghostty-is-coming | 2024-10-22 | 现有终端"pushed an unnecessary choice between speed, features, and platform-native GUIs" | WebFetch（一手） |
| C16 | Ghostty 下载 HEAD；desktopinsights | https://release.files.ghostty.org/1.3.1/Ghostty.dmg | 1.3.1，Last-Modified 2026-03-13 | dmg 33,808,867 字节（universal）；最低 macOS 13；装后 64 MB，Sparkle（三方） | curl -I + WebFetch |
| C17 | coder/ghostty-web | https://github.com/coder/ghostty-web | 2026 | 把 Ghostty 的 VT 解析器编译成 WASM，API 与 xterm.js 兼容，MIT | 搜索摘要 |
| C18 | Letter to Arc members 2025 | https://browsercompany.substack.com/p/letter-to-arc-members-2025 | 2025-05-26 | "sunsetting our use of TCA and SwiftUI to make Dia lightweight, snappy, and responsive"；"Arc was bloated"；ADK（Arc Development Kit）是 Dia 的基础 | WebFetch（一手） |
| C19 | Fatbobman's Swift Weekly #86 | https://fatbobman.com/en/weekly/issue-086/ | 2025-06-02 | TCA 作者澄清 Arc 用的是早期定制 TCA 分支 | WebFetch（二手） |
| C20 | Speaking in Swift：Swift, meet WinRT | https://speakinginswift.substack.com/p/swift-meet-winrt | 2023-10-30 | WinUI 3 是"what we are using to build Arc on Windows" | WebFetch（一手） |
| C21 | Atlassian 完成收购 The Browser Company（Business Wire） | https://www.businesswire.com/news/home/20251021473486/en | 2025-10-21 | 收购完成；6.1 亿美元来自 heise 报道 | 搜索摘要 |
| C22 | Arc / Dia 下载 HEAD；desktopinsights | https://releases.arc.net/release/Arc-latest.dmg ；https://releases.diabrowser.com/release/Dia-latest.dmg | Last-Modified 2026-10-14 / 2026-10-16 | Arc 1.169.0 dmg 454,702,040 字节，装后 1338 MB；Dia dmg 853,592,727 字节，装后 1584 MB；均为 Swift + Sparkle（三方） | curl -I + WebFetch |
| C23 | diabrowser.com 首页 | https://www.diabrowser.com/ | 2026-10-16 | "Available for macOS and Windows" | WebFetch（一手） |
| C24 | Wikipedia Dia；The Browser Company 招聘帖 | https://en.wikipedia.org/wiki/Dia_(web_browser) | — | 把 Chromium 打包成 SDK，外层用 Swift；Swift 代码在 macOS/Windows/iOS/Android 间共享 | 搜索摘要 |
| C25 | Reactiflux Q&A：Conrad Irwin（Superhuman CTO） | https://reactiflux.js.org/transcripts/conrad-irwin | 2019-06-07 | "We actually started with node-webkit…ran into many issues"；"Electron had a much larger community" | WebFetch（一手访谈） |
| C26 | desktopinsights：Superhuman | https://desktopinsights.com/apps/superhuman | 2026-10 | Electron 41.10.6；装后 475 MB；electron-builder/updater；React | WebFetch（三方扫描） |
| C27 | Linear 下载 HEAD；desktopinsights | https://releases.linear.app/mac ；https://desktopinsights.com/apps/linear | Last-Modified 2026-09-04 | dmg 213,054,457 字节（universal）；Electron 41.3.0，装后 541 MB（三方，扫描版本可能过期） | curl -I + WebFetch |
| C28 | Linear：Scaling the Linear Sync Engine；第三方解读 | https://linear.app/blog/scaling-the-linear-sync-engine ；https://performance.dev/what-makes-linear-feel-instant | — | local-first、IndexedDB、乐观更新 | 搜索摘要 |
| C29 | desktopinsights：notion-calendar、notion-mail、notion；Notion 下载 HEAD | https://desktopinsights.com/apps/notion-calendar ；https://www.notion.com/desktop/mac/download | 2026-10 | Notion Calendar：Electron 41.5.0，装后 488 MB；Notion Mail：Electron 39.1.1，装后 425 MB（扫描较旧）；Notion：Electron 44.4.4，`Notion-7.39.0-universal.dmg` 233,350,224 字节 | WebFetch（三方）+ curl -I |
| C30 | Cron × Notion | https://www.cron.com/blog/2022-06-09-cron+notion | 2022-06-09 | Cron 被 Notion 收购；Cron 本来就是 Electron（bundle ID `com.cron.electron`，二手） | 搜索摘要 |
| C31 | Daring Fireball：ChatGPT Mac app（2026-07-09） | https://daringfireball.net/linked/2026/07/09/todays-the-day-openai-fucked-up-the-chatgpt-mac-app | 2026-07-09 | Classic 159 MB，新版"a svelte 1.5 GB Electron bundle" | WebFetch（二手） |
| C32 | Michael Tsai：ChatGPT Work and ChatGPT Classic | https://mjtsai.com/blog/?p=52579 | 2026-07-10 | 同 A6 | WebFetch（二手汇总） |
| C33 | Meta：Messenger Desktop … moving to React Native from Electron | https://developers.facebook.com/blog/post/2023/05/17/messenger-desktop-faster-and-smaller-by-moving-to-react-native-from-electron/ | 2023-05-17 | Electron 本体"up to 200 MB"；迁移后包体减少 100 MB 以上、加载时间减半、原生崩溃 macOS -15%、Windows -67%；代价是多窗口、自动更新（自己集成 Sparkle）、菜单、托盘、E2E 测试都要自建 | WebFetch（一手） |
| C34 | TechCrunch：Facebook Messenger's desktop app is no more | https://techcrunch.com/2025/12/15/facebook-messengers-desktop-app-is-no-more/ | 2025-12-15 | 桌面版关停，转网页 | 搜索摘要 |
| C35 | Microsoft Teams: Advantages of the new architecture；XDA | https://techcommunity.microsoft.com/t5/microsoft-teams-blog/microsoft-teams-advantages-of-the-new-architecture/ba-p/3775704 ；https://www.xda-developers.com/microsoft-teams-faster-more-efficient-march-2023/ | 2023-03 | "transitioning from Electron to leverage WebView2 as the host"；"two times faster and consume 50 percent less memory"（媒体转述，"up to"） | 官方原文被 WAF 拦截，用搜索摘要；XDA 用 WebFetch |
| C36 | WABetaInfo：WhatsApp deprecation of the Electron app for Mac | https://wabetainfo.com/whatsapp-announced-the-deprecation-of-the-electron-app-for-mac/ | 2024-09-04 | "the Catalyst app has reached a level where it provides a more robust experience" | WebFetch（二手） |
| C37 | Daring Fireball：WhatsApp for Windows 11 … Web App | https://daringfireball.net/2025/11/meta_whatsapp_windows_shitty_web_app | 2025-11-15 | "simply loads web.whatsapp.com in a WebView2 container" | WebFetch（二手） |
| C38 | Slack Engineering：When a rewrite isn't | https://slack.engineering/rebuilding-slack-on-the-desktop/ | 2019-07-22 | 在 Electron 内重写为单进程多工作区、React/Redux；"内存最多降 50%、加载快 33%"来自媒体转述 | WebFetch + 搜索摘要 |
| C39 | PCWorld：Discord auto-restart | https://www.pcworld.com/article/3005317/discord-admits-windows-11-app-hogs-ram-tries-solving-it-with-auto-restarts.html | 2025-12 | Windows 版内存超过 4 GB 时实验性自动重启 | 搜索摘要 |
| C40 | Allen Pike：Why is ChatGPT for Mac So Good? | https://allenpike.com/2025/why-is-chatgpt-so-good-claude | 2025-11-30 | "a website with window controls"；"Electron apps can be good apps!" | WebFetch（二手） |
| C41 | codenote.net：A 2026 Audit of Famous Electron Apps | https://codenote.net/en/posts/famous-electron-apps-2026-research/ | 2026-05-04 | 知名 Electron 应用汇总 | WebFetch（二手汇总，只作旁证） |

### C 组未能核实或有矛盾的地方

- Linear：官方从没说过用 Electron，结论只来自三方扫描（C27）；三方列出的版本（v1.30.2，2026-05）与官方 dmg 的 Last-Modified（2026-09-04）对不上。
- Teams Mac 版用不用 WebView2 没有核实；"2x / 50%"是媒体转述的"up to"数字。
- Dia：三方扫描没检出 Chromium，但官方信和招聘帖都说基于 Chromium/ADK；装后 1.5 GB 原因不明；弃用 SwiftUI/TCA 之后改用什么，官方没写。
- 1Password 员工的论坛原话只拿到搜索摘要。
- Superhuman 官方下载链接没找到，没有官方包体；Notion Calendar 下载接口返回 HTML 跳转页。
- 参照产品里只有 Raycast 公开了常驻内存的分项数字。
