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
- GitHub API 先用匿名访问；匿名额度很快用完（60 次/小时，可能是共享出口 IP），之后改用本机已登录的 `gh api` 做只读 GET。请求本身没有附带邮箱、姓名等内容，但认证头会让 GitHub 知道是哪个账号在查询，这一点请编排者知悉。
- 调研分五路并行（A、B、C、D、E 各一个分支调研员），L 和 X 由主执行者完成，最后统一整理到本文件。

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

### L9 Delta.app（Zed Industries 的 agent 协作客户端）

- 0.1.0；`com.zed-industries.delta`；`LSMinimumSystemVersion` 13.0；没有 `Frameworks` 目录。
- `Contents/MacOS/delta` 是单个 197,353,600 字节的 arm64 可执行文件；`Contents/Helpers/delta-pty-helper`（52 KB）；安装后 190 MB。
- `otool -L`：链接 `Metal`、`QuartzCore`、`CoreText`、`AppKit`，没有 WebKit。
- 二进制中的源码路径和 crate 名（`grep -a -o`）：`crates/gpui`、`crates/gpui_macos`、`crates/gpui_apple`、`crates/gpui_tokio`；应用自己的 crate 有 `thread_view`、`markdown`、`diff_view`、`diff_store`、`comment_set`、`inline_terminal`、`worktree_mount`、`local_turns`、`text_input` 等；第三方 crate 有 `agent-client-protocol-2.0.0`、`merman-0.12.3`（纯 Rust 的 Mermaid 渲染）、`pulldown-cmark-0.13.4`、`accesskit_macos-0.26.2`。
- 结论：Zed 公司的 agent 客户端是 GPUI 原生界面，通过 ACP 接 agent，Markdown、diff 评论、内嵌终端、Mermaid 都是 Rust 实现。官网说明见 D43。

### L10 Electron 运行时自身的体积（安装后、arm64）

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

## B：ACP 与多 agent 客户端

7 个仓库浅克隆到临时目录（只读，未安装依赖、未执行）。包体取自 GitHub Release API 的 `size` 字段，没有下载安装包。匿名 GitHub API 很快被限流，之后改用 `gh api` 只读 GET。

| 编号 | 标题 | URL | 版本 / 提交 / 日期 | 要点摘录 | 获取方式 |
|---|---|---|---|---|---|
| B1 | Conductor 创始人 Charlie Holtz 推文（镜像） | https://threadreaderapp.com/thread/1945870105109246401.html | 2025-07-17（由推文 ID 解码） | "Conductor is built in Tauri. It has a Rust backend and uses the native Mac renderer." | WebFetch（二手镜像，引文逐字） |
| B2 | desktopinsights：Conductor | https://desktopinsights.com/apps/conductor | v0.91.0，2026-10-15 | 装后 115 MB；Tauri（置信度 80/100，依据 crate：tauri、muda、window-vibrancy、tokio、sqlx）；有 allow-jit、unsigned-executable-memory entitlement | WebFetch（三方扫描） |
| B3 | conductor.build 首页、changelog、blog | https://www.conductor.build/changelog | 0.91.0，2026-10-15 | 不提技术栈；changelog 多次出现"Performance Improvements"、两次"Terminal Performance"，0.36.3 修过 SDK 内存泄漏 | WebFetch（一手，只有标题） |
| B4 | Dimillian/CodexMonitor：package.json、`src-tauri/Cargo.toml`、`tauri.conf.json`、README | https://github.com/Dimillian/CodexMonitor | HEAD `dd61b9a`（2026-03-26）；v0.7.67（2026-03-24）；4.3k star | Tauri 2.10 + React 19；@pierre/diffs ^1.0.6、TanStack Virtual、xterm 5.5、react-markdown + prismjs、tauri-plugin-liquid-glass；Overlay 标题栏、透明窗口、开启 macOSPrivateApi；dmg 25.5 MB；iOS 版走 Tauri mobile，WIP；仓库 2026-03-26 后无推送 | 浅克隆 + gh api（一手） |
| B5 | CodexMonitor issue #105、#393、#394、#143、#158 | https://github.com/Dimillian/CodexMonitor/issues/393 | 2026-01～02 | #393：WebContent 进程 1–1.7 GB、CPU 60–75%，点击卡顿 0.5–2 s，根因是 React useEffect 依赖不稳导致订阅死循环和轮询（#394）；#105：用户报 4.3 GB；#143/#158：修复拼音组字时 Enter 误发送 | gh api（一手） |
| B6 | OpenSource03/harnss：package.json、`electron/src/main.ts`、`lib/glass.ts`、README；issue #9、#18 | https://github.com/OpenSource03/harnss | HEAD `dc1dfd8`（2026-06-19）；v0.21.5（2026-03-19）；386 star | Electron ^40.4 + electron-builder 26；React 19 + shadcn/radix + Tailwind 4；react-markdown、monaco、mermaid 11、xterm 6、jsdiff、TanStack Virtual；dmg 321 MB；Liquid Glass 用第三方 electron-liquid-glass，开启时 `main.ts` L59–62 打开 9222 调试端口并设 `remote-allow-origins=*`（只读源码，未运行验证） | 浅克隆 + gh api（一手） |
| B7 | spacering-net/codeg：package.json、`src-tauri/Cargo.toml`、`tauri.conf.json`、`next.config.ts`、README | https://github.com/spacering-net/codeg | HEAD `a9a788e`（2026-10-16）；v0.35.2（2026-10-15）；4.0k star | Tauri 2.11（`tauri` 为可选 feature，同一 Rust 核心也能编成 axum 无头服务器）；Next.js 16 静态导出 + React 19；streamdown ^2.2 + 官方 cjk/code/math/mermaid 插件、shiki 3、monaco、xterm 6、tiptap 3；agent-client-protocol 2.2.0；dmg 86.7 MB，服务器 tar 包 57 MB | 浅克隆 + gh api（一手） |
| B8 | Codeg issue #935、#330、#332、#518 | https://github.com/spacering-net/codeg/issues/935 | 2026-07～10 | #935：3 个会话跑 1.5 天后 WebContent 进程 1.4→2.2 GB、单核跑满，主进程 322 MB；采样里 `Document::updateEventRegions` 占 24–43%，MutationObserver 回调读 `scrollWidth` 引发布局抖动；#330：中文输入法组词被打断；#518：CJK @ 提及问题 | gh api（一手） |
| B9 | iOfficeAI/AionUi：package.json、`mobile/package.json`、`docs/contributing/development.md`、CHANGELOG | https://github.com/iOfficeAI/AionUi | HEAD `c140550`（2026-10-14）；v2.2.2（2026-09-09）；33.6k star | Electron ^37.10（electron-vite）+ React 19 + Arco Design；streamdown ^1.5、react-markdown、react-virtuoso、mermaid、diff2html、CodeMirror 6、monaco、i18next；后端已迁到 Rust 的 AionCore；移动端 Expo 55 / RN 0.83 | 浅克隆（一手） |
| B10 | AionUi RFC #2762「Migrate from Electron to Tauri v2」 | https://github.com/iOfficeAI/AionUi/issues/2762 | 2026-05-06，open，0 评论 | "the Electron main process has become a thin shell"；macOS 包约 248 MB；估算 Electron 300–500 MB、Tauri 80–150 MB（非实测）；风险列了"macOS WebKit limitations" | gh api（一手） |
| B11 | aaif-goose/goose：`ui/desktop/package.json`、`ui/` 目录 | https://github.com/aaif-goose/goose | ui/desktop 1.55.0；v1.54.0（2026-10-08）；55k star（block/goose 已重定向） | Electron 43.4 + Electron Forge 7.11 + fuses；React 19 + Radix + Tailwind 4；react-markdown、katex；新增 `ui/goose-acp-client`；zip 208.7 MB | gh api（一手） |
| B12 | goose PR #9066、#9368；Discussion #9367「Goose Desktop Evolution」 | https://github.com/aaif-goose/goose/discussions/9367 | 2026-05-22 | 2026-03～05 试做过 Tauri 版 goose2（`ui/goose2/src-tauri`），05-22 删除；理由是同时迁 ACP 后端和翻新 UI 难保质量，没有归咎于 Tauri | gh api + WebFetch（一手） |
| B13 | goose issue #11348、PR #11994 | https://github.com/aaif-goose/goose/issues/11348 | 2026-08～09 | 7.5k 条消息的会话从头回放并全量挂载、没有窗口化；⌘F 只搜已挂载的行；delta 不合并；结束时丢失贴底 | gh api（一手） |
| B14 | anomalyco/opencode（dev 分支）`packages/desktop`、`app`、`ui`、`session-ui` 的 package.json；v1.19.2 | https://github.com/anomalyco/opencode | desktop 2.0.30；v1.19.2（2026-10-16）；214k star | Electron 44.4.5 + electron-vite；SolidJS；@pierre/diffs、@pierre/trees、marked + marked-shiki + @shikijs/stream、mermaid 11.17、ghostty-web（终端）、@tanstack/solid-virtual；dmg 151.1 MB | gh api（一手） |
| B15 | opencode PR #15663、#19067、#25822；Brendonovich《Moving OpenCode Desktop to Electron》 | https://dev.to/brendonovich/moving-opencode-desktop-to-electron-4hip | 2026-03-02 新增 desktop-electron；03-25 CI 只构建 Electron；05-05 删除 src-tauri；文章 2026-04-19 | "Tauri uses WebKit on macOS and Linux, which not only has worse performance than Chromium when rendering our app"；不一致"directly impacts our ability to ship a consistent experience"；打包的 CLI 拖慢启动且"occasionally just failed"；计划让 server 直接跑在 Electron 内置 Node 里；"Electron is simply a better fit for our use case"；包体变大是"a trade-off we're willing to make"；"Getting our UI working was easy… agents are pretty useful" | gh api + WebFetch（一手；文章经摘要模型转述，引文为逐字摘句） |
| B16 | slopus/happy：packages 目录、`happy-app/package.json`、`src-tauri/tauri.conf.json`、happy-ios README 与 AGENTS.md；PR #1152 | https://github.com/slopus/happy | 2026-10-16 | 以前用 Tauri 包 Expo Web，src-tauri 最后提交 2026-05-22；README 下载入口已指向 happy-desktop；iOS 在把 RN 版逐屏移植成 UIKit 原生（未发布） | gh api（一手） |
| B17 | slopus/happy-desktop：各包 package.json、AGENTS.md、`docs/releases.md`；issue #23 | https://github.com/slopus/happy-desktop | HEAD `5bce846`（2026-10-15）；v0.0.97（2026-10-12） | 仓库 2026-07-11 创建，首个 release v0.0.25 于 08-01；Electron ^43.1 + React 19 + TanStack Router + Tailwind 4；@pierre/diffs、TanStack Virtual、react-markdown、beautiful-mermaid、CodeMirror 6、@slopus/ghostty-web；dmg 133.2 MB；Nightly 版从托管地址加载渲染层；#23：流式更新引发整窗 React 渲染风暴，渲染进程 OOM | 浅克隆 + gh api（一手） |
| B18 | formulahendry/acp-ui package.json | https://github.com/formulahendry/acp-ui | `cd9c3cb`（2026-05-25）；v0.1.16；499 star | Tauri 2 + Vue 3.5 + marked；dmg 4.1 MB | 浅克隆 + gh api（一手） |
| B19 | getpaseo/paseo：README、LICENSE、`docs/architecture.md`、`docs/agent-stream-performance.md`、`packages/desktop` 与 `app` 的 package.json；issue #5242、#6477 | https://github.com/getpaseo/paseo | HEAD `5acfbb3`（2026-10-16）；v0.11.4（2026-10-15）；21k star | Apache-2.0（LICENSE 原文）；Electron 44.2 + electron-builder；Expo 54 / RN 0.81 经 react-native-web 编成 Web，一套代码覆盖 iOS、Android、Web、桌面；xterm 6.1 beta（WebGL）、TanStack Virtual、CodeMirror 6、mermaid、markdown-it；"Electron wrapper around the web app that bundles and auto-manages its own daemon"；daemon 每个 agent 最多每 60 ms 发一条；dmg 185.8 MB；#5242 启动拉起大量进程，#6477 渲染进程把打开过的时间线都留在内存里 | 浅克隆 + gh api（一手） |
| B20 | LodyAI/Lody：`apps/electron`、`packages/components`、`packages/ui` 的 package.json；issue #240、#1455 | https://github.com/LodyAI/Lody | v0.104.0（2026-10-08）；2026-08-07 开源；1.4k star | Electron 44.7 + electron-vite；React + jotai + TanStack Router + base-ui/radix；@lobehub/streamdown、@pierre/diffs、shiki 4.1、xterm 5.5、mermaid + beautiful-mermaid、monaco、TanStack Virtual；8 个 ACP 扩展包；#240 长会话内存和打开延迟；#1455 大文档序列化导致多 GB 内存 | gh api + WebFetch（一手） |
| B21 | ACP 官方客户端清单 `docs/get-started/clients.mdx` | https://github.com/agentclientprotocol/agent-client-protocol | 2026-10-16 | "Desktop and Web"一节约 60 项 | gh api（一手） |
| B22 | Agent Studio（AgentX）Cargo.toml | GitHub | 2026-10-16 | GPUI（直接引 Zed 仓库 git 依赖）+ gpui-component；511 star；无许可证 | gh api |
| B23 | Vibex Cargo.toml、README | GitHub | 2026-10-16 | GPUI（gpui-pre 0.3.9）；AGPL；30 star | gh api |
| B24 | superlite.dev | https://superlite.dev | 2026-10-16 | "Built with GPUI"；只有"fast… lightweight"的定性描述 | WebFetch |
| B25 | oxi Cargo.toml、README | GitHub | 2026-10-16 | egui/eframe 0.35（wgpu + accesskit）+ syntect；"No Electron"；自称约 70 MB 内存（无测量方法）；未公证 | gh api |
| B26 | ClickPM/AcpAgentClient：`README.en.md`、`docs/background.md` | https://github.com/ClickPM/AcpAgentClient | 2026-10-15 | Flutter 外壳 + Rust 核心（cdylib 经 flutter_rust_bridge 同进程加载）；前作 Electron "500MB+，双运行时"；前作 GPUI "18 轮里大量时间花在手工复刻 UI"、"gpui 与任何宿主事件循环冲突"；2026-09-11 选 Tauri，09-12 改 Flutter | gh api（一手） |
| B27 | jockey package.json | GitHub | 2026-10-16 | Tauri 2 + SolidJS + marked + xterm 6；25 star | gh api |
| B28 | littleguys package.json | GitHub | 2026-10-16 | Tauri 2 + React 19 + react-markdown + TanStack Virtual；34 star | gh api |
| B29 | Gold-Band package.json | GitHub | 2026-10-16 | Tauri 2.11 + React 19 + radix；streamdown ^2.6、shiki 3、mermaid 12.1、@codemirror/merge；AGPL-3.0；147 star | gh api |
| B30 | Threadlines（`apps/desktop`、`apps/web`）package.json | GitHub | 2026-10-16 | Electron 43.7 + React 19.3；@pierre/diffs、xterm 6、lexical、react-markdown + rehype-sanitize；16 star | gh api |
| B31 | deepchat package.json | GitHub | 2026-10-16 | Electron 43.7 + Vue 3.5；@pierre/diffs、stream-diffs、stream-monaco、mermaid、xterm 6、tiptap、vue-virtual-scroller；6.4k star | gh api |
| B32 | Dealroom 新闻：Poolside Desktop Assistant 发布 | https://app.dealroom.co/news/note/poolside-launches-desktop-assistant-one-place-to-run-coding-agents-across-macos-vs-code-and-visual-studio | 2026-07-28 | 只有发布消息；ACP 清单写"native macOS desktop app"；技术栈不明 | 搜索摘要 |
| B33 | tahayerd/opencode-tauri README | https://github.com/tahayerd/opencode-tauri | 0 star | 自称空闲内存低约 10 倍，无测量方法，不可信 | WebFetch |

### B 组未能核实或有矛盾的地方

- Conductor 官网不提技术栈；创始人推文只能通过镜像读到；三方扫描为中等置信度，115 MB 也来自三方。
- Happy 从 Tauri 换到 Electron 的原因没找到；happy-ios 原生化会不会替换 RN 版不确定。
- Poolside Desktop 框架不明。
- AionUi、Lody 的 GitHub Release 没有安装包；AionUi RFC 的内存数字是估算。
- Harnss 的正式版停在 2026-03-19，是否仍活跃不确定。
- Paseo：本机装的是 0.2.3（L6），当前是 0.11.4，本机 404 MB 不代表现在的包体。
- opencode 的 dev.to 文章没有年份，2026 年是由推文 ID 推出的。
- 内存数字只有 issue 里的用户报告和项目自述；没有任何项目公布同一 UI 在 Electron 和 Tauri 上的实测对比。

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

---

## D：Rust 原生 UI 框架

只读克隆：zed（sparse，HEAD `28d0a35`，2026-10-15）、longbridge/gpui-kit（浅克隆，v0.7.3）、makepad（dev 分支浅克隆，2026-10-14）、slint（sparse）。没有编译或执行。

| 编号 | 标题 | URL | 版本 / 日期 | 要点摘录 | 获取方式 |
|---|---|---|---|---|---|
| D1 | Zed：Leveraging Rust and the GPU to render UIs at 120 FPS | https://zed.dev/blog/videogame | 2023-03-07 | Atom/Electron 时期遇到 GC 停顿、DOM 重排掉帧；"the only way to achieve the performance we needed was to build our own UI framework"；每种图元一个 Metal 着色器，用 SDF 画圆角；文字用系统 API 排版 | WebFetch（一手） |
| D2 | zed `crates/gpui/README.md`、`Cargo.toml` | https://github.com/zed-industries/zed/tree/main/crates/gpui | `28d0a35`（2026-10-15） | "still pre-1.0… often be breaking changes"；"best way to learn… read the Zed source code"；Apache-2.0 | sparse clone（一手） |
| D3 | zed `crates/gpui_macos/src/window.rs`、`crates/gpui/src/platform.rs` | 同 D2 | 同 D2 | 实现 NSTextInputClient（`setMarkedText`、`firstRectForCharacterRange`）；`WindowBackgroundAppearance::Blurred` 在窗口后插一层 NSVisualEffectView（UnderWindowBackground、BehindWindow），只能整窗；`set_traffic_light_position`；`accesskit_macos` | sparse clone（一手） |
| D4 | zed PR #56065 "gpui: Accesskit support" | https://github.com/zed-industries/zed/pull/56065 | 2026-05-27 合入 | GPUI 接入 AccessKit | clone + gh api（一手） |
| D5 | zed PR #63068 "gpui_ios: Add iOS platform backend"；gpui_web 首个提交 #50228 | https://github.com/zed-industries/zed/pull/63068 | 2026-10-14 合入；web 后端从 2026-02-26 开始 | iOS 后端走 UIKit、CoreText、Metal；未发布到 crates.io | gh api（一手） |
| D6 | zed 的 `terminal`、`markdown`、`editor`、`mermaid_render`、`auto_update` 各 `Cargo.toml` 和根 `Cargo.toml` | 同 D2 | 同 D2 | 这些 crate 都是 GPL-3.0-or-later，workspace 设 `publish = false`；terminal 用 Zed 自己 fork 的 alacritty_terminal；markdown 有 `append()`；Mermaid 用 `merman = "=0.8.0-alpha.5"` | sparse clone + gh api（一手） |
| D7 | crates.io：gpui、gpui-pre | https://crates.io/crates/gpui ；https://crates.io/crates/gpui-pre | gpui 0.2.2（2025-10-22）；gpui-pre 0.3.9（2026-10-12） | gpui-pre 是第三方（longbridge 维护者）发布的 "snapshot of zed@93d7456"，约每周一版 | crates.io API（一手） |
| D8 | gpui.rs 官网 | https://www.gpui.rs/ | — | "For the near future, GPUI is tied to Zed"；"From the creators of Zed & Delta" | WebFetch（一手） |
| D9 | longbridge/gpui-kit README 与源码（`component/diff`、`text`、`message_scroller`、`highlighter`；`webview/Cargo.toml`；`examples/ai_recipes`） | https://github.com/longbridge/gpui-kit | v0.7.3（2026-10-12），17.7k star | 75 个以上组件；Markdown 与 HTML 渲染、`stream_fade`；tree-sitter 与 syntect 两种高亮后端；`Diff`："Readonly, virtualized patch display inspired by Diffs from Pierre"，支持 `DiffAnnotation`（行或文件级，ID 稳定）和 hunk 分隔；变高 `VirtualList`；`MessageScroller` 自带回到底部按钮和底部渐隐；用 wry 嵌 WKWebView；Longbridge Pro 在生产中使用；仓库带 AGENTS.md、CLAUDE.md、skills、`script/check-ai`；Apache-2.0 | clone（一手） |
| D10 | longbridge/gpui-fast README；zed PR #62379 | https://github.com/longbridge/gpui-fast | 仓库创建于 2026-09-24；PR 仍 open | GPUI 的 fork，加了 Retained Mode 和窗口合成（macOS 上嵌 WKWebView，GPUI 浮层画在其上）；自称 "experimental" | gh api（一手） |
| D11 | zed-industries/awesome-gpui | https://github.com/zed-industries/awesome-gpui | 2026-10-15 | 收录的 GPUI 应用中有大量 2026 年的编码 agent 客户端：Zeron、Waku、Arbor、Runner、OxiMux、Hadron、Reviu、hunk、Codux、Farcaster、Vibex、Delta 等 | gh api（社区提交的列表） |
| D12 | Zed 的 IME 与无障碍 issue | https://github.com/zed-industries/zed/issues | 截至 2026-10-16 | #41138（Windows 读屏，open）、#56327（中文 IME，平台未注明，open）、#59193（Windows） | gh api search（一手） |
| D13 | Zed v1.24.2 与 gpui-kit v0.7.3 release 附件 | GitHub releases | 2026-10-14 / 2026-10-12 | Zed-aarch64.dmg 约 113 MB（另见 A27 的 HEAD 数字 118,780,925 字节）；gpui-shell darwin 压缩包 20 MB | gh api（一手） |
| D14 | zed `script/bundle-mac`；`crates/auto_update/Cargo.toml` | 同 D2 | 同 D2 | 自写脚本做 codesign + hardened runtime + 公证；auto_update 为 GPL | gh api（一手） |
| D15 | merman README 与 crates.io | https://github.com/Latias94/merman | 0.9.0（2026-10-12） | "follows Mermaid 12.2.0"；37 种图；输出 SVG/PNG/PDF；"Pixel-identical Chromium rendering is not guaranteed"；MIT/Apache（ELK 部分为 EPL-2.0）；Zed 在用 | WebFetch + crates.io（一手） |
| D16 | gpui-terminal、alacritty_terminal | crates.io | 0.1.0（2025-12-24）/ 0.26.0（2026-04-06） | 前者 MIT/Apache（社区），后者 Apache-2.0 | crates.io API |
| D17 | Dioxus releases、0.8.0-alpha.2 发布说明 | https://github.com/DioxusLabs/dioxus/releases | 0.7.10（2026-07-30）、0.8.0-alpha.2（2026-10-14）；crates.io 上 0.7.0 为 2025-10-31 | 0.8 为门户和多窗口重写运行时，有破坏性变更 | gh api + crates.io（一手） |
| D18 | Dioxus 0.7 发布博客 | https://dioxuslabs.com/blog/release-070 | 页面显示 2025-09-08 | Native 渲染器 "work in progress"；"We have not focused on performance"；提供 llms.txt；支持 iOS/iPad | WebFetch（一手） |
| D19 | dioxus-desktop 0.7.10、dioxus-native、blitz-shell 依赖 | crates.io | — | 桌面版依赖 wry 0.53、tao 0.34、muda；Native 版依赖 blitz-*、winit；accesskit 为可选依赖 | crates.io API（一手） |
| D20 | DioxusLabs/blitz README | https://github.com/DioxusLabs/blitz | 0.3.0-beta.3（2026-10-12） | "currently in a **beta** state… still many bugs and missing features"；"Used by" 为空 | WebFetch + crates.io（一手） |
| D21 | Dioxus 0.6 打包文档 | https://dioxuslabs.com/learn/0.6/cookbook/bundling | 0.6 | `dx bundle` 出 .app/.dmg；没有内建签名工具 | 搜索摘要 |
| D22 | slint `LICENSE.md` | https://github.com/slint-ui/slint/blob/master/LICENSE.md | master | GPLv3、Royalty-free 2.0、商业许可三选一；"Use Slint for free under the GPLv3… your own files can stay MIT or Apache-2.0"；Royalty-free 需披露使用了 Slint（AboutSlint 组件或徽章） | curl（一手） |
| D23 | Slint 1.18 发布博客；v1.18.0、v1.18.1 | https://slint.dev/blog/slint-1.18-released | 2026-09-16 / 2026-09-21 | FlexboxLayout、Vello 渲染器（实验）、输入框无障碍 | WebFetch + gh api（一手） |
| D24 | Slint 风格文档 | https://docs.slint.dev/latest/docs/slint/reference/std-widgets/style/ | latest | macOS 上 `native` 解析为 `cupertino`；`qt` 风格需装 Qt | WebFetch（一手） |
| D25 | Slint iOS 文档 | https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/ios/ | latest | iOS "can only use Rust"，走 winit + Skia | WebFetch（一手） |
| D26 | Slint 1.16 发布博客 | https://slint.dev/blog/slint-1.16-released | 2026-04-16 | StyledText 与 `@markdown`，"first step" | 搜索摘要 |
| D27 | slint 源码 `internal/backends/winit`；ListView 文档；1.18.1 features | https://github.com/slint-ui/slint | master；1.18.1 | 依赖 accesskit 0.24；`Ime::Preedit`、`set_ime_cursor_area`；默认 feature 含 accessibility；ListView "only instantiated if they are visible" | sparse clone + crates.io（一手） |
| D28 | Slint AI Coding Assistants 文档；博客 "Slint and AI MCP" | https://docs.slint.dev/latest/docs/slint/guide/tooling/ai-coding-assistants/ ；https://slint.dev/blog/slint-and-AI-MCP | 2026-07-03 | App 内嵌 MCP server，AI 可读无障碍树、驱动操作、截图，支持无头 | 搜索摘要（未逐字核对） |
| D29 | LibrePCB 2.0.0 发布 | https://librepcb.org/blog/2026-01-28_release_2.0.0/ | 2026-01-28 | 新 UI 用 Slint 重写，支持多窗口，从 Qt 迁出 | 搜索摘要 |
| D30 | iced 0.14.0 发布说明 | https://github.com/iced-rs/iced/releases/tag/0.14.0 | 2025-12-07 | 输入法支持（#2777）、Markdown 增量解析（#2776）、端到端测试、热重载、透明窗口、auto_scroll | gh api（一手） |
| D31 | iced PR #3281、#3111、#3245 | https://github.com/iced-rs/iced/pull/3281 | #3281 于 2026-03-14 关闭未合并；#3111 草稿 open；#3245 关闭未合并 | 上游没有无障碍；没有 iOS | gh api（一手） |
| D32 | COSMIC Epoch 1；pop-os/iced | https://lwn.net/Articles/1050268/ ；https://github.com/pop-os/iced | 2025-12-11 | COSMIC 稳定版；pop-os 的 fork 带 `accessibility` 目录 | 搜索摘要 + gh api |
| D33 | iced_term、iced_aw、iced_highlighter | crates.io | 0.8.0（2026-03-27）/ 0.14.1 / 0.14.0 | iced_term 基于 alacritty | crates.io API |
| D34 | egui README | https://github.com/emilk/egui | — | "If you want a GUI that looks native, egui is not for you"；"content needs to be laid out each frame"；AccessKit 支持 macOS/Windows；Rerun 赞助 | WebFetch（一手） |
| D35 | egui 0.37.0、eframe 默认 features；PR #7967、#7973 | https://github.com/emilk/egui/pull/7967 | 2026-10-15；PR 为 2026-03 | accesskit 为默认 feature；"Much improved IME" | crates.io + gh api（一手） |
| D36 | egui_commonmark、egui_extras、egui_virtual_list、egui_term、egui_code_editor、dioxus-markdown | crates.io | 0.26.0、0.37.0、0.12.0、0.1.0、0.4.2、0.0.1 | egui_virtual_list 支持变高；egui_term 自 2025-04 未更新 | crates.io API |
| D37 | makepad（dev 分支）README、`widgets/Cargo.toml`、`platform/os/apple`、`libs` | https://github.com/makepad/makepad | 2026-10-14 推送；crates.io 上 makepad-widgets 1.0.0 为 2025-05-13 | dev 分支 2.0.0 未发布；有 setMarkedText、NSVisualEffectView；没有 accesskit；libs 有 cef、terminal_core、html，widgets 有 rich_text | clone + WebFetch（一手） |
| D38 | Robrix、Moly AI | https://github.com/project-robius/robrix | 2026-10-14 | 基于 Makepad | gh api |
| D39 | Freya README、crates.io、PR #2380 | https://github.com/marc2332/freya | 0.4.3（2026-08-30）、0.5.0-rc.10（2026-10-11）；PR #2380（iOS）2026-10-08 仍 open | "Starting 0.4 Freya does no longer use Dioxus"；有 MarkdownViewer、CodeEditor（tree-sitter）、Terminal、VirtualScrollView | WebFetch + crates.io + gh api（一手） |
| D40 | cargo-packager；cargo-bundle | https://github.com/crabnebula-dev/cargo-packager | 0.11.8（2025-11-27）；cargo-bundle 0.12.0（2026-09-20） | 出 .app/.dmg，配 cargo-packager-updater；示例有 dioxus、egui、slint，无 iced；README 没提签名、公证 | WebFetch + crates.io |
| D41 | window-vibrancy | https://crates.io/crates/window-vibrancy | 0.8.1（2026-09-22） | 基于 raw-window-handle，基于 winit 的框架都能用 | crates.io |
| D42 | Reddit：egui 迁到 Slint 的内存 | 搜索结果 | 2026-06 | 内存从约 130 MB 降到 30–50 MB，单个 App 自测 | 搜索摘要（可信度低） |
| D43 | Delta 官网 | https://delta.dev/ | 2026 | Zed Industries 出品，"A Multiplayer Environment for Coding with Agents"，"Now in Beta"；官网未写 UI 技术（本机 bundle 检查见 L9） | WebFetch（一手） |
| D44 | Dioxus Labs 的 YC 页面 | https://ycombinator.com/companies/dioxus-labs | — | 自称 Airbus、ESA、华为在用 | 搜索摘要（自述） |
| D45 | rerun、halloy、icebreaker 仓库 | GitHub | 2026-10 | 分别基于 egui、iced、iced | gh api |

### D 组未能核实或有矛盾的地方

- Dioxus 0.7 日期：博客页显示 2025-09-08，crates.io 上 0.7.0 是 2025-10-31，以 crates.io 为准。
- GPUI 没有正式版本：crates.io 官方版停在 0.2.2（2025-10），实际使用的是第三方快照 gpui-pre 或 git 依赖；没有找到稳定 API 承诺。
- Slint：定价页只说 GPLv3 用于开源，LICENSE.md 写得更具体，以 LICENSE.md 为准。
- 没有找到任何受控的 Rust GUI 内存基准。
- 未核实：iced、egui 的多窗口；Freya 的 VirtualScrollView 能否自动测量变高；Slint ListView 能否处理变高行；@pierre/diffs 有没有不依赖 React 的 API；Slint MCP 与 LibrePCB 只拿到搜索摘要。

---

## E：新一代 Web 外壳与其他跨平台框架

只读浅克隆：Electrobun `c4bbeca`（2026-10-15）、Tauri `v3` 分支 `1991873`（2026-10-14）、Wails master 的文档目录（稀疏克隆）。GitHub 匿名 API 限流后改用 `gh api`、`gh search code` 做只读 GET。

| 编号 | 标题 / 内容 | URL | 版本 / 日期 | 要点摘录 | 获取方式 |
|---|---|---|---|---|---|
| E1 | Electrobun 官网 | https://blackboard.sh/electrobun/ （electrobun.dev 302 跳转） | 2.0 | hello world 压缩包：Zig 主进程 1.28 MiB，Cottontail 约 15 MiB；对比"典型 Electron 约 85 MiB"；差分更新"小到 4 KB"；CEF、WGPU 可选；canary/production 两个通道；点名的生产产品只有 Dash | WebFetch（一手） |
| E2 | Electrobun README 与文档（`apis/browser-window.mdx`、`guides/code-signing.mdx`、`guides/updates.mdx`）、`package/src/native/macos/nativeWrapper.mm` | https://github.com/blackboardsh/electrobun | `c4bbeca`（2026-10-15） | 窗口选项有 `titleBarStyle` hidden/hiddenInset、`transparent`、`trafficLightOffset`；macOS 包装层（11,065 行）里没有 NSVisualEffectView，也没有 UNUserNotificationCenter（通知用已弃用的 NSUserNotification）；Hutch 负责签名、公证、staple；bsdiff 差分更新，失败退回全量 | 浅克隆只读（一手） |
| E3 | Electrobun `changelog/v2-x.mdx` | 同 E2 | 2.0 | 六种主进程运行时（Cottontail、Bun、Zig、Rust、Go、Odin）；解压后可执行文件：Cottontail 57,714,960 字节，Bun 63,072,928 字节（macOS ARM64）；Hutch 取代旧 CLI。现行 README 写"Bun 为默认，Cottontail 为实验性" | 同 E2 |
| E4 | Electrobun `CEF.md` | 同 E2 | 同 E2 | `CEF_VERSION 147.0.10`，Chromium 147.0.7727.118 | 同 E2 |
| E5 | Electrobun releases | https://github.com/blackboardsh/electrobun/releases | 至 2026-10-15 | 稳定版 v2.0.2（2026-09-29），v2.0.3-beta.26（2026-10-15）；v0.1.0 于 2025-08；1.x 于 2026-02 起；2.0.1 于 2026-08-22；约 1.3 万 star | gh api |
| E6 | Better Stack：Tauri vs Electron vs Deno vs Electrobun | https://betterstack.com/community/guides/scaling-nodejs/tauri-vs-electron-vs-deno-vs-electrobun/ | 2026-07-20 更新 | 包体 / 启动 / 空闲内存：Tauri 57 MB / 311 ms / 109 MB；Deno 111 / 242 / 98；Electron 323 / 273 / 128；Electrobun（开 CEF）418 / 773 / 208；未注明机型和版本 | WebFetch（三方） |
| E7 | Wails releases | https://github.com/wailsapp/wails/releases | 至 2026-10-15 | v3 alpha.1 于 2023-10-28，beta.1 于 2026-08-02，beta.30 于 2026-10-15（每晚自动发版）；v2.14.0 于 2026-08-10；约 3.7 万 star | gh api |
| E8 | Wails v3 `status.md` | https://github.com/wailsapp/wails（`docs/mpress/content/status.md`） | master | "Current Status: Beta"；beta 承诺只覆盖桌面；Android、iOS 为 experimental；要求 Go 1.25 | 稀疏克隆（一手） |
| E9 | Wails `v3/pkg/application/webview_window_options.go` | 同上 | master | `MacBackdropTranslucent`、`MacBackdropLiquidGlass`（macOS 15+，不支持时降级）；可选 NSVisualEffectMaterial；透明 webview 需 `-tags private_mac_apis` | 稀疏克隆（一手） |
| E10 | Wails `guides/updater.md`、`build/signing.md` | 同上 | master | updater 支持 GitHub Releases、Sparkle AppCast 等来源并校验签名；`wails3 task darwin:sign:notarize` | 稀疏克隆（一手） |
| E11 | Tiny RDM 仓库 go.mod | https://github.com/tiny-craft/tiny-rdm | 2026-07 | `wailsapp/wails/v2 v2.13.0`；本机包体见 L8 | 匿名 raw |
| E12 | Tauri v3 分支 `crates/tauri/CHANGELOG.md` | https://github.com/tauri-apps/tauri/tree/v3 | `1991873`（2026-10-14） | alpha.0："webview runtime is now selected when building the app"，`.runtime(tauri_runtime_wry::Wry::default())` 或 `tauri_runtime_cef::Cef::default()`；MSRV 1.95；alpha.3 插件钩子改为 Sync；alpha.4 移除 `macos-private-api` | 稀疏克隆（一手） |
| E13 | Tauri `crates/tauri-runtime-cef/CHANGELOG.md`、Cargo.toml；crates.io | 同上；https://crates.io/crates/tauri-runtime-cef | runtime-cef 3.0.0-alpha.0（2026-09-13）至 alpha.6（2026-10-14）；`@tauri-apps/cli-cef-v3.0.0-cef.0`（2026-05-04） | CEF 154.5.0+154.0.34；支持 `traffic_light_position`；alpha.6 修了"macOS overlay 标题栏拦截 webview 鼠标事件"；没有稳定版，累计下载 3,235 | 稀疏克隆 + gh api + crates.io API（一手） |
| E14 | crates.io tauri；`crates/tauri/Cargo.toml` | https://crates.io/crates/tauri | 2.12.4 / 3.0.0-alpha.5 | 核心依赖 window-vibrancy 0.8.1 | API + 克隆 |
| E15 | Neutralinojs releases；npm `@neutralinojs/neu` | https://github.com/neutralinojs/neutralinojs/releases | v6.10.0（2026-10-06）；neu CLI 11.8.0 | 约 8.7k star | gh api / npm |
| E16 | Neutralinojs 官网 | https://neutralino.js.org/ | — | 未压缩约 2 MB、压缩约 0.5 MB；"secure WebSocket connection for native operations and embeds a static web server"；扩展通过 IPC 子进程接入 | WebFetch（一手） |
| E17 | Neutralinojs 分发文档 | https://neutralino.js.org/docs/distribution/overview/ | — | neu builder 能出 dmg；没有签名、公证说明；updater 只更新资源包 | WebFetch（一手） |
| E18 | Deno 2.9 / `deno desktop` | https://deno.com/blog/v2.9 ；https://docs.deno.com/runtime/reference/cli/desktop/ | 2026-06-25 | experimental；默认系统 WebView，可选 CEF；是否进了 2.9.0 稳定版说法不一 | 搜索摘要 |
| E19 | Flutter `releases_macos.json` | https://storage.googleapis.com/flutter_infra_release/releases/releases_macos.json | 3.47.7（2026-10-08） | — | curl（一手） |
| E20 | What's new in Flutter 3.47 | https://flutter.dev/blog/whats-new-in-flutter-3-47 | 2026-08-12 | Impeller 成为 macOS 默认渲染器；"experimental desktop windowing APIs"；最低 macOS 12；Intel Mac 逐步停止支持 | WebFetch（一手） |
| E21 | Flutter 源码：`FlutterTextInputPlugin.mm`、`AccessibilityBridgeMac.mm`、`foundation/_features.dart`（`isWindowingEnabled`） | https://github.com/flutter/flutter | 2026-10 | 输入法实现了 NSTextInputClient；无障碍接 VoiceOver；官方多窗口仍在 feature flag 之后 | gh search code（一手） |
| E22 | pub.dev 包元数据 | https://pub.dev/api/packages/&lt;name&gt; | 见要点 | flutter_markdown 停更（0.7.7+1，2025-05），由 flutter_markdown_plus 1.0.12 接替；gpt_markdown 1.3.4（2026-10-08）；flutter_highlight 0.7.0（2021）；re_editor 0.10.0（2026-07）；xterm（xterm.dart）4.0.0（2024-02）；super_sliver_list 0.4.1（2024-03）；flutter_rust_bridge 2.13.0（2026-08）；auto_updater 1.0.0（2024-10，基于 Sparkle）；macos_ui 2.2.2（2025-10）；macos_window_utils 1.9.1（2026-01）；pub 上的 mermaid 包仅支持 Web | pub.dev API |
| E23 | RustDesk `Cargo.toml`；AppFlowy `frontend/rust-lib/Cargo.toml` | https://github.com/rustdesk/rustdesk ；https://github.com/AppFlowy-IO/AppFlowy | 2026-10 | RustDesk `flutter_rust_bridge = "=1.80"`；AppFlowy 用自写 dart-ffi | gh api（一手） |
| E24 | Compose Multiplatform releases | https://github.com/JetBrains/compose-multiplatform/releases | 1.12.2（2026-10-15）；1.13.0-alpha02 | — | gh api |
| E25 | Compose Multiplatform 1.8.0：iOS stable | https://blog.jetbrains.com/kotlin/2025/05/compose-multiplatform-1-8-0-released-compose-multiplatform-for-ios-is-stable-and-production-ready/ | 2025-05 | iOS 进入 stable；比 SwiftUI 多约 9 MB（厂商自报） | 搜索摘要 |
| E26 | Compose 桌面无障碍文档 | https://kotlinlang.org/docs/multiplatform/compose-desktop-accessibility.html | — | macOS "Fully supported" | WebFetch（一手） |
| E27 | Compose 原生分发文档 | https://kotlinlang.org/docs/multiplatform/compose-native-distribution.html | — | jpackage + jlink + ProGuard；有 notarize 任务；没有官方自动更新（Conveyor 为第三方，闭源项目付费） | WebFetch（一手） |
| E28 | Toolbox App 迁移到 Compose | https://blog.jetbrains.com/kotlin/2021/12/compose-multiplatform-toolbox-case-study/ | 2021-12 | CEF + React + C++ → Kotlin + Compose，官方称包更小、运行更快 | 搜索摘要 |
| E29 | multiplatform-markdown-renderer；JediTerm | https://github.com/mikepenz/multiplatform-markdown-renderer ；https://github.com/JetBrains/jediterm | v0.46.0（2026-10-13）；JediTerm 最近推送 2026-09-08 | JediTerm："Pure Java Terminal Emulator"，Swing 组件 | gh api |
| E30 | npm：react-native-macos、react-native | https://registry.npmjs.org/react-native-macos | 0.83.0（2026-09-30）；上游 0.88.0（2026-10-15） | macOS fork 落后上游 5 个小版本 | npm API（一手） |
| E31 | RN Windows/macOS showcase；Messenger 桌面关停报道 | https://microsoft.github.io/react-native-windows/resources-showcase ；https://cyberinsider.com/meta-to-shut-down-messenger-desktop-apps-on-december-15-redirects-users-to-web/ | 2023 至 2025-12 | Office 的 Word、Excel、PowerPoint 部分界面使用；Messenger 先改 PWA 再关停 | 搜索摘要（新闻为二手） |
| E32 | react-native-enriched-markdown；react-native-webview | https://github.com/software-mansion-labs/react-native-enriched-markdown | 1.1.1（2026-10-06）；webview 14.0.1 | 支持 iOS、Android、macOS、Web，支持流式，原生文本 | gh api / npm |
| E33 | uniffi-bindgen-react-native；uniffi | https://github.com/jhugman/uniffi-bindgen-react-native | 0.31.0-6（2026-09-25）；uniffi 0.32.2（2026-09-23） | 支持 Android、iOS、Web(WASM)、Node，未列 macOS | gh api / crates.io |
| E34 | Avalonia releases | https://github.com/AvaloniaUI/Avalonia/releases | 12.0.0（2026-04-07）、12.1.0（2026-07-09）、12.1.4（2026-10-09） | — | gh api |
| E35 | Avalonia 12 公告 | https://avaloniaui.net/blog/avalonia-12 | 2026-04-07 | 重点在性能和稳定性；WebView 控件开源；只支持 .NET 8 及以上，iOS 需 .NET 10 | WebFetch 列表页 + 搜索摘要 |
| E36 | Avalonia showcase | https://avaloniaui.net/showcase | — | JetBrains dotMemory/dotTrace、Unity Plastic SCM、GitHub Git Credential Manager、Icons8 Lunacy、Schneider Electric | WebFetch（一手） |
| E37 | Avalonia 源码 `native/Avalonia.Native/src/OSX/`（AutoFitContentView.mm、AvnView.h、automation.mm、menu.h）、`NativeMenu.cs` | https://github.com/AvaloniaUI/Avalonia | 2026-10 | macOS 上材质（NSVisualEffectView）、输入法（NSTextInputClient）、无障碍（NSAccessibility）、系统菜单都有原生实现 | gh search code（一手） |
| E38 | NuGet：AvaloniaEdit、Markdown.Avalonia、LiveMarkdown.Avalonia、Avalonia.Controls.WebView；GitHub 上的 Avalonia 终端库 | https://www.nuget.org/ | 2026-10 | AvaloniaEdit 12.0.0 + TextMate；Markdown.Avalonia 停在 11.0.3；LiveMarkdown.Avalonia 2.4.3 面向 LLM 流式；WebView 12.1.1；终端库只有一个 9 star 的社区项目（2024-05） | NuGet API / gh |
| E39 | Avalonia `TransparencyLevelHint` 文档 | https://api-docs.avaloniaui.net/docs/P_Avalonia_Controls_TopLevel_TransparencyLevelHint | — | AcrylicBlur 可能降级为 Blur | 搜索摘要 |
| E40 | crates.io：csbindgen、specta、tauri-specta；uniffi-bindgen-cs 仓库 | https://crates.io/crates/csbindgen | csbindgen 1.9.8（2026-05-20）；specta、tauri-specta 2.0.0-rc.25（2026-05）；uniffi-bindgen-cs 最近推送 2026-10-09 | — | API |

### E 组未能核实或有矛盾的地方

- **更正上一轮**：desktop-engineer.md 第 2.1 节写"CEF 后端没有进入发布版"，已不成立。`tauri-runtime-cef` 从 2026-09-13 起随每个 v3 alpha 发布到 crates.io（E13）。没找到 Tauri 官方关于 v3 或 CEF 的公告、路线图和正式版时间。
- Electrobun 的"12–14 MB 包体"在现行文档里找不到，官方现在给的是压缩后 1.28 MiB / 约 15 MiB；默认运行时中途从 Cottontail 改回 Bun；1.0.0 的精确发布日期找不到。
- Deno Desktop 只看了搜索摘要。
- Flutter、Compose、RN macOS、Avalonia 在 macOS 上的包体和内存没有可信实测数字。
- RN macOS 的 vibrancy 和多窗口 API 用代码搜索没找到，可能是索引没覆盖。
- Compose 桌面的中文输入法质量没有核实。
- Messenger 关停、Office 使用 RN、Toolbox 迁移、Compose iOS stable 只看了搜索摘要或新闻。

---

## X：横向资料

| 编号 | 标题 | URL | 版本 / 日期 | 要点摘录 | 获取方式 |
|---|---|---|---|---|---|
| X1 | GitHub Octoverse 2025 | https://github.blog/news-insights/octoverse/octoverse-a-new-developer-joins-github-every-second-as-ai-leads-typescript-to-1/ | 2025-10-28（2026-02-28 更新） | "August 2025 marks the first time TypeScript emerged as the most used language on GitHub"；"developers are shifting toward typed languages that make agent-assisted coding more reliable in production"；"Typed systems help identify LLM-generated compile errors earlier in the pipeline." | WebFetch（一手） |
| X2 | crates.io：类型共享与 FFI 工具 | https://crates.io/api/v1/crates/&lt;name&gt; | 2026-10-16 查询 | `schemars` 1.2.3（2026-10-12）；`ts-rs` 12.0.1（2026-01-31）；`specta` / `tauri-specta` 稳定版 1.0.x，2.0 仍是 2.0.0-rc.25（2026-05-07）；`typeshare-cli` 1.13.4（2025-12-11）；`uniffi` 0.32.2（2026-09-23，仍是 0.x）；`napi`（napi-rs）3.14.2（2026-10-06） | curl（UA 为 `cupper-research`） |
