# R05 来源索引与原始摘录

访问日期均为 2026-10-16。编号规则：

- `L*`：本机 `/Applications` 下已安装 App 的 bundle 只读检查（拆解方法见 L0）；
- `G*`：海外 AI 助手与 agent 类；
- `N*`：国内热门 AI 类；
- `P*`：大众热门 App；
- `X*`：横向资料（整体分布统计等）；
- 引用 R04 的来源时写成"R04 A1"这种形式，指 `docs/tasks/R04-desktop-framework-survey/sources.md` 的编号。

标注：

- "一手"指官方文档、官方博客、官方开源仓库、官方更新源或下载接口、Mac App Store 的 iTunes Lookup API、release 元数据、本机 bundle；"二手"指第三方文章、论坛帖子，以及 Homebrew cask 这类社区维护的元数据；"三方扫描"指 desktopinsights.com 这类按 bundle 自动推断框架的站点；"搜索摘要"指只拿到搜索结果摘要、没有打开原文。
- 外发请求都不带个人信息：curl 统一用 `-A cupper-research`，没有登录任何网站，没有提交表单。少数版本接口要求带机器 ID 参数时，填的是占位值（例如 `x-machine-id=default`、`machine=0`）。GitHub 只用本机已登录的 `gh` 做只读 GET；请求内容不含个人信息，但认证头会让 GitHub 知道是哪个账号在查询（与 R04 相同）。
- 没有下载或运行任何第三方安装包；包体只来自 HEAD 请求的 `Content-Length`、Release API 的 `size` 字段、iTunes Lookup API 的 `fileSizeBytes`、更新源（latest-mac.yml、appcast.xml）里的 `size`/`length`、官方页面上写的大小，或本机已安装 App 的 `du -sh`。下载过的只有 appcast.xml、latest-mac.yml、版本 JSON 这类小的元数据文本。
- 包体单位：字节数照抄（B）；文中的 MB 是十进制（10^6）；`du -sh` 的 MB、GB 和三方扫描的"装后"大小按原样照抄。
- 调研分三路并行（G、N、P 各一个分支调研员），L 和 X 由主执行者完成，最后统一整理到本文件。主执行者对各组的关键结论做了复核，复核记录见各组末尾。

---

## L：本机 bundle 检查

### L0 拆解方法

与 R04 L0 相同，只读查看 `/Applications/<App>.app`，没有运行任何 App，没有读取任何用户数据目录，没有解包或修改 bundle。用到的操作：

- `ls`、`du -sh`：`Contents/Frameworks`、`Contents/Helpers`、`Contents/Resources` 的目录结构和体积；
- `plutil -p`：主 App、框架、Helper 的 `Info.plist`（`CFBundleIdentifier`、版本、`LSMinimumSystemVersion`、`NSPrincipalClass`）；
- `otool -L`：主可执行文件或主库链接了哪些系统框架；`lipo -archs`：单架构还是通用二进制；
- `grep -a -o`：在二进制里搜版本串（`Chrome/x.y.z`、`Qt x.y.z`）、类名或命名空间（`iron::`、`QWidget`）、Rust crate 路径（`cargo/registry/src/...`）；
- 用一个只读的 Python 脚本读 `.asar` 的头部目录（JSON），只看条目名，没有解包、没有执行其中的代码。

### L1 飞书 Feishu / Lark（`/Applications/Lark.app`）

| 项目 | 观察结果 |
|---|---|
| 版本 | 主 App `CFBundleShortVersionString` 为 143.0.7499.203（等于 Chromium 版本号）；`Resources/manifest.json` 里 `appVersion` = 7.75.15，`appIdName` = Feishu，`native-branch` = `release-native-7.75.0`，`native-iron-branch` = `release-iron-7.75.0` |
| 标识 | `CFBundleIdentifier` = `com.electron.lark`；`CFBundleName` = Feishu，`CFBundleDisplayName` = Lark；`LSMinimumSystemVersion` 11.0；`DTSDKName` macosx15.0；Info.plist 带 `AsarIntegrity`（逐个列出 messenger、calendar、mail、docs 等 asar 的 SHA256） |
| Frameworks | 只有 `Lark Framework.framework`（`com.electron.lark.framework`，版本 143.0.7499.203，`SCMRevision` 指向 Chromium `refs/branch-heads/7499`）和 `Notification.app`。**没有** `Electron Framework.framework` |
| 运行时二进制 | `Lark Framework` 主二进制 285,396,480 字节，arm64 单架构；含版本串 `Chrome/143.0.7499.203`；有 `BrowserCrApplication`；大量 C++ 命名空间 `iron::`（`iron::IronFrame`、`iron::IronServiceHandler`、`iron::FeatureGatingManager`，以及视频会议相关的 `iron::VCEntry`、`iron::MeetingWindowManager` 等）；**没有** `Electron/x.y.z` 版本串，也没有 `NODE_MODULE_VERSION`、`node::Environment`（即没有内嵌 Node.js）；残留 `ElectronDataDir`、`ElectronLarkConfig` 这类配置名 |
| Helpers | `Lark Helper.app`、`Lark Helper (GPU).app`、`Lark Helper (Renderer).app`、`Lark Helper (Plugin).app`、`Lark Helper (Iron).app`（`com.electron.lark.iron`） |
| 前端 | `Resources/webcontent/` 下按业务拆成多个 asar：`messenger`、`main-window`、`calendar`、`mail`、`space`、`wiki`、`todo`、`search`、`video-conference-*` 等；另有 `docs`、`bitable`、`miniprogram`、`openplatform`、`doubao-page`、`doubao-worklet` 等目录。`main-window.asar` 头部只有按数字编号的分块目录和若干模态框目录，是前端构建产物 |
| 原生库 | `Libraries/` 下有 `liblark.dylib`（91,047,824 字节）。其中能搜到 735 处 `cargo/registry` 路径，crate 有 `tokio`、`prost`、`candle-nn` 等，自有 crate 路径有 `crates/msg`、`crates/sqlite`、`crates/secretchat`、`crates/livequery`、`crates/ai` 等，说明核心逻辑是 Rust。另有 ByteRTC / VolcEngineRTC、ffmpeg、`libvk_swiftshader` 等音视频和图形库 |
| 体积 | 安装后 1.5 GB（几乎全部在 `Lark Framework.framework`） |

解读（推测）：飞书桌面端 bundle 的命名（`com.electron.lark`、`AsarIntegrity`、Helper 进程布局）都是 Electron 式的，但运行时已经不是官方 Electron，而是字节自己基于 Chromium 143 的运行时（内部代号疑为 Iron），去掉了 Node.js；UI 是 asar 里的 Web 前端，业务核心是 Rust。外部证据见 P 组。

### L2 Microsoft Teams（`/Applications/Microsoft Teams.app`）

| 项目 | 观察结果 |
|---|---|
| 版本 | 26261.303.5165.1465；`com.microsoft.teams2`；`LSMinimumSystemVersion` 14.0；`DTSDKName` macosx26.2；`NSPrincipalClass` = `NSApplication` |
| 主程序 | `Contents/MacOS/MSTeams` 61,172,096 字节，通用二进制（x86_64 + arm64）；`otool -L` 链接 `SwiftUI`、`AppKit`、`Metal` 和 `@rpath/MSWebView2.framework` |
| Frameworks | `MSWebView2.framework`、`libSlimCoreWebview2.dylib`、`wv2_preload_fix.dylib`、`OneAuth.framework`、`MacRenderer.framework`、`libskypert.dylib`、`libRtmMediaStack.dylib`（75 MB，音视频）、AppCenter 系列；共 148 MB |
| Helpers | `Microsoft Teams WebView.app`（`com.microsoft.teams2.helper`，版本 154.0.4258.37，`NSPrincipalClass` = `BrowserCrApplication`），里面是 `Microsoft Edge Framework.framework` 154.0.4258.37（通用二进制，852 MB）；另有 `Microsoft Teams ModuleHost.app` |
| 资源 | `Resources` 只有 11 MB；`Resources/client-bindings/` 下是 `bundle.js`、`calling_electron_bundle.js`、`scwv2_bundle.js` 等桥接脚本，没有看到完整的 Web 前端 |
| 体积 | 安装后 1.1 GB，其中 Edge 框架约 853 MB |

解读：新版 Teams 在 macOS 上是原生外壳（Swift/SwiftUI + AppKit）加 Microsoft 自带的 WebView2（内嵌一份 Edge/Chromium 154），不是 Electron，也不是系统 WKWebView。Web 前端不在 bundle 里，推测从云端加载并在本地缓存【推测】。

### L3 WeChat（`/Applications/WeChat.app`）

| 项目 | 观察结果 |
|---|---|
| 版本 | 4.1.15（`CFBundleVersion` 270100）；`com.tencent.xinWeChat`；`LSMinimumSystemVersion` 12.0；`DTSDKName` macosx15.2；`NSPrincipalClass` = `NSApplication` |
| 主程序 | `Contents/MacOS/WeChat` 只有 186,368 字节，主体在 `Resources/wechat.dylib`（337 MB，通用二进制） |
| UI 框架 | `wechat.dylib` 中有版本串 `Qt 5.15.14`，`QWidget` 出现 180 次、`QApplication` 44 次；没有 `QQuickItem`、`QQmlEngine`（即用 Qt Widgets，不是 QML）；`otool -L` 链接 `Cocoa`、`AppKit`、`CoreText`、`OpenGL`，以及自带的 `libEGL`、`libGLESv2`（ANGLE） |
| 内嵌浏览器 | `MacOS/WeChatAppEx.app`（`com.tencent.flue.WeChatAppEx`，2.5.7.25716），内含 `WeChatAppEx Framework.framework`，二进制中有 `Chrome/144.0.7559.236` 和大量 `XWeb` 字样；体积 703 MB。用于小程序和内置网页 |
| 其他 | `Sparkle.framework`（2.6.4，自动更新）；`owl.framework` 只有 2 MB，二进制里是 `owl::promise`、`owl::ioc` 这类 C++ 异步库符号，和 OpenAI 的 OWL 无关；`mmcronet`（Cronet 网络栈）、`MultiMediaDyn`、OCR 和翻译模型 |
| 体积 | 安装后 1.4 GB（`Resources` 598 MB，WeChatAppEx 703 MB，XPlayer 95 MB） |

### L4 Telegram Desktop（`/Applications/Telegram.app`）

| 项目 | 观察结果 |
|---|---|
| 版本 | 7.1.3；`com.tdesktop.Telegram`（这是 Qt 版 Telegram Desktop，不是 App Store 上的原生版 Telegram for macOS）；`LSMinimumSystemVersion` 10.13；`DTSDKName` macosx26.5；`NSPrincipalClass` = `NSApplication` |
| 主程序 | 单个 232,068,480 字节的 arm64 可执行文件，静态链接 Qt：版本串 `Qt 6.11.1`，`QApplication` 61 处；`otool -L` 链接 `AppKit`、`Metal`、`WebKit` |
| Frameworks | 只有 Swift 运行时 dylib 和 `Updater`（tdesktop 自带的更新程序），没有 Qt 框架目录（静态链接） |
| 体积 | 安装后 248 MB |

---

## G：海外 AI 助手与 agent 类

| 编号 | 标题 | URL | 资料日期 | 要点摘录 | 获取方式 |
|---|---|---|---|---|---|
| G1 | Manus 博客：Introducing My Computer（Manus Desktop） | https://www.manus.im/blog/manus-my-computer-desktop | 2026-03-16 | "the core capability of the new Manus Desktop application"；"available today for all macOS and Windows users"；"Manus executes command line instructions (CLI) in your computer's terminal" | WebFetch（一手） |
| G2 | Manus：Introducing Manus 2.0 | https://manus.im/en/blog/introducing-manus-2-0 | 2026-09-28 | "With Manus 2.0, the desktop app is upgraded to Manus Studio: a shared workspace for people and AI." | WebFetch（一手） |
| G3 | Manus 桌面下载页 | https://manus.im/desktop | 2026-10-16 | 标题"Manus Studio"；macOS 链接为 `https://download.manus.im/Manus-Setup-2.0.4.dmg`；Windows 走 Microsoft Store | WebFetch（一手） |
| G4 | Manus 更新源 latest-mac.yml | https://download.manus.im/latest-mac.yml | releaseDate 2026-10-12 | electron-builder 格式；`version: 2.0.7`；`Manus-Setup-2.0.7.zip size: 319163581`；`Manus-Setup-2.0.7.dmg size: 319733438` | curl（一手；主执行者复核一致） |
| G5 | HEAD：Manus 2.0.7 / 2.0.4 dmg | https://download.manus.im/Manus-Setup-2.0.7.dmg ；https://download.manus.im/Manus-Setup-2.0.4.dmg | Last-Modified 2026-10-13 / 2026-10-05 | 2.0.7：319,733,438 B；2.0.4（官网按钮当前指向的版本）：315,061,735 B | curl -I（一手） |
| G7 | desktopinsights：Manus Studio | https://desktopinsights.com/apps/manus | 约 2026-10-14 扫描 | "Manus Studio is an Electron app (v43.6.0)"；装后 914 MB（Apple Silicon）；依赖含 React、Next.js、electron-builder、electron-updater、node-pty、objc2；版本记录从 v1.6.3（2026-06-23）起 | WebFetch（三方扫描） |
| G8 | Manus 博客：Browser Operator | https://manus.im/en/blog/manus-browser-operator | 2025-11-18 | "a browser extension that enables Manus to operate directly within your local browser environment"；"Chrome and Edge are currently recommended" | WebFetch（一手） |
| G9 | oflight：Manus Studio 解说 | https://www.oflight.co.jp/en/columns/manus-studio-desktop-app-computer-use-guide-2026 | 2026-10-12 | "rebuilt and renamed from 'Manus Desktop' with Manus 2.0"；"supports macOS (Apple Silicon) and Windows" | WebFetch（二手） |
| G10 | Homebrew cask：perplexity | https://github.com/Homebrew/homebrew-cask/blob/main/Casks/p/perplexity.rb | 2026-10-14 | `url "https://macos-download.perplexity.ai/Perplexity.dmg"`；livecheck 用 Sparkle appcast；bundle `ai.perplexity.macv3`；zap 清单含 `~/Library/WebKit/ai.perplexity.macv3` | gh api（二手） |
| G11 | Perplexity Sparkle appcast | https://macos-download.perplexity.ai/appcast.xml | pubDate 2026-10-14 | `Version 26.40.0`；`minimumSystemVersion 15.0`；`length="210974945"` | curl（一手） |
| G12 | HEAD：Perplexity.dmg | https://macos-download.perplexity.ai/Perplexity.dmg | Last-Modified 2026-10-14 | 210,974,945 B | curl -I（一手） |
| G13 | desktopinsights：Perplexity | https://desktopinsights.com/apps/perplexity | 约 2026-10-15 扫描 | runtime "swift"；装后 847 MB（Universal）；依赖含 Sparkle、Sentry、GRDB、Tokio、Rayon、WebRTC、objc2；未列 Chromium、Electron | WebFetch（三方扫描） |
| G14 | createwith / apfelpatient：Perplexity 新 Mac App | https://www.createwith.com/tool/perplexity/updates/perplexity-launches-new-mac-app-with-personal-computer-agents ；https://www.apfelpatient.de/news/perplexity-startet-neue-mac-app-mit-personal-computer | 2026-05-07 | "the older Mac app will be deprecated in the coming weeks"；"komplett neue native macOS-App"；官方用语 "all-new native Mac experience"（搜索摘要） | WebFetch（二手） |
| G15 | Thurrott：Perplexity Personal Computer | https://www.thurrott.com/?p=335813 | 2026-05-08 | 新版从官网下载；旧版 "will be deprecated in the coming weeks" | WebFetch（二手） |
| G16 | 9to5Mac、TechCrunch：2024 年的原生 Mac App | https://9to5mac.com/2024/09/30/perplexity-native-mac-app/ ；https://techcrunch.com/2024/10/24/perplexity-launches-a-native-mac-app | 2024-09-30 / 2024-10-24 | "its new native Mac app"，经 Mac App Store 分发；未提框架 | WebFetch + 搜索摘要（二手） |
| G17 | iTunes Lookup：Perplexity 旧版 | https://itunes.apple.com/lookup?id=6714467650&country=us&entity=macSoftware | 2026-10-16 | 按 id 和两个 bundleId 查询都返回 `"resultCount":0`，推断旧版已从美区商店下架 | curl（一手） |
| G18 | Homebrew cask：comet | https://github.com/Homebrew/homebrew-cask/blob/main/Casks/c/comet.rb | 2026-10 | `version "145.2.7632.4581"`；下载走 `perplexity.ai/rest/browser/download`；`depends_on macos: :monterey` | gh api（二手） |
| G19 | Comet 更新 API | https://www.perplexity.ai/rest/browser/update?browser=1.1.1.1&channel=stable&platform=mac_arm64&machine=0 | 2026-10-16 | `"browser_version":"145.2.7632.4581"`，arm64 和 x64 指向同一个 `comet_latest.dmg` | curl（一手） |
| G20 | Chromium Dash：M145 与当前 Stable | https://chromiumdash.appspot.com/fetch_milestones?mstone=145 ；https://chromiumdash.appspot.com/fetch_releases?channel=Stable&platform=Mac&num=1 | 2026-10-16 | M145 的 `chromium_branch` 为 "7632"，stable_date 2026-02-10；当前 Chrome Mac Stable 为 155.0.8059.85。Comet 版本号与 M145 分支对应 | curl（一手） |
| G21 | The Register：Comet | https://www.theregister.com/2025/07/09/perplexity_comet_browser/ | 2025-07-09 | "a customization built on its Chromium browser platform"；支持 Chrome 扩展 | 搜索摘要 |
| G22 | HEAD：Comet 下载（失败记录） | https://www.perplexity.ai/rest/browser/download?channel=stable&platform=mac_arm64 | 2026-10-16 | 307 跳到 example.com；R2 预签名 URL 的 HEAD 返回 403，拿不到大小 | curl -I（一手，失败） |
| G23 | Google 博客：The Gemini app is now on Mac | https://blog.google/innovation-and-ai/products/gemini-app/gemini-app-now-on-mac-os/ | 2026-04-15 | "The Gemini app is now available as a native macOS experience"；"on macOS versions 15 and up" | WebFetch（一手；主执行者复核一致） |
| G24 | Michael Tsai：Gemini App for Mac | https://mjtsai.com/blog/2026/04/16/gemini-app-for-mac/ | 2026-04-16 | 转引 Pichai："built this initial release with @Antigravity"，"went from an idea to a native Swift app prototype in a few days"；Josh Woodward："100% native Swift"；Gus Mueller："it contains 1,856 Objective-C classes whose class name starts with Java"，"a lot of shared Android code in there, but compiled to Objective-C and Swift" | WebFetch（二手汇总一手推文） |
| G25 | MacStories：Google Releases Gemini for Mac | https://www.macstories.net/news/google-releases-gemini-for-mac/ | 2026-04-15 | "The app is written in Swift"；"All the same tools found on the web are here, but in a native wrapper" | WebFetch（二手） |
| G26 | Homebrew cask：google-gemini | https://github.com/Homebrew/homebrew-cask/blob/main/Casks/g/google-gemini.rb | 2026-10-14 | `version "1.128.14.1166"`；`url "https://dl.google.com/release2/j33ro/release/Gemini.dmg"`；只支持 arm64、macOS 15+；bundle `com.google.GeminiMacOS`；zap 清单含 Keystone/GoogleUpdater 和 `~/Library/WebKit/com.google.GeminiMacOS` | gh api（二手） |
| G27 | HEAD：Gemini.dmg | https://dl.google.com/release2/j33ro/release/Gemini.dmg | 2026-10-13 至 10-15（不同 CDN 节点返回的 Last-Modified 不同） | 237,988,020 B | curl -I（一手；主执行者复核大小一致） |
| G28 | iTunes Lookup：Gemini | https://itunes.apple.com/lookup?bundleId=com.google.GeminiMacOS&country=us&entity=macSoftware | 2026-10-16 | 返回 0 条，不在 Mac App Store；iOS 版的 supportedDevices 不含 MacDesktop | curl（一手） |
| G29 | OpenAI：Building ChatGPT Atlas（即 R04 A1） | https://openai.com/index/building-chatgpt-atlas/ | 2025-10-30 | "OWL is OpenAI's integration of Chromium, which runs Chromium's browser process outside the main Atlas app process"；UI 用 SwiftUI、AppKit、Metal | 原文 403，见 R04 A1 的 Wayback 读取 |
| G30 | Atlas Sparkle appcast | https://persistent.oaistatic.com/atlas/public/sparkle_public_appcast.xml | 最后一条 2026-07-24 | 最终版 `1.2026.189.1`，带 `<sparkle:criticalUpdate/>`；`minimumSystemVersion 14.2`；arm64；`length="269967267"` | curl（一手） |
| G31 | HEAD：ChatGPT_Atlas.dmg | https://persistent.oaistatic.com/atlas/public/ChatGPT_Atlas.dmg | Last-Modified 2026-07-27 | 269,967,267 B（与最终版一致，仍可下载） | curl -I（一手） |
| G32 | Homebrew cask：chatgpt-atlas | https://github.com/Homebrew/homebrew-cask/blob/main/Casks/c/chatgpt-atlas.rb | 2026-08 | `disable! date: "2026-08-26", because: :discontinued` | gh api（二手） |
| G33 | OpenAI 帮助中心：Evolving Atlas into ChatGPT | https://help.openai.com/articles/20001371 | 2026-07 | 2026-08-09 起停止工作 | 搜索摘要（原页 403） |
| G34 | 36Kr：Atlas 关停 | https://eu.36kr.com/en/p/3933002676534663 | 2026-08-10 | "On August 9, OpenAI's AI-native browser ChatGPT Atlas was officially shut down, less than 10 months after its launch"；替代：ChatGPT 桌面 App 内置浏览器、云端浏览器、Chrome 扩展 | WebFetch（二手） |
| G35 | AI Toolbox：Atlas 关停之后 | https://www.ai-toolbox.co/chatgpt-management-and-productivity/chatgpt-atlas-browser-guide-2026 | 2026-09-16 | ChatGPT 桌面 App 内置浏览器 2026-07-09 上线（作者为竞品厂商） | WebFetch（二手） |
| G36 | TestingCatalog：Genspark 浏览器发布 | https://www.testingcatalog.com/new-genspark-browser-brings-ai-assistant-to-every-webpage.md | 2025-06-12 | "debuted yesterday (June 10, 2025)"；"first for macOS (Apple Silicon and Intel)" | WebFetch（二手） |
| G37 | GitHub：genspark-ai/omaha、genspark-ai/sparkle | https://github.com/genspark-ai/omaha ；https://github.com/genspark-ai/sparkle | 2025-12 创建 | omaha fork 自 `brave/omaha`，描述 "Omaha for Genspark browser"；sparkle fork 自 `brave/Sparkle` | gh api（一手） |
| G38 | HEAD：Genspark 浏览器旧版 dmg | https://cdn1.genspark.ai/user-upload-image/browser-package/Genspark-arm64-1.1.302.dmg | Last-Modified 2025-07-31 | 147,343,960 B（链接出自第三方 dotfiles 仓库） | curl -I（一手文件，三方链接） |
| G39 | aiindigo、LayerX：Genspark 浏览器 | https://aiindigo.com/tutorials/getting-started-with-genspark-browser-transform-passive-browsing-into-active-ai ；https://layerxsecurity.com/generative-ai/genspark-risks-and-vulnerabilities/ | 2025–2026 | "a standalone application built on Chromium architecture" | 搜索摘要 |
| G40 | Genspark Claw 帮助页、Workspace 4.0 新闻稿 | https://www.genspark.ai/helpcenter/genspark-claw ；https://secure.businesswire.com/news/home/20260408545044/en/ | 2026-04-08；帮助页 2026-09-21 | "The desktop app runs Claw directly on your own computer"；新闻稿称 "a native desktop client"（搜索摘要） | WebFetch + 搜索摘要（一手） |
| G41 | GitHub：genspark-ai/genoffice | https://github.com/genspark-ai/genoffice | v0.11.829，2026-10-15 | `apps/shell/package.json`：`"electron": "^43.7.6"`、`"electron-builder": "^26.0.12"`；Release：arm64 dmg 191,978,256 B，x64 dmg 197,950,633 B | gh api（一手） |
| G42 | MacDailyNews：How to use Grok AI on your Mac | https://macdailynews.com/2026/08/29/how-to-use-grok-ai-on-your-mac-iphone-and-ipad/ | 2026-08-29 | "There is still no official native Grok chat app for macOS." | WebFetch（二手） |
| G43 | iTunes Lookup / Search：Grok | https://itunes.apple.com/lookup?id=6670324846&country=us ；https://itunes.apple.com/search?term=grok&country=us&entity=macSoftware&limit=10 | 2026-10-16 | iOS 版 supportedDevices 不含 MacDesktop；Mac 搜索结果里没有 xAI/X Corp. 的应用 | curl（一手） |
| G44 | Homebrew cask：grok-bot | https://github.com/Homebrew/homebrew-cask/blob/main/Casks/g/grok-bot.rb | 2026-10 | `version "0.71.2"`；下载在 `downloads.cursor.com/sand/...`；`homepage "https://x.ai/bot"`；bundle `com.anysphere.sand`（Anysphere 即 Cursor 公司） | gh api（二手） |
| G45 | Grok Bot 更新 API | https://api2.cursor.sh/updates/api/update/darwin-arm64/sand/0.0.0/stable | 2026-10-16 | `{"url":"https://downloads.cursor.com/grokbot/stable/darwin-arm64/0.71.2/Grok_Bot_0.71.2.zip","name":"0.71.2"}` | curl（一手；主执行者复核一致） |
| G46 | HEAD：Grok Bot 安装包 | https://downloads.cursor.com/sand/stable/darwin-arm64/0.71.2/Grok_Bot_0.71.2.dmg ；https://downloads.cursor.com/sand/stable/darwin-x64/0.71.2/Grok_Bot_0.71.2_x64.dmg | Last-Modified 2026-10-14 | arm64 dmg 146,985,092 B；x64 dmg 156,486,357 B；arm64 更新 zip 132,951,076 B | curl -I（一手） |
| G47 | GitHub：grok-bot-linux-unofficial README | https://github.com/jakob-bu/grok-bot-linux-unofficial | 2026-08-27 | "take the official Electron app, put it on a Linux Electron runtime"；"The current Linux runtime is Electron 42.1.0"；与官方 Mac App 用 "same `app.asar`" | gh api（三方） |
| G48 | xAI 文档、AlternativeTo：Grok Bot | https://docs.x.ai/grok-bot/get-started ；https://alternativeto.net/news/2026/8/spacexai-launches-grok-bot-a-new-ai-agent-app-for-autonomous-tasks-on-iphone-and-mac/ | 2026-08-11 | Mac 版按 Apple Silicon / Intel 选 dmg；由 SpaceXAI 与 Cursor 推出，2026-08-11 beta | WebFetch + 搜索摘要 |
| G49 | iTunes Lookup：Microsoft Copilot（Mac） | https://itunes.apple.com/lookup?id=6738511300&country=us&entity=macSoftware | 2026-10-16 | `kind: mac-software`；`bundleId: com.microsoft.copilot-mac`；`version: 25.7.440902001`；`currentVersionReleaseDate: 2026-09-03`；`releaseDate: 2025-02-26`；`fileSizeBytes: 128940662`；`minimumOsVersion: 14.0` | curl（一手） |
| G50 | Mac App Store 页面：Microsoft Copilot | https://apps.apple.com/us/app/microsoft-copilot/id6738511300?mt=12 | 2026-10-16 | "128.9 MB"；没有 "Designed for iPad" 字样 | WebFetch（一手） |
| G51 | 9to5Mac：Copilot app to the Mac | https://9to5mac.com/2025/02/27/microsoft-brings-its-copilot-ai-app-to-the-mac/ | 2025-02-27 | "pulled what was actually the iPad version running on the Mac last year"；"no Intel Mac support" | WebFetch（二手） |
| G52 | Thurrott：Copilot Gets a Native Mac App | https://www.thurrott.com/?p=317829 | 2025-02-27 | "this new Copilot Mac app also seems to use a web view for the main screen"；"The app's left sidebar and settings menu look native" | WebFetch（二手） |
| G53 | Microsoft AI 招聘：Copilot Mac | https://microsoft.ai/job/software-engineer-5/ （现已 404，有转载） | 2025-09 至 2026 | "Copilot Mac app as the deeply integrated AI assistant built natively for macOS"；要求 "Swift and SwiftUI, Objective-C, AppKit, Combine, URLSession, XPC" | 搜索摘要 |
| G54 | Homebrew cask：poe | https://github.com/Homebrew/homebrew-cask/blob/main/Casks/p/poe.rb | 2026-06 | `version "1.1.45"`；zap 清单含 `com.quora.poe.electron`、`com.quora.poe.electron.ShipIt` | gh api（二手） |
| G55 | Poe 更新 API | https://updater.poe.com/darwin_arm64/0.0.0 | 2026-10-16 | Squirrel.Mac 格式：`{"url":"https://desktop-app.poecdn.net/updates/darwin_arm64/1.1.45.zip"}` | curl（一手） |
| G56 | HEAD：Poe 1.1.45 更新 zip | https://desktop-app.poecdn.net/updates/darwin_arm64/1.1.45.zip | Last-Modified 2026-06-19 | 108,856,750 B | curl -I（一手） |
| G57 | desktopinsights：Poe | https://desktopinsights.com/apps/poe | 2026-10 | "Electron v37.2.4"；装后 420 MB；v1.1.45（2026-06-20） | WebFetch（三方扫描） |
| G58 | IT之家：Poe 推出 Mac App；iTunes Lookup | https://www.ithome.com/0/715/495.htm ；https://itunes.apple.com/lookup?id=1640745955&country=us&entity=macSoftware | 2023-08-29；2026-10-16 | Mac 版 2023-08 推出；Mac App Store 没有 Quora 的 Mac 应用 | 搜索摘要 + curl |
| G59 | Granola 更新源 latest-mac.yml | https://api.granola.ai/v1/check-for-update/latest-mac.yml（302 跳转） | releaseDate 2026-10-15 | electron-builder 格式；`version: 7.663.2`；`Granola-7.663.2-mac-universal.zip size: 296998390` | curl -L（一手；主执行者复核一致） |
| G60 | HEAD：Granola dmg | https://dr2v7l5emb758.cloudfront.net/7.663.2/Granola-7.663.2-mac-universal.dmg | Last-Modified 2026-10-15 | 306,275,587 B（URL 出自 Homebrew cask，cask 用 `strategy :electron_builder`） | curl -I（一手；主执行者复核一致） |
| G61 | Granola 招聘（Ashby） | https://jobs.ashbyhq.com/granola/31cd18b8-f208-4a48-b287-11f32c0e32dd | 2026-06-26 | "Proficiency with TypeScript, Node.js, and React, and ideally Electron"；"Comfortable dropping into native code and OS audio APIs (CoreAudio, WASAPI, …)" | curl（一手） |
| G62 | desktopinsights：Granola | https://desktopinsights.com/apps/granola | 2026-10 | "Electron v44.4.2"；React；`better-sqlite3-multiple-ciphers`；装后 1074 MB（Universal） | WebFetch（三方扫描） |

编号有空缺（G6）：原始清单里是 2.0.4 dmg 的 HEAD 记录，已并入 G5。

### G 组未能核实或有矛盾的地方

- Comet 的包体没拿到（G22）。最新稳定版对应 Chromium 145，比当前 Chrome Stable（155）落后约 10 个大版本，原因不明。
- Gemini：官方说 "100% native Swift"，但包里有 1,856 个 Java 前缀的 ObjC 类（转译的 Android 共享代码），也有评论者觉得像 web view；SwiftUI 还是 AppKit、WKWebView 用了多少都未核实。desktopinsights 的 `/apps/gemini` 是另一款同名 App，不能用。
- Perplexity 新版：Swift 只有三方扫描支持；WKWebView 只是根据 cask 清理清单里的 WebKit 目录推测。旧版下架日期和框架未核实。
- Atlas：OpenAI 帮助中心原文 403，关停日期来自二手报道和搜索摘要。
- Genspark：官网对请求返回 403；浏览器最新版本和大小没拿到；"基于 Brave 代码线"是由官方 fork 了 brave/omaha 和 brave/Sparkle 推出来的；Claw for Desktop 框架未知。
- Grok Bot：Electron 结论来自社区移植仓库，xAI 和 Cursor 都没有官方说明。
- Microsoft Copilot：没有官方框架说明；"原生外壳加 web view 主面板"来自评测观察和招聘描述；不能排除 Catalyst。desktopinsights 的 `/apps/microsoft-copilot` 是 Windows 版，不能当 Mac 版证据。
- Poe：Electron 版本只有三方扫描；Mac 版 2026-06 后没有更新。

### G 组主执行者复核

- 重取 G4、G59 的 latest-mac.yml，版本、文件名和 `size` 一致；G45 的 Grok Bot 更新 API 返回一致；G27 的 Gemini.dmg `Content-Length` 一致（Last-Modified 因 CDN 节点不同而不同）；WebFetch 复读 G23，"native macOS experience"和"macOS versions 15 and up"一致。

---

## N：国内热门 AI 类

| 编号 | 标题 | URL | 资料日期 | 要点摘录 | 获取方式 |
|---|---|---|---|---|---|
| N1 | 豆包 settings 配置接口 | https://www.doubao.com/service/settings/v3/?device_platform=web&brand=doubao&aid=582465 | 2026-10-16 | `"version": "2.32.5"`，给出 `mac_universal_url`、`mac_arm64_url`、`mac_x64_url`；配置键里有大量 `chromium_*` 开关，例如 `chromium_SamanMultiProfile`、`chromium_AhaBrowserCookieImport`、`chromium_BrowserUseWebViewFullscreen`，以及 `saman_set_default_browser_config` | curl（一手；主执行者复核一致） |
| N2 | HEAD：豆包 2.32.5 dmg | https://lf-flow-web-cdn.doubao.com/obj/flow-doubao/doubao_pc/2.32.5/Doubao_universal_2.32.5.dmg ；同目录 `Doubao_Arm64_2.32.5.dmg`、`Doubao_X64_2.32.5.dmg` | Last-Modified 2026-10-15 | universal 791,928,218 B；arm64 592,305,074 B；x64 631,137,865 B | curl -I（一手） |
| N5 | desktopinsights：Doubao | https://desktopinsights.com/apps/doubao | 2026-10-16 扫描 | "Doubao is a cocoa app (v2.32.5)"；装后 1658 MB；Universal；依赖项 "Doubao Browser Framework" 147.0.7727.149；Rust 依赖 Tokio、hyper、reqwest 等 | curl + WebFetch（三方扫描；主执行者复核框架名和版本一致） |
| N6 | 少数派：使用豆包的 mac app 一段时间后 | https://sspai.com/post/91380 | 2024-08-13 | "这货本质上其实是一个 Chrome 内核的浏览器"；"它提供了和 Chrome 相同的浏览器功能设置，甚至连文案都没有改" | WebFetch（二手） |
| N7 | Homebrew cask：doubao | https://github.com/Homebrew/homebrew-cask/blob/master/Casks/d/doubao.rb | 2025-07-17 首次收录；2026-09-29 更新 | 卸载项删除 `/Applications/Doubao Browser.app`；livecheck 读 N1；2026-02-05 从 1.88.4 跳到 2.0.28 | gh api（二手） |
| N8 | iTunes Lookup：豆包（Mac App Store） | https://itunes.apple.com/lookup?id=6683305962&country=cn&entity=macSoftware | 2026-08-24 | version "2.24.10"；fileSizeBytes "438675878"；bundleId "com.bot.neotix.doubao"；minimumOsVersion "12.0" | curl（一手） |
| N9 | App Store 页面：豆包 | https://apps.apple.com/cn/app/id6683305962?mt=12 | 2026-10-16 | 2.14.8 版"浏览器地址栏改版"；简介写"支持使用浏览器、调用 Skills 技能和定时任务" | WebFetch（一手） |
| N10 | Homebrew cask：kimi（API） | https://formulae.brew.sh/api/cask/kimi.json | 2026-10-16 | version "3.2.17"；`app "Kimi Installer.app/Contents/Helpers/Kimi.app"`；`depends_on arch: :arm64` | curl（二手） |
| N11 | Kimi 最新 macOS 下载跳转 | https://appsupport.moonshot.cn/api/app/pkg/latest/macos/download | 2026-10-16 | 302 到 `https://kimi-img.moonshot.cn/app/download/mac/kimi_3.2.17.dmg` | curl -I（一手） |
| N12 | HEAD：Kimi 3.2.17 dmg | https://kimi-img.moonshot.cn/app/download/mac/kimi_3.2.17.dmg | Last-Modified 2026-10-15 | 379,301,600 B | curl -I（一手） |
| N13 | desktopinsights：Kimi | https://desktopinsights.com/apps/kimi | 约 3 个月前扫描 | "Kimi is an Electron app (v42.4.1)"；对应 v3.0.23；chromiumVersion 148.0.7778.265；装后 1900 MB；Apple Silicon | curl（三方扫描） |
| N14 | Homebrew cask kimi 提交历史 | https://github.com/Homebrew/homebrew-cask/commits/master/Casks/k/kimi.rb | 2025-11-11 至 2026-10-15 | 2025-11-11 收录 2.0.4；2025-12-20 "kimi: disable"（`because: :no_longer_available`）；2026-05-07 改为 3.0.6，下载路径换成 `/download/mac/`；2026-05-08 "kimi: require ARM" | gh api（二手） |
| N15 | HEAD：Kimi 2.1.1 dmg（旧版） | https://kimi-img.moonshot.cn/app/download/macos/kimi_2.1.1.dmg | Last-Modified 2025-11-20 | 225,867,996 B | curl -I（一手） |
| N16 | IT之家（新浪转载）：Kimi Work Beta 内测 | https://finance.sina.com.cn/tech/digi/2026-06-03/doc-iniacxkk4001164.shtml | 2026-06-03 | "Kimi 工程师使用 Kimi Code 等 Coding Agent 工具，一周内完成了 Beta 版的 Mac 和 Windows 客户端"；未提框架 | WebFetch（二手） |
| N17 | IT之家：Kimi Code Desktop 发布 | https://ithome.com/1/005/258.htm | 2026-09-21 | macOS 和 Windows 同步上线；未提框架 | WebFetch（二手） |
| N19 | 元宝公开配置接口 | https://yuanbao.tencent.com/api/info/public/general | 2026-10-16 | `"pcDownloadUrl": {"mac": ".../yuanbao_2.88.1.610_universal.dmg"}` | curl（一手） |
| N20 | HEAD：元宝 dmg | https://cdn-hybrid-prod.hunyuan.tencent.com/Desktop/official/0f1fc5bc1c36b43b1c330ef32639c08d/yuanbao_2.88.1.610_universal.dmg | Last-Modified 2026-10-10 | 102,168,902 B | curl -I（一手） |
| N21 | desktopinsights：元宝 | https://desktopinsights.com/apps/yuanbao | 约 2026-10-13 扫描 | "元宝 is a rust app (v2.88.1)"；"Desktop Framework" 一栏为 Tauri 2.2.0；装后 439 MB；Universal；另列出 "wxWidgets"（疑似误报） | curl（三方扫描；主执行者复核 Tauri 2.2.0 一致） |
| N22 | Homebrew cask：yuanbao | https://github.com/Homebrew/homebrew-cask/blob/master/Casks/y/yuanbao.rb | 2025-06-18 首次收录 | zap 清单含 `~/Library/WebKit/com.tencent.yuanbao`（说明用系统 WebKit） | gh api（二手） |
| N23 | 知乎、CSDN：元宝桌面端用 Tauri | https://www.zhihu.com/question/1900272230006491126 ；https://blog.csdn.net/m0_62652709/article/details/147630174 | 约 2025-04/05 | 知乎问题"如何评价腾讯元宝桌面端使用 Rust 的 Tauri 框架？"；CSDN 称基于 Tauri 2.0。原文都打不开（403 / 521） | 搜索摘要 |
| N24 | 千问 PC 客户端下载页 | https://www.qianwen.com/download | 2026-10-16 | `download.mac = ".../native/app/2.3.2/darwin/universal/qwenclient_setup_2.3.2.2603032246.dmg"` | curl（一手） |
| N25 | 千问 latest-mac.yml | https://qianwen-portal-cdn.tongyi.com/native/app/2.3.2/darwin/universal/latest-mac.yml | 2026-03-03 | electron-builder 格式；"version: 2.3.2"；dmg size 209952594；zip size 202421709 | curl（一手） |
| N26 | HEAD：千问 2.3.2 dmg | https://qianwen-portal-cdn.tongyi.com/native/app/2.3.2/darwin/universal/qwenclient_setup_2.3.2.2603032246.dmg | Last-Modified 2026-03-03 | 209,952,594 B | curl -I（一手） |
| N27 | desktopinsights：Qianwen | https://desktopinsights.com/apps/qianwen | 约 3 个月前扫描 | "Qianwen is an Electron app (v33.0.0)"；v2.3.1；chromiumVersion 130.0.6723.44；装后 820 MB；Universal | curl（三方扫描） |
| N28 | iTunes Lookup：千问（Mac App Store） | https://itunes.apple.com/lookup?id=6752689384&country=cn&entity=macSoftware | 2026-09-23 | version "4.3.5"；fileSizeBytes "287155442"；bundleId "com.alibaba.tongyi" | curl（一手） |
| N29 | App Store 页面：千问 | https://apps.apple.com/cn/app/id6752689384?mt=12 | 2026-10-16 | 3.0.0（2026-03-26）新增"智能浏览模式" | WebFetch（一手） |
| N30 | 界面新闻：通义改名千问 | https://www.jiemian.com/article/13662928.html | 2025-11-20 | "11月17日，阿里巴巴'千问APP'公测版正式上架…并同步提供网页版和PC版"；"在升级成'千问'之前，阿里的C端AI对话APP名为'通义'" | WebFetch（二手） |
| N33 | 安全内参转载国家计算机病毒应急处理中心通报 | https://www.secrss.com/articles/75719 | 2025-02-17 | "由于DeepSeek目前尚未针对Windows平台和MacOS平台推出官方客户端程序，因此相关文件均为仿冒程序" | WebFetch（二手转载官方通报） |
| N34 | DeepSeek 官方下载页 | https://download.deepseek.com/ | 2026-10-16 | "下载 DeepSeek 应用 … 支持 iOS 和 Android 平台。" | curl（一手） |
| N35 | DeepSeek Harness 官方页 | https://www.deepseek.com/harness/ | 2026-10-16 | 标"预览版"；Mac 下载 `https://download.deepseek.com/desktop/dsh-latest-macos-arm64.dmg` | WebFetch + curl（一手） |
| N36 | deepseek-ai/deepseek-harness 的 apps/desktop | https://github.com/deepseek-ai/deepseek-harness/tree/master/apps/desktop | 2026-10-16 | package.json：`"name": "@deepseek-ai/dsh-desktop"`，`"electron": "^44.7.0"`，`"electron-builder": "^26.17.0"`，`"electron-updater": "^6.8.9"`；README："The desktop application is an Electron shell around the complete dsh Web application." | gh api（一手；主执行者复核 package.json 一致） |
| N37 | deepseek-harness Releases | https://github.com/deepseek-ai/deepseek-harness/releases | 2026-09-29 至 10-15 | dsh-v0.2.0-rc.2（2026-09-29）；dsh-v0.2.1-alpha.4（2026-10-15，预发布） | gh api（一手） |
| N38 | HEAD：DeepSeek Harness dmg / zip | https://download.deepseek.com/desktop/dsh-latest-macos-arm64.dmg ；https://download.deepseek.com/dsh-desk/bin/mac-arm64/deepseek-harness-0.2.0-rc.2-mac-arm64.zip | Last-Modified 2026-09-29 | dmg 369,764,183 B；zip 374,053,565 B | curl -I（一手） |
| N40 | 36氪：DeepSeek Harness 桌面预览版 | https://eu.36kr.com/en/p/3998199345500040 | 2026-09-25 | "the desktop client is implemented with Electron"；"directly reuse the existing Web UI" | WebFetch（二手） |
| N41 | Trae 国际版版本接口 | https://api.trae.ai/icube/api/v1/native/version/trae/latest | 2026-10-16 | darwin "version": "3.6.2"；路径 `.../stable/2.3.91093/darwin/TRAE-darwin-arm64.dmg` | curl（一手） |
| N42 | HEAD：Trae 国际版 dmg | https://lf-cdn.trae.ai/obj/trae-ai-us/pkg/app/releases/stable/2.3.91093/darwin/TRAE-darwin-arm64.dmg ；同目录 `TRAE-darwin-x64.dmg` | Last-Modified 2026-10-09/10 | arm64 449,613,479 B；x64 478,468,707 B | curl -I（一手） |
| N43 | Trae CN 版本接口与 HEAD | https://api.trae.ai/icube/api/v1/native/version/trae/cn/latest ；https://lf-cdn.trae.com.cn/obj/trae-com-cn/pkg/app/releases/stable/2.3.92182/darwin/TRAE_CN-darwin-arm64.dmg | 2026-10-13 | "version": "3.4.3"；arm64 dmg 479,212,064 B；x64 dmg 515,022,979 B | curl + curl -I（一手） |
| N45 | TRAE 官方 Changelog | https://docs.trae.ai/ide/changelog | 2025-01 至 2026-10-08 | "Upgraded the VS Code kernel to 1.104."（2025-10-14）；"Upgraded the VS Code kernel to 1.100.3."（2025-06-24）；"Upgraded the VS Code kernel to 1.98.2."；"TRAE v3.6.0 is released."（2026-10-08） | curl（一手；主执行者复核一致） |
| N46 | desktopinsights：Trae / Trae CN | https://desktopinsights.com/apps/trae ；https://desktopinsights.com/apps/trae-cn | 约 2 周前扫描 | "Trae is an Electron app (v39.2.7)"；chromiumVersion 142.0.7444.235；装后 1155 / 1169 MB；检出 "VS Code Extension API" | curl（三方扫描） |
| N47 | Homebrew cask：trae | https://github.com/Homebrew/homebrew-cask/blob/master/Casks/t/trae.rb | 2026-10-16 | `uninstall launchctl: "com.trae.ShipIt"`（Squirrel.Mac 更新器，与 VS Code 同类） | gh api（二手） |
| N48 | qoder.com 下载页脚本 | https://g.alicdn.com/Qoder/qoder-web/0.0.315/_next/static/chunks/9774-74dd9d885a1e8d5a.js | 2026-10-16 | 下载链接 `https://download.qoder.com/release/latest/Qoder-IDE-darwin-arm64.dmg` 等 | curl（一手） |
| N49 | HEAD：Qoder IDE dmg | https://download.qoder.com/release/latest/Qoder-IDE-darwin-arm64.dmg ；同目录 `Qoder-IDE-darwin-x64.dmg` | Last-Modified 2026-10-13 | arm64 268,337,406 B；x64 282,110,050 B | curl -I（一手） |
| N50 | Qoder IDE Release Notes | https://docs.qoder.com/release-notes/desktop | 2025-11 至 2026-10-13 | 最新 1.33.0（2026-10-13）；0.6.0（2026-03-10）"Upgraded to VSCode base version 1.106.3."；0.2.19（2025-12-02）"Electron Upgraded to 37.7.0: Upgraded from 34.5.1 to address potential performance lag on macOS 26" | curl + WebFetch（一手；主执行者复核一致） |
| N51 | desktopinsights：Qoder | https://desktopinsights.com/apps/qoder | 约 2 周前扫描 | "Qoder is an Electron app (v42.2.0)"；v1.24.2；chromiumVersion 148.0.7778.97；装后 904 MB；检出 "VS Code Extension API" | curl（三方扫描） |
| N53 | CodeBuddy 国际版更新接口 | https://www.codebuddy.ai/v2/update?platform=ide-darwin-arm64&version=1.0.0&x-machine-id=default | 2026-09-20 | `"version":"4.12.1.39217423"`，字段（url、productVersion、sha256hash、timestamp、supportsFastUpdate）与 VS Code 更新服务一致 | curl（一手） |
| N54 | CodeBuddy CN 更新接口 | https://copilot.tencent.com/v2/update?platform=ide-darwin-arm64&version=1.0.0&x-machine-id=default | 2026-10-13 | `"version":"4.12.2.41207131"` | curl（一手） |
| N55 | HEAD：CodeBuddy zip | https://codebuddy-1328495429.cos.accelerate.myqcloud.com/aiide/darwin-arm64/CodeBuddy-darwin-arm64-4.12.1.39217423-757a5b2f.zip ；https://download.codebuddy.cn/aiide/darwin-arm64/CodeBuddy-darwin-arm64-4.12.2.41207131-425ab70e-cn.zip | 2026-09-20 / 2026-10-13 | 国际版 arm64 221,159,871 B（x64 225,990,256 B）；CN 版 arm64 221,221,340 B | curl -I（一手） |
| N57 | 腾讯云文档：CodeBuddy IDE 更新记录 | https://cloud.tencent.com/document/product/1749/123662 | 2026-09-08 | 4.9.14–4.9.15："同步上游 VSCode 版本至 1.102.3"；"修复 Webview Cmd+F 在标准 Electron 下不可用的问题" | WebFetch（一手） |
| N58 | desktopinsights：CodeBuddy | https://desktopinsights.com/apps/codebuddy | 约 2026-10 | "CodeBuddy is an Electron app (v37.7.0)"；chromiumVersion 138.0.7204.251；装后 741 MB | curl（三方扫描） |
| N60 | 智谱清言 latest-mac.yml | https://sfile.chatglm.cn/apk/xinyu/windows/latest-mac.yml | 2026-10-14 | electron-builder 格式；"version: 2.0.7"；dmg size 226370572；zip size 218284473；Homebrew cask 用 `strategy :electron_builder` | curl + gh api（一手 / 二手） |
| N61 | HEAD：智谱清言 dmg | https://sfile.chatglm.cn/apk/xinyu/windows/chatglm_2.0.7_universal.dmg | Last-Modified 2026-10-15 | 226,372,997 B（比 yml 多 2,425 B，推测生成 yml 后又重签） | curl -I（一手） |
| N62 | desktopinsights：智谱清言 | https://desktopinsights.com/apps/chatglm | 2026-10-16 扫描 | "智谱清言 is an Electron app (v43.6.0)"；装后 900 MB；Universal | curl（三方扫描） |
| N63 | iTunes Lookup：文心 | https://itunes.apple.com/lookup?id=6760705915&country=cn&entity=macSoftware | 2026-09-22 | version "3.3.5"；fileSizeBytes "197624451"；bundleId "com.baidu.wenxin"；首发 2026-03-31 | curl（一手） |

编号有空缺（N3、N4、N18、N31、N32、N39、N44、N52、N56、N59）：分支调研员原始清单里，N3、N4、N39、N44、N56 是重复的 HEAD 记录，已并入 N2、N38、N43、N55；N18、N31、N32、N52、N59 是只有搜索摘要或 Homebrew 提交历史的旁证（Kimi 早期扫描快照、千问 cask 历史、千问与夸克联动、QoderWork 包体、CodeBuddy cask），对结论没有增量，整理时删去。其余条目保留原编号以便对照。

### N 组未能核实或有矛盾的地方

- 千问：官网 dmg 停在 2.3.2（2026-03-03，Electron）；Mac App Store 同一个 bundle id 已到 4.3.5，3.0.0 起加了"智能浏览模式"。3.x/4.x 是否仍是 Electron、是否换成夸克的 Chromium 内核，未核实。
- 豆包：没有官方技术说明。"自研 Chromium"的依据是一手配置接口里的 `chromium_*` 开关、三方扫描到的 "Doubao Browser Framework 147.0.7727.149"、2024 年的评测。没有找到独立的 Mac 版"豆包浏览器"产品，推测浏览器能力就内置在豆包电脑版里。Mac App Store 版（`com.bot.neotix.doubao`）的框架未核实。
- Kimi：框架只有三方扫描（Electron 42.4.1）。2.x 用什么框架没查到，"2.x 到 3.x 重写"是根据 cask 下架、换下载路径、改为只出 arm64、官方说一周写出 Beta 客户端推断的。
- 元宝：Tauri 只有三方扫描、cask 的 WebKit 目录和社区帖子，腾讯没有官方说明。
- Qoder：官方 notes 最后一次提到 Electron 是 37.7.0（2025-12），扫描显示现在是 42.2.0。
- Trae、CodeBuddy 的 Electron 版本只来自扫描；当前的 VS Code 内核版本未核实。
- DeepSeek：聊天应用没有官方桌面端；Harness 是 Agent 工作台，按"DeepSeek 官方桌面端"计入统计时要注意这一区别。
- 文心：只拿到商店版本和大小，框架未核实。
- 装后大小全部来自三方扫描，扫描时间和当前版本不一定对得上。

### N 组主执行者复核

- 重取 N1，`"version": "2.32.5"` 和 `chromium_*` 开关一致；重取 N5 页面，"Doubao Browser Framework" 147.0.7727.149 一致；用 gh 重读 N36 的 package.json，`electron ^44.7.0`、`electron-builder ^26.17.0`、`electron-updater ^6.8.9` 一致；WebFetch 复读 N50，两条 Electron 和 VS Code 基座原话一致；重取 N45，三条 "Upgraded the VS Code kernel" 一致；重取 N21，Tauri 2.2.0 一致。

---

## P：大众热门 App

| 编号 | 标题 | URL | 资料日期 | 要点摘录 | 获取方式 |
|---|---|---|---|---|---|
| P1 | 飞书下载接口 | https://www.feishu.cn/api/downloads | 2026-10-16 | MacOS_m1 "MacOS-Apple@V8.2.14"，release_time 对应 2026-10-12；Intel 同为 V8.2.14 | curl（一手） |
| P2 | HEAD：飞书 8.2.14 dmg | https://sf3-cn.feishucdn.com/obj/ee-appcenter/e47a0484/Feishu-darwin_arm64-8.2.14-signed.dmg ；https://sf3-cn.feishucdn.com/obj/ee-appcenter/f5db63c3/Feishu-darwin_x64-8.2.14-signed.dmg | Last-Modified 2026-10-12 | arm64 717,301,663 B；x64 758,698,331 B | curl -I（一手） |
| P4 | Mac App Store：飞书 | https://itunes.apple.com/search?term=feishu&entity=macSoftware&country=cn&limit=20 | 2026-10-13 | bundleId com.bytedance.macos.feishu，8.1.25，fileSizeBytes 682079391 | curl（一手） |
| P5 | Lark 下载接口与 HEAD | https://www.larksuite.com/api/downloads ；https://lf16-larkversion.larksuitecdn.com/obj/lark-version-sg/9dd05af0/Lark-darwin_arm64-8.1.22-signed.dmg | 2026-09-24 | Lark arm64 8.1.22，dmg 605,124,281 B；Intel 8.1.24 | curl + curl -I（一手） |
| P7 | Mac App Store：Lark | https://itunes.apple.com/search?term=lark&entity=macSoftware&country=us&limit=10 | 2026-10-13 | com.larksuite.macos.lark，8.1.25，fileSizeBytes 637239488 | curl（一手） |
| P8 | desktopinsights：Lark | https://desktopinsights.com/apps/lark | 约 2026-10-09 扫描 | "a cocoa app (v147.0.7727.149)"；装后 1673 MB（Apple Silicon）；组件里有 "Lark Framework"，没有 Electron Framework | WebFetch（三方扫描） |
| P9 | 掘金：我用 GitHub 第一的 REA 拆了飞书，6 分钟实现了飞书多开 | https://juejin.cn/post/7696466781289005056 | 2026-10-15 | "混合架构：一个原生的 macOS 外壳，里面跑着多进程的 Web 环境"；"25 个 ASAR 包"；包标识 `com.electron.lark`，但"不能凭这一个字段就认定飞书是标准 Electron"；"对标准 Electron IPC 的统计结果是 0"；作者的飞书为 8.1.17 | curl（二手） |
| P10 | 个人博客：Electron 笔记 | https://wshibin.github.io/posts/2022/09_07_Electron.html | 2022-09（路径日期） | "Feishu：/Applications/Feishu.app/Contents/Frameworks/Electron Framework.framework + Rust（业务）"；"Dingtalk：node-webkit --> Chromium Embedded Framework --> Native UI + 前端 + fluttter" | gh search code + curl（二手） |
| P11 | GitHub 上提到 `Lark Helper (Iron)` 的三方配置和代码 | https://github.com/saitofun/rules/blob/45ffbad7606339a6977049a557b9ad84003429da/UserDefinedDirect.yaml ；https://github.com/Intuition-Lab/personal-model/blob/HEAD/src/persome/parsers/feishu.py | 2023-05-26 起 | 2023-05-26 的代理规则里已有 "PROCESS-NAME,Lark Helper (Iron)"；另一份代码称 "the meeting renderer process `com.electron.lark.iron`" | gh api（三方） |
| P12 | 三方脚本：feishu-native bootstrap | https://github.com/Ninglo/remotelab/blob/HEAD/scripts/feishu-native/bootstrap.sh | 2026-10-12 | 路径 "Contents/Frameworks/Lark Framework.framework/Versions/147.0.7727.149/Resources/webcontent/messenger.asar"，对应飞书 8.1.17 | gh api（三方） |
| P13 | 新华财经：飞书发布 8.0 | https://www.cnfin.com/gs-lb/detail/20260915/4470235_1.html | 2026-09-15 | "对产品进行系统性重构，以适配智能体（Agent）能力的全面接入"（讲产品层，未提客户端框架） | WebFetch（二手） |
| P14 | HEAD：新版 Teams macOS 安装包 | https://go.microsoft.com/fwlink/?linkid=2249065 → https://statics.teams.cdn.office.net/production-osx/enterprise/webview2/lkg/MicrosoftTeams.pkg | Last-Modified 2026-10-07 | 348,409,843 B；下载路径里带 `webview2` | curl -I（一手） |
| P15 | MS Learn：Teams app versioning | https://learn.microsoft.com/en-us/officeupdates/teams-app-versioning | 2026-10-09 | Mac 新版 Teams："October 07, 2026 \| 26261.303.5165.1465" | WebFetch（一手） |
| P16 | MS Learn：System requirements for the Teams client | https://learn.microsoft.com/en-us/microsoftteams/teams-client-system-requirements | ms.date 2026-07-16 | "Teams client for macOS Desktop" 表格里有一行 "Webview2 \| Update to the most current version."；"up to two times faster app performance while using 50% less memory than previous Teams versions" | WebFetch（一手；主执行者复核一致） |
| P17 | WebView2Feedback #1314 "Mac OSX support" | https://github.com/MicrosoftEdge/WebView2Feedback/issues/1314 | 2022-05-20 / 2024-07-06 | 微软（2022-05-20）："we are actively working on the WebView2 macOS support"；微软（2024-07-06）："we have decided to discontinue plans for a public release of WebView2 on MacOS and Linux" | gh api（一手；主执行者复核 2024-07-06 原话一致） |
| P18 | Microsoft Support KB5025166 | https://support.microsoft.com/en-us/kb/5025166 | 2023-10-05 | 新版 Teams 在 Windows 和 Mac 上 GA；经典版自动升级截止 2024-03-31 | 搜索摘要 |
| P19 | Petri：New Microsoft Teams for Mac Client | https://petri.com/microsoft-teams-for-mac-client/ | 2023-09-01 | "built on the Electron framework"（指经典版）；"Microsoft has now ditched Electron and moved toward the WebView2 technology instead" | WebFetch（二手） |
| P20 | iTunes Lookup：微信 / Slack / Telegram / 钉钉 | https://itunes.apple.com/lookup?id=747648890,803453959,836500024,1435447041&country=us&entity=macSoftware | 2026-10 | 微信 com.tencent.xinWeChat 4.1.15（2026-10-14），fileSizeBytes 488967391 | curl（一手） |
| P21 | HEAD：微信 Mac dmg | https://dldir1v6.qq.com/weixin/Universal/Mac/WeChatMac_4.1.15.dmg | Last-Modified 2026-10-12 | 532,016,523 B | curl -I（一手） |
| P22 | 微信 Mac 官网 | https://mac.weixin.qq.com/ | 2026-10-16 | 版本 "4.1.15"；"适用于 macOS 12 Monterey 及以上版本"；更新说明标题"微信 macOS 版多桌面端统一" | WebFetch + curl（一手） |
| P23 | 微信官方更新日志（Mac） | https://weixin.qq.com/updates?platform=mac | 2026-10-16 | 最早的 4.x 是 "4.0.3 (2025-03-31)"，前一版 "3.8.9 (2024-09-26)" | curl（一手） |
| P24 | IT之家：微信 4.0.0 测试版上架 | https://www.ithome.com/0/806/537.htm | 2024-10-30 | "微信官方在官网上架了最新的 Windows / macOS 微信 4.0.0 测试版" | WebFetch（二手） |
| P25 | IT之家：微信 PC 端 4.0 "转正" | https://www.ithome.com/0/842/153.htm | 2025-03-31 | "4.0.3 版本……实现 Win / Mac 等多端统一" | WebFetch（二手） |
| P26 | WeChatTweak Release 2.0.0 | https://github.com/sunnyyoung/WeChatTweak/releases/tag/2.0.0 | 2025-12-05 | "WeChat 4 采用 QT 重构，丧失了大量的 runtime 特性，导致分析、开发难度陡增" | gh api（二手，开发者说法） |
| P27 | SegmentFault：微信 4.0 架构 | https://segmentfault.com/a/1190000047217890 | 2025-08-26 | "全新采用 QT+C++原生跨平台架构" | WebFetch（二手） |
| P28 | desktopinsights：WeChat | https://desktopinsights.com/apps/wechat | 约 2026-10-13 扫描 | "WeChat is a wxwidgets app (v4.1.15)"（与本机看到的 Qt 矛盾，判为误报）；装后 1973 MB；组件有 "WeChatAppEx Framework"、Sparkle | WebFetch（三方扫描） |
| P29 | 微信开放文档：小程序运行环境 | https://developers.weixin.qq.com/miniprogram/dev/framework/runtime/env.html | 无日期 | "iOS / iPadOS / MacOS \| JavaScriptCore \| WKWebView"（与本机 4.x 的 WeChatAppEx/Chromium 不符，推测文档停留在 3.x） | WebFetch（一手，可能过时） |
| P30 | tdesktop Release v7.3.1 | https://github.com/telegramdesktop/tdesktop/releases/tag/v7.3.1 | 2026-10-10 | td-setup-mac-7.3.1.dmg size 168783211 | gh api（一手） |
| P31 | HEAD：Telegram Desktop dmg | https://telegram.org/dl/desktop/mac → https://td.telegram.org/mac/td-setup-mac-7.3.1.dmg | Last-Modified 2026-10-10 | 168,783,211 B | curl -I（一手） |
| P32 | tdesktop README、qt_version.py、changelog | https://github.com/telegramdesktop/tdesktop | 2021-10-29、2026-05-19 | README："Qt 6 (LGPL) and Qt 5.15 (LGPL) slightly patched"；qt_version.py：macOS 用 Qt 6.11.2；changelog："3.1.10 beta (29.10.21) … Native support for M1 on macOS. Build macOS and Linux versions on Qt 6.2."；"6.8.3 beta (19.05.26) … Upgrade to Qt 6.11.1 on macOS and Linux." | gh api（一手） |
| P33 | iTunes Lookup：Telegram（原生版） | https://itunes.apple.com/lookup?id=747648890&country=us&entity=macSoftware | 2026-10-13 | ru.keepcoder.Telegram，13.0，fileSizeBytes 107225238 | curl（一手） |
| P34 | Telegram macOS Sparkle appcast | https://osx.telegram.org/updates/versions.xml | 2026-10-09 | shortVersionString "13.0"，version "283305"，length "150486795" | curl（一手） |
| P35 | HEAD：Telegram macOS dmg | https://osx.telegram.org/updates/Telegram.dmg | Last-Modified 2026-10-09 | 150,663,741 B | curl -I（一手） |
| P36 | overtake/TelegramSwift、overtake/telegram | https://github.com/overtake/TelegramSwift ；https://github.com/overtake/telegram | 2014-08-19 / 2017-10-10 创建 | TelegramSwift："Source code of Telegram for macos on Swift 5.0"，"the official source code for Telegram for macOS"，含 TGUIKit；overtake/telegram："Source code of deprecated Telegram for macos version"，Objective-C | gh api（一手） |
| P37 | Figma 更新元数据 | https://desktop.figma.com/mac-arm/RELEASE.json | 2026-10-16 | `{"version":"126.9.13", ..., "url":"https://desktop.figma.com/mac-arm/Figma-126.9.13.zip"}` | curl（一手） |
| P38 | HEAD：Figma 安装包 | https://desktop.figma.com/mac-arm/Figma-126.9.13.zip ；https://desktop.figma.com/mac-installer/Figma.dmg | Last-Modified 2026-10-14 | arm64 zip 132,198,500 B；dmg 是 786,370 B 的在线安装器 | curl -I（一手） |
| P39 | Figma Blog：Introducing BrowserView for Electron | https://www.figma.com/blog/introducing-browserview-for-electron/ | 2017-06-29 | "doubled down on the web by going with [Electron]" | WebFetch（一手） |
| P40 | Figma Blog：WebAssembly cut Figma's load time by 3x | https://www.figma.com/blog/webassembly-cut-figmas-load-time-by-3x/ | 2017-06-08 | "our product is written in C++, which can easily be compiled into WebAssembly"；"our load time improved by more than 3x" | WebFetch（一手） |
| P41 | Figma Blog：Figma rendering: Powered by WebGPU | https://www.figma.com/blog/figma-rendering-powered-by-webgpu/ | 2025-09-18 | "Our renderer is written in C++. We compile this C++ code to WebAssembly (Wasm) using Emscripten"；可回退到 WebGL | WebFetch（一手） |
| P42 | desktopinsights：Figma | https://desktopinsights.com/apps/figma | 约 2026-10-09 扫描 | "Figma is an Electron app (v43.7.9)"；装后 491 MB | WebFetch（三方扫描） |
| P43 | HEAD：Discord dmg | https://discord.com/api/download?platform=osx → https://stable.dl2.discordapp.net/apps/osx/0.0.416/Discord.dmg | Last-Modified 2026-10-13 | 215,173,350 B | curl -I（一手） |
| P44 | Discord 更新接口 | https://discord.com/api/updates/stable?platform=osx | 2026-10-13 | `{"name": "0.0.416", "pub_date": "2026-10-13T19:39:54", ...}` | curl（一手） |
| P45 | Discord Patch Notes: September 8, 2026 | https://discord.com/blog/discord-patch-notes-september-8-2026 | 2026-09-08 | "We also upgraded our Electron version to 42 for the Desktop app and changed some of the ways we consume it."；"p50 CPU usage improvements (~17%)"，"at the cost of a smaller regression to p50 memory usage (~4%)" | WebFetch（一手；主执行者复核一致） |
| P46 | Discord Blog：64-bit 升级 | https://discord.com/blog/how-discord-seamlessly-upgraded-millions-of-users-to-64-bit-architecture | 2024-12-13 | "By using a web wrapper called Electron, we can run Discord in a browser-like environment" | WebFetch（一手） |
| P47 | desktopinsights：Discord | https://desktopinsights.com/apps/discord | 约 2026-10-14 扫描 | "Electron v42.11.11"；装后 837 MB | WebFetch（三方扫描） |
| P48 | iTunes Lookup：Slack | https://itunes.apple.com/lookup?id=803453959&country=us&entity=macSoftware | 2026-10-06 | com.tinyspeck.slackmacgap，4.52.178，fileSizeBytes 169219846 | curl（一手） |
| P49 | HEAD：Slack dmg | https://downloads.slack-edge.com/desktop-releases/mac/universal/4.52.178/Slack-4.52.178-macOS.dmg ；https://downloads.slack-edge.com/desktop-releases/mac/arm64/4.52.178/Slack-4.52.178-macOS.dmg | Last-Modified 2026-09-30 | universal 220,726,998 B；arm64 124,496,038 B | curl -I（一手） |
| P50 | slack.engineering：Building Hybrid Applications with Electron | https://slack.engineering/building-hybrid-applications-with-electron/ | 2016-10-25 | "Originally, the Slack desktop application was written using the MacGap v1 framework"；"WebView was tied to the operating system's version of Safari"；"moving to a unified codebase across Mac, Windows, and Linux was an easy choice" | WebFetch（一手） |
| P51 | Slack Mac release notes | https://slack.com/release-notes/mac | 2026-10-05 | 最新 "Slack 4.52.178, 5 October 2026" | WebFetch（一手） |
| P52 | desktopinsights：Slack | https://desktopinsights.com/apps/slack | 约 2026-10-09 扫描 | "Slack is an Electron app (v44.4.5)"；装后 518 MB | WebFetch（三方扫描） |
| P53 | HEAD：Spotify 安装包 | https://download.scdn.co/SpotifyARM64.dmg ；https://download.scdn.co/Spotify.dmg | Last-Modified 2026-10-15 | ARM64 dmg 173,859,661 B；Spotify.dmg 190,843,741 B（架构未标，推测 Intel） | curl -I（一手） |
| P54 | Spotify Engineering：Building the Future of Our Desktop Apps | https://engineering.atspotify.com/2021/4/building-the-future-of-our-desktop-apps | 2021-04-07 | "The Spotify Desktop client is a Windows and Mac native application that uses CEF"；"We decided, then, to use the Web Player as the starting point for a single user experience" | WebFetch（一手） |
| P55 | Spotify Engineering：Building Spotify's New Web Player | https://engineering.atspotify.com/2019/3/building-spotifys-new-web-player | 2019-03-25 | "Spotify started using Chromium Embedded Framework (CEF) in 2011 to embed web views on the desktop application." | WebFetch（一手） |
| P56 | Homebrew cask spotify / dingtalk；desktopinsights：Spotify | https://formulae.brew.sh/api/cask/spotify.json ；https://formulae.brew.sh/api/cask/dingtalk.json ；https://desktopinsights.com/apps/spotify | 2026-10-16 | spotify "1.3.4.258"；dingtalk "9.0.2,59049656"；desktopinsights：Spotify 检出 "Chromium Embedded Framework"，装后 623 MB | curl / WebFetch（二手 / 三方扫描） |
| P57 | obsidian-releases v1.14.4 | https://github.com/obsidianmd/obsidian-releases/releases/tag/v1.14.4 | 2026-10-05 | Obsidian-1.14.4.dmg size 233704463；App 本体 obsidian-1.14.4.asar.gz size 9200670 | gh api（一手） |
| P58 | Obsidian Changelog | https://obsidian.md/changelog/ | 2026-10-05 | "The installer has been updated to use Electron v43.7.5." | WebFetch（一手） |
| P59 | desktopinsights：Obsidian | https://desktopinsights.com/apps/obsidian | 约 2026-10-09 扫描 | "Electron v43.7.7"；装后 920 MB | WebFetch（三方扫描） |
| P60 | Notion 更新源 | https://desktop-release.notion-static.com/latest-mac.yml ；https://desktop-release.notion-static.com/arm64-mac.yml | 2026-10-15 | electron-builder 格式；version 7.39.0；Notion-7.39.0.dmg size 139258596；Notion-7.39.0-arm64.dmg size 132514593 | curl（一手） |
| P61 | HEAD：Notion universal dmg | https://desktop-release.notion-static.com/Notion-7.39.0-universal.dmg | Last-Modified 2026-10-15 | 233,350,224 B（与 R04 C29 一致） | curl -I（一手） |
| P62 | desktopinsights：Notion | https://desktopinsights.com/apps/notion | 2026-10-16 扫描 | "Electron v44.4.4"；v7.39.0；装后 498 MB | WebFetch（三方扫描） |
| P63 | iTunes Lookup：钉钉 | https://itunes.apple.com/lookup?id=1435447041&country=us&entity=macSoftware | 2026-10-14 | com.dingtalk.mac，9.0.2，fileSizeBytes 723198130 | curl（一手） |
| P64 | HEAD：钉钉 dmg | https://dtapp-pub.dingtalk.com/dingtalk-desktop/mac_dmg/Release/DingTalk_v9.0.2-Installer_59049656_universal.dmg | Last-Modified 2026-10-12 | 586,311,750 B | curl -I（一手） |
| P65 | 阿里巴巴终端技术：钉钉 Flutter 跨四端方案设计与技术实践 \| Dutter | https://segmentfault.com/a/1190000041830204 | 2022-05-11 | "Dutter 即 DingTalk Flutter，是钉钉内基于 Flutter 构建的跨四端研发框架"；"客户端主体仍以原 Native 实现为主" | WebFetch（一手技术文章的转载） |
| P66 | desktopinsights：DingTalk | https://desktopinsights.com/apps/dingtalk | 约 2026-10-12 扫描 | "DingTalk is a flutter app (v9.0.2)"；装后 1834 MB；UI 框架一栏还有 Qt；另有 AFNetworking、Sparkle | WebFetch（三方扫描） |
| P67 | HEAD：Zoom 安装包 | https://cdn.zoom.us/prod/7.2.2.88465/zoomusInstallerFull.pkg ；https://cdn.zoom.us/prod/7.2.2.88465/arm64/zoomusInstallerFull.pkg | Last-Modified 2026-09-24 | 完整包 223,001,957 B；arm64 完整包 216,365,102 B（另有 43 MB 的 Zoom.pkg，推测不是完整安装包） | curl -I（一手） |
| P68 | Zoom Developer Docs：Zoom Apps architecture | https://developers.zoom.us/docs/zoom-apps/architecture/ | 无日期 | "For Mac, it's WKWebView, tied to the version that your Safari browser uses." | WebFetch（一手） |
| P69 | desktopinsights：Zoom | https://desktopinsights.com/apps/zoom | 约 2026-10-02 扫描 | runtime "swift"；UI 框架 Qt（置信度 70/100）；v7.2.2 (88465)；装后 560 MB | WebFetch（三方扫描） |
| P70 | codenote.net：A 2026 Audit of Famous Electron Apps（即 R04 C41） | https://codenote.net/en/posts/famous-electron-apps-2026-research/ | 2026-05-04 | 转引 Teams 负责人 2021 年原话："We are moving away from Electron to Edge Webview2." | WebFetch（二手） |

编号有空缺（P3、P6）：原始清单里是重复的 HEAD 记录，已并入 P2、P5。

### P 组未能核实或有矛盾的地方

- 飞书：没有官方或技术文章讲"从 Electron 迁到自研框架"。时间只能框出大概：2022-09 的个人博客还记录了 `Electron Framework.framework`（P10）；`Lark Helper (Iron)` 这个进程名 2023-05-26 已经出现（P11），但三方说它是会议进程，当时可能和 Electron 并存。"Iron"是整个运行时的名字还是会议进程的名字未核实，本机看到的 C++ 命名空间 `iron::`（L1）是唯一把它和整个框架联系起来的线索。迁移原因未知。
- 飞书版本：本机是 7.75.15（Chromium 143），官方当前已是 8.2.14；三方看到 8.1.17 用 Chromium 147.0.7727.149（P8、P12），**与豆包电脑版的 "Doubao Browser Framework" 147.0.7727.149（N5）完全相同**，推测字节内部有一套共用的 Chromium 发行版【推测】。
- 微信：腾讯官方只说"多端统一"，没点名 Qt；"Qt 重构"来自开发者说法（P26、P27）和本机检查（L3）。desktopinsights 判为 wxWidgets（P28），与本机检查矛盾。官方小程序文档写 Mac 用 WKWebView（P29），与本机看到的 Chromium 内核 WeChatAppEx 不符。
- 微信 4.1.15 的日期：官方更新日志写 2026-09-15，Mac App Store 是 2026-10-14，官网 dmg 的 Last-Modified 是 2026-10-12。
- Teams：`MSWebView2.framework`、`Microsoft Edge Framework` 只来自本机检查，没有公开文档；微软已取消 WebView2 for macOS 的公开发布，Teams 用的是内部版本。
- Zoom：三方检出的 Qt（中等置信度）未核实。
- 钉钉：Web 容器用 CEF 还是 WKWebView 不清楚；三方检出的 Qt 未核实；"node-webkit → CEF → 原生 + 前端 + Flutter"只来自个人博客。
- 只有三方来源的数字：Spotify 1.3.4.258，以及 Figma、Slack、Discord、Notion、Obsidian 的 Electron 小版本号（Discord 的大版本 42、Obsidian installer 用的 43.7.5 有官方说明）。Notion 用 Electron 没有官方明文，靠 electron-builder 格式的更新源和三方扫描。

### P 组主执行者复核

- 用 gh 重读 P17 的评论，2024-07-06 的 "discontinue plans for a public release of WebView2 on MacOS and Linux" 一致；WebFetch 复读 P16，macOS 一栏确有 "Webview2" 行，"two times faster … 50% less memory" 一致；WebFetch 复读 P45，Electron 42 和 CPU/内存的原话一致。

---

## X：横向资料

| 编号 | 标题 | URL | 资料日期 | 要点摘录 | 获取方式 |
|---|---|---|---|---|---|
| X1 | desktopinsights：Desktop App Statistics | https://desktopinsights.com/stats | 页面标"Data as of October 2026" | 共跟踪 28,753 个 App（macOS 12,710，Windows 16,043）；"Electron powers 16.2%" of tracked apps；各运行时 App 数：Native (Swift/ObjC) 6,944、.NET 5,049、Electron 4,672、Tauri 1,685、Qt 1,015、Flutter 708、Wails 110、CEF 98；macOS 包体均值/中位数：Native 93.6/18.5 MB、Electron 613.6/527.6 MB（2,194 个）、Tauri 62.6/27.8 MB（917 个）、Qt 454.7/217.2 MB（512 个）、Flutter 226.8/151.6 MB（266 个）、CEF 1,225.3/1,129.2 MB（31 个）；81.2% 的 Electron App 落后最新版（v45）5 个大版本以上 | WebFetch（三方扫描，全量样本，不区分热门与否） |
