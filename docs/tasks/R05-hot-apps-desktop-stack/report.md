# R05 汇报：热门 App 的桌面端技术栈（R04 补充）

- 执行者：子 agent（技术调研员）
- 分支：`research/R05-hot-apps-desktop-stack`
- worktree：`$HOME/.../cupper/.claude/worktrees/agent-aa48aa3dd8d77f4a0`
- 完成日期：2026-10-16

> 编排者存档说明：正文按子 agent 的回复原样保留，只改了"做了什么"中的一句：原文逐一列出了本机安装的是哪几个 App，与 brief 的约束冲突，改为概括写法。表格里证据来源标为"本机 bundle"（L1–L4），这是 brief 允许的写法，予以保留。

## 做了什么

- **本机 bundle 检查**：其中 4 个目标 App 的证据来自本机已安装 bundle 的只读检查（L1–L4）。
- **网络调研**：其余 App 分三路并行查证，G 组是海外 AI，N 组是国内 AI，P 组是大众热门 App，各由一个分支调研员负责。我对每组的关键结论都做了复核，复核记录写在 sources.md 各组末尾。
- **来源登记**：一共 187 条，都登记在 `docs/tasks/R05-hot-apps-desktop-stack/sources.md`。其中本机 4 条（L1–L4），网络 183 条（G 61、N 53、P 68、X 1）。每条都写明日期、获取方式，以及属于一手、二手、三方扫描还是搜索摘要。
- **引用写法**：下文用方括号引用来源编号。"R04 A1"指 R04 sources.md 的编号。标【推测】的结论只有间接证据。

### 核心结论

1. **热门 App 里，Electron 仍占一半**：已知框架的 32 个 App 中有 16 个。另有 9 个也用 Web 技术渲染主界面，只是不用官方 Electron：
   - 6 个是 Chromium 系非 Electron：自研 Chromium 运行时、CEF，或 Chromium 浏览器；
   - 2 个是原生外壳加 Web 主界面；
   - 1 个是 Tauri。

   合计 25/32（78%）。纯原生 4 个，Qt 2 个，原生加 Flutter 1 个。
2. **发现了两个新的"脱离 Electron 但保留 Web UI"案例，都已在本机 bundle 上核实。**
   - **飞书**：bundle id 仍是 `com.electron.lark`，但运行时换成了字节自己的 `Lark Framework`（Chromium 143，C++ 命名空间 `iron::`，不带 Node.js）。前端是按业务拆分的 asar，核心库是 Rust [L1]。
   - **新版 Teams**：外壳是 SwiftUI 加 AppKit，里面用微软内部的 macOS 版 WebView2，并自带一份 Edge 154（853 MB）。微软 2024 年已经取消了 WebView2 for macOS 的公开发布 [L2][P16][P17]。

   加上 R04 的 Codex（OWL），说明大公司"超出 Electron"之后，做法是自己掌握 Chromium，而不是改用系统的 WKWebView。
3. **字节的两款产品，Chromium 版本完全相同。** 豆包电脑版的 "Doubao Browser Framework" 和飞书 8.x 的 Lark Framework 都是 Chromium 147.0.7727.149 [N5][P8][P12]。推测字节内部有一套共用的 Chromium 发行版【推测】。
4. **AI 新产品分成两派。**
   - **Electron 派**：创业公司和国内厂商，包括 Manus、Granola、Poe、Grok Bot、Kimi、DeepSeek Harness、千问、智谱清言。三家国内大厂的 AI IDE（Trae、Qoder、CodeBuddy）也都是 VS Code fork 加 Electron。
   - **原生派**：美国平台大厂，包括 Gemini、Perplexity 新版、Copilot，都是原生 Swift 外壳。不过它们都带有 WebKit 或 web view 的痕迹，Gemini 还复用了转译过来的 Android 代码。
   - **AI 浏览器**一律基于 Chromium。其中 Atlas 已于 2026-08-09 关停，能力并回 ChatGPT 桌面 App。

### 一、App 技术栈表

包体：dmg、zip、pkg 的大小来自 HEAD 请求、更新源（latest-mac.yml、appcast）或 Release API，单位是十进制 MB；"MAS"是 Mac App Store 的 `fileSizeBytes`；"装后"来自本机 `du -sh` 或三方扫描。完整字节数和 URL 见 sources.md。

| 类别 | App | 框架 | 渲染方式 | 版本或时间 | 包体 | 迁移史 | 证据 | 核实 |
|---|---|---|---|---|---|---|---|---|
| 海外 AI | Manus Studio（原 Manus Desktop） | Electron 43.6（三方扫描）、electron-builder；React/Next.js，有 Rust 原生模块 | Chromium | 2.0.7（2026-10-12）；只支持 Apple Silicon | dmg 319.7 MB；装后 914 MB（三方） | 2026-09 随 Manus 2.0 改名为 Studio，没有框架迁移的证据；浏览器能力做成了 Chrome/Edge 扩展 | G1–G9 | 已核实（electron-builder 更新源加三方扫描） |
| 海外 AI | Perplexity Mac App（新版） | 原生 Swift 加 Rust 组件，用 Sparkle 更新 | 原生；局部 WKWebView【推测】 | 26.40.0（2026-10-14）；首发 2026-05-07；要求 macOS 15+ | dmg 211.0 MB；装后 847 MB（三方） | 2024-10 的商店原生版于 2026-05 被全新原生版取代，新版改为官网直接下载 | G10–G17 | 部分核实（Swift 只有三方扫描） |
| 海外 AI | Comet（Perplexity 的浏览器） | Chromium 分支 | Chromium | 145.2.7632.4581，对应 Chromium M145；2025-07 发布 | 未取得（下载端点拒绝 HEAD） | 无 | G18–G22 | 已核实是 Chromium；包体未取得 |
| 海外 AI | Gemini（Google） | 原生 Swift（官方说"100% native Swift"，用 Antigravity 几天做出原型），里面有转译的 Android 共享代码 | 原生；局部 WKWebView【推测】 | 1.128.14.1166（2026-10-14）；首发 2026-04-15；macOS 15+，只支持 Apple Silicon | dmg 238.0 MB；不上 Mac App Store | 首个桌面端 | G23–G28 | 部分核实（有官方表态；用 SwiftUI 还是 AppKit 未核实） |
| 海外 AI | ChatGPT Atlas | SwiftUI 加 AppKit 外壳，Chromium 通过 OWL 在独立进程里运行 | Chromium 加原生 UI | 最终版 1.2026.189.1（2026-07-24）；**2026-08-09 已关停** | dmg 270.0 MB | 浏览器能力并入 ChatGPT 桌面 App 的内置浏览器（2026-07-09，也就是 R04 L1 里的 Codex/OWL 外壳），以及 Chrome 扩展 | G29–G35、R04 A1 | 已核实（关停日期来自二手资料） |
| 海外 AI | Genspark | 浏览器是 Chromium 分支（推测基于 Brave 代码线）；GenOffice 用 Electron 43.7；Claw 桌面端框架不明 | Chromium | 浏览器 2025-06 发布；GenOffice 0.11.829（2026-10-15） | 浏览器旧版 1.1.302 的 dmg 147.3 MB；GenOffice arm64 dmg 192.0 MB | 无 | G36–G41 | 浏览器部分核实；GenOffice 已核实；Claw 未核实 |
| 海外 AI | Grok（xAI） | **没有官方 Mac 聊天 App**；agent App Grok Bot 由 SpaceXAI 和 Cursor 合做，是 Electron | Chromium | Grok Bot 0.71.2（2026-10-14），2026-08 发布 beta | Grok Bot arm64 dmg 147.0 MB | 无 | G42–G48 | 没有聊天 App 已核实；Grok Bot 是 Electron 只有社区移植仓库为证，部分核实 |
| 海外 AI | Microsoft Copilot | 原生外壳（招聘要求 Swift、SwiftUI、AppKit），主界面像 web view | 原生加 Web【推测】 | 25.7.440902001（2026-09-03）；macOS 14+，只支持 Apple Silicon | MAS 128.9 MB | 2024 年是 iPad 版跑在 Mac 上，2025-02 换成独立的 Mac App | G49–G53 | 部分核实（没有官方的框架说明） |
| 海外 AI | Poe | Electron 37.2（三方扫描）；bundle 名是 `com.quora.poe.electron` | Chromium | 1.1.45（2026-06-19） | arm64 更新 zip 108.9 MB；装后 420 MB（三方） | 无 | G54–G58 | 已核实 |
| 海外 AI | Granola | Electron 44.4（三方扫描）、electron-builder、React；招聘写 "ideally Electron" | Chromium | 7.663.2（2026-10-15） | universal dmg 306.3 MB；装后 1074 MB（三方） | 无 | G59–G62 | 已核实 |
| 国内 AI | 豆包电脑版（字节） | 自研 Chromium 运行时 "Doubao Browser Framework"（Chromium 147.0.7727.149），加 Rust 组件；不是 Electron | Chromium | 2.32.5（2026-10-15）；商店版是另一条版本线，2.24.10 | universal dmg 791.9 MB，arm64 592.3 MB；MAS 438.7 MB；装后 1658 MB（三方） | 2024-08 已经是 Chrome 内核；浏览器能力内置在电脑版里，没有找到独立的 Mac 版豆包浏览器 | N1–N9 | 部分核实（一手配置里有大量 `chromium_*` 开关；内核版本来自三方扫描） |
| 国内 AI | Kimi | Electron 42.4（三方扫描，扫描时是 v3.0.23） | Chromium | 3.2.17（2026-10-15）；只支持 Apple Silicon | dmg 379.3 MB；装后 1900 MB（三方） | 疑似重写：2.x 在 2025-12 下架，2026-05 换成 3.x；官方说 Kimi Work 客户端是用 Coding Agent "一周内"写出来的 | N10–N17 | 部分核实（框架只有三方扫描） |
| 国内 AI | 腾讯元宝 | Tauri 2（三方扫描是 2.2.0） | WKWebView | 2.88.1.610（2026-10-10） | universal dmg 102.2 MB；装后 439 MB（三方） | 无 | N19–N23 | 部分核实（三方扫描、WebKit 数据目录、社区帖子） |
| 国内 AI | 千问（原"通义"，阿里） | 官网包是 Electron 33 加 electron-builder | Chromium | 官网包 2.3.2（2026-03-03）；商店版 4.3.5（2026-09-23） | 官网 dmg 210.0 MB；MAS 287.2 MB；装后 820 MB（三方） | 2025-11 改名为千问；商店版从 3.0 起加了"智能浏览模式"，框架未核实 | N24–N30 | 官网包已核实；商店版未核实 |
| 国内 AI | DeepSeek | **没有官方桌面聊天 App**；官方 agent 工作台 DeepSeek Harness（预览版）是"Electron shell around the complete dsh Web application"（Electron 44） | Chromium | Harness 0.2.0-rc.2（2026-09-29） | Harness arm64 dmg 369.8 MB | 2025-02 官方通报说还没有 Mac 客户端 | N33–N40 | 已核实（官方开源仓库） |
| 国内 AI | Trae（字节，国际版和国内版） | VS Code fork 加 Electron（三方扫描 39.2） | Chromium | 国际版 3.6.2，国内版 3.4.3（2026-10） | arm64 dmg 449.6 / 479.2 MB；装后约 1.16 GB（三方） | 一直跟进 VS Code 内核，从 1.97 升到 1.104 | N41–N47 | 已核实（官方 changelog）；Electron 版本只有三方扫描 |
| 国内 AI | Qoder（阿里） | VS Code fork（基座 1.106.3）加 Electron；官方写过 34.5 升 37.7，三方扫描显示现在是 42.2 | Chromium | 1.33.0（2026-10-13） | arm64 dmg 268.3 MB；装后 904 MB（三方） | 2025-12 把 Electron 从 34 升到 37，原话是 "address potential performance lag on macOS 26" | N48–N51 | 已核实（官方 release notes） |
| 国内 AI | CodeBuddy IDE（腾讯） | VS Code fork（上游 1.102.3）加 Electron 37.7（三方扫描） | Chromium | 国际版 4.12.1，国内版 4.12.2（2026-09/10） | arm64 zip 221.2 MB；装后 741 MB（三方） | 一直同步上游 VS Code | N53–N58 | 已核实（官方文档） |
| 国内 AI | 智谱清言 | Electron 43.6（三方扫描）加 electron-builder | Chromium | 2.0.7（2026-10-14） | universal dmg 226.4 MB；装后 900 MB（三方） | 无 | N60–N62 | 已核实 |
| 国内 AI | 文心（百度） | 未核实 | 未核实 | 商店版 3.3.5（2026-09-22），2026-03 首发 | MAS 197.6 MB | — | N63 | 只核实了版本和包体 |
| 大众 | Figma | Electron（三方扫描 43.7）；编辑器核心是 C++，编译成 WebAssembly | Chromium；画布用 WebGPU，可以回退到 WebGL | 126.9.13（2026-10-14） | arm64 zip 132.2 MB；装后 491 MB（三方） | 外壳一直是 Electron；渲染层 2017 年改用 Wasm，2025-09 改用 WebGPU | P37–P42 | 已核实 |
| 大众 | Discord | Electron 42（官方 patch notes） | Chromium | 0.0.416（2026-10-13） | dmg 215.2 MB；装后 837 MB（三方） | 没换框架；2026-09 升到 Electron 42，p50 CPU 降约 17%，内存升约 4% | P43–P47、R04 C39 | 已核实 |
| 大众 | Slack | Electron（三方扫描 44.4） | Chromium | 4.52.178（2026-10-05） | universal dmg 220.7 MB，arm64 124.5 MB；MAS 169.2 MB | 2016 年从 MacGap（系统 WebView）换到 Electron，为了统一三个平台的代码；2019 年在 Electron 内重写 | P48–P52、R04 C38 | 已核实 |
| 大众 | Spotify | 原生 C++ 外壳加 CEF | Chromium（CEF） | 1.3.4.258（版本号只有三方来源）；dmg 2026-10-15 | arm64 dmg 173.9 MB；装后 623 MB（三方） | 2011 年起用 CEF；2021 年起 UI 统一到 Web Player 的代码 | P53–P56 | 框架已核实 |
| 大众 | Obsidian | Electron 43.7（官方 changelog） | Chromium | 1.14.4（2026-10-05） | dmg 233.7 MB，其中 App 本体 asar.gz 只有 9.2 MB；装后 920 MB（三方） | 无 | P57–P59 | 已核实 |
| 大众 | Notion（主 App） | Electron 44.4（三方扫描），更新源是 electron-builder 格式 | Chromium | 7.39.0（2026-10-15） | universal dmg 233.4 MB，arm64 132.5 MB；装后 498 MB（三方） | 无 | P60–P62、R04 C29 | 已核实（官方没有明文） |
| 大众 | Telegram for macOS（原生版） | 原生 Swift（AppKit 加自研 TGUIKit），官方开源 | 原生 | 13.0（2026-10-09） | dmg 150.7 MB；MAS 107.2 MB | 原来是 Objective-C 版，2017 年起用 Swift 重写，始终是原生 | P33–P36 | 已核实 |
| 大众 | Telegram Desktop（Qt 版） | Qt 6（C++，静态链接），官方开源 | Qt 自绘 | 7.3.1（2026-10-10）；本机是 7.1.3，Qt 6.11.1 | dmg 168.8 MB；本机装后 248 MB | 2021-10 从 Qt 5 升到 Qt 6；2026-05 升到 Qt 6.11 | L4、P30–P32 | 已核实 |
| 大众 | 微信 Mac 4.x | Qt 5.15 Widgets（C++）；另外内嵌一个基于 Chromium 144 的 WeChatAppEx（XWeb），跑小程序和网页 | Qt 自绘加 Chromium | 4.1.15（2026-10） | dmg 532.0 MB；MAS 489.0 MB；本机装后 1.4 GB | 3.x 是原生 Objective-C（最后一版 3.8.9，2024-09）；4.0 跨平台重构，测试版 2024-10-30，正式版 2025-03-31，官方说法是"多端统一" | L3、P20–P29 | 部分核实（时间线有官方来源；Qt 来自本机 bundle 和开发者说法，腾讯没有点名） |
| 大众 | 飞书 / Lark | 自研 Chromium 运行时 `Lark Framework`（内部名可能叫 Iron；7.75 是 Chromium 143，8.x 是 147），加按业务拆分的 asar 前端和 Rust 核心；不带 Node.js | Chromium | 飞书 8.2.14（2026-10-12）；本机是 7.75.15 | 飞书 arm64 dmg 717.3 MB；Lark arm64 dmg 605.1 MB；本机装后 1.5 GB | 2022-09 还是 Electron（个人博客），现在是自研运行时，bundle id 没改；迁移时间和原因都没有官方说明 | L1、P1–P13 | 现状已核实（本机 bundle）；迁移史【推测】 |
| 大众 | 钉钉 | 原生宿主加 Flutter 模块（官方 2022 年的 Dutter 文章）；三方扫描判为 Flutter，还检出了 Qt | 原生加 Flutter 自绘；Web 容器用什么内核不清楚 | 9.0.2（2026-10） | universal dmg 586.3 MB；MAS 723.2 MB；装后 1834 MB（三方） | 二手资料说经历过 node-webkit → CEF → 原生加 Flutter，未核实 | P63–P66 | 部分核实 |
| 大众 | Zoom | 原生（三方扫描 runtime 是 Swift，疑似含 Qt）；Zoom Apps 用 WKWebView | 原生加 WKWebView | 7.2.2（2026-09-24） | 完整 pkg 223.0 MB；装后 560 MB（三方） | 没发现迁移 | P67–P69 | 部分核实 |
| 大众 | Microsoft Teams（新版） | 原生外壳（SwiftUI 加 AppKit）加微软内部的 macOS 版 WebView2，自带 Edge 154（Chromium） | Chromium（Edge） | 26261.303.5165.1465（2026-10-07） | pkg 348.4 MB；本机装后 1.1 GB（Edge 占 853 MB） | 经典版是 Electron，新版换成 WebView2，2023-10 在 Windows 和 Mac 上 GA；WebView2 for macOS 的公开发布 2024-07 被取消 | L2、P14–P19、R04 C35 | 已核实 |

### 二、按框架分组的统计，以及与 R04 合并后的分布

**计数口径**

- **一个 App 计一次**。Telegram 的两个官方客户端分开计。
- **特殊归类**：
  - Grok 按 Grok Bot 计；
  - DeepSeek 按 Harness 计；
  - Genspark 按浏览器计。
- **不计入**：文心，因为框架未核实。R05 有效样本 32 个。
- **R04 样本**：取 R04 报告第一节三张表里的 41 个产品。21 个 ACP 和多 agent 客户端拆开逐个计；Poolside 框架未知，不计。

**分类定义**

- **Chromium 系非 Electron**，有三种：
  - 自研 Chromium 运行时跑 Web UI：Codex OWL、飞书、豆包；
  - CEF：Spotify；
  - Chromium 浏览器：Comet、Genspark、Atlas、Arc、Dia。
- **原生外壳加 Web 主界面**：Teams（WebView2）、Copilot【推测】、Raycast（WKWebView）。
- **原生**：Swift/ObjC 写的 App，可以带局部 WebView。
- **自研 GPU UI 和其他自绘**：GPUI、WarpUI、JVM 加 Skia、egui。

| 框架组 | R05 热门 App（32） | R04 样本（41） | 合并（73） | 合并占比 |
|---|---|---|---|---|
| Electron（含 VS Code fork） | 16 | 19 | 35 | 47.9% |
| Chromium 系非 Electron | 6（豆包、飞书、Spotify、Comet、Genspark、Atlas） | 3（Codex OWL、Arc、Dia） | 9 | 12.3% |
| 原生外壳加 Web 主界面 | 2（Teams、Copilot） | 1（Raycast） | 3 | 4.1% |
| Tauri | 1（元宝） | 8 | 9 | 12.3% |
| 原生（Swift/ObjC） | 4（Perplexity、Gemini、Telegram for macOS、Zoom） | 1（Ghostty） | 5 | 6.8% |
| Qt | 2（微信、Telegram Desktop） | 0 | 2 | 2.7% |
| Flutter（含原生加 Flutter） | 1（钉钉） | 1（AcpAgentClient） | 2 | 2.7% |
| 自研 GPU UI 和其他自绘 | 0 | 8（Zed、Delta、Warp、3 个 GPUI 客户端、JetBrains Air、oxi） | 8 | 11.0% |

**汇总口径**

- 用 Web 技术渲染主界面的（前四行）：R05 25/32（78%），R04 31/41（76%），合并 56/73（77%）。这一行里有 5 个浏览器，它们自己的界面不一定是 Web；去掉后合并为 51/68（75%）。
- 引擎是 Chromium 的（Electron、Chromium 系、Teams）：R05 23/32（72%），R04 22/41（54%），合并 45/73（62%）。

**R05 按类别细分**

- 海外 AI（10 个）：Electron 4，Chromium 浏览器 3，原生 2，原生外壳加 Web 1。
- 国内 AI（9 个）：Electron 7（其中 3 个 VS Code fork），自研 Chromium 1，Tauri 1。
- 大众（13 个）：Electron 5，自研 Chromium 1，CEF 1，原生外壳加 Web 1，原生 2，Qt 2，原生加 Flutter 1。

**全量参照** [X1]：desktopinsights 跟踪了 28,753 个 App，其中 Electron 只占 16.2%。macOS 上数量最多的是原生（6,944 个），装后大小的中位数只有 18.5 MB。Electron App 的中位数是 527.6 MB，Tauri 是 27.8 MB。所以 Electron 在热门 App 里约占 50%，远高于它在长尾 App 里的比例。

### 三、补充规律

1. **AI 新产品：创业公司和国内厂商默认用 Electron，美国平台大厂用"原生 Swift 外壳"。与 R04 规律 1、9 基本一致，但规律 9 出现了反例。**
   - Electron 派：Manus、Granola、Poe、Grok Bot、Kimi 3、DeepSeek Harness、千问、智谱清言，版本集中在 42–44。DeepSeek 的原话是 "Electron shell around the complete dsh Web application"；Kimi 说新客户端是用 Coding Agent "一周内"写出来的 [N16][N36]。
   - 原生派：Gemini、Perplexity、Copilot 都是 Swift 外壳，而且都只支持 Apple Silicon，要求 macOS 14 或 15 以上。但它们都还有 WebKit 或 web view 的痕迹，Gemini 还内嵌了转译过来的 Android 共享代码 [G24][G52]。
   - **规律 9 的反例**：Gemini 的原生 Swift 原型是用 AI 工具 Antigravity "几天"做出来的 [G24]。这说明对有平台团队的公司来说，AI 编码不再只偏向 Web/TS。
   - OpenAI 走的是相反方向：原生版先换成了 Codex 的 Electron 编程模型（R04），基于 SwiftUI 和 OWL 的 Atlas 不到 10 个月就关停了 [G34]。
2. **国内大厂：各自掌握 Chromium，或者 fork VS Code，基本不用系统 WebView。**
   - 字节：豆包和飞书都是自研 Chromium 运行时，内核版本相同【推测共用】。
   - 三家大厂的 AI IDE 都是 VS Code fork 加 Electron，并且在官方 changelog 里公开写明跟进的上游版本 [N45][N50][N57]。
   - 超级 IM 走 C++ 跨平台路线：微信用 Qt 5.15，钉钉用原生加 Flutter，再内嵌 Chromium 跑小程序。
   - 唯一的例外是元宝：Tauri 加 WKWebView，universal dmg 只有 102 MB，是大厂产品里最小的。
   - 大厂产品普遍不在意包体：豆包 592–792 MB，飞书 717 MB，钉钉 586 MB，微信 532 MB。
3. **"超出 Electron"的大公司，换的是运行时，没换 Web UI。这是 R04 规律 2 的三个新例子，同时印证规律 3。**
   - 飞书从 Electron 换成自研 Chromium，去掉了 Node.js，但保留了 asar 和 `com.electron.lark` [L1]。
   - Teams 从 Electron 换成 WebView2，在 Mac 上自带一整份 Edge，而不是用 WKWebView [L2]。
   - 豆包和 Spotify（CEF）走的是同一类路线。
   - 32 个热门 App 里，没有一个大厂产品把 Web 主界面放进 WKWebView（元宝除外）。微软甚至宁可在 Mac 上自带 853 MB 的 Edge。这和 opencode 因为 WebKit 不一致而换回 Electron 的理由相同（R04 B15）。
4. **留在 Electron 的老牌产品没有一个离开，都在框架内优化。与 R04 规律 4 和"越成熟越偏 Electron"一致。**
   - Slack、Discord、Figma、Notion、Obsidian 都跟进到了 Electron 42–44。Discord 升级到 42 后，p50 CPU 降了约 17% [P45]。
   - Figma 的画布是 C++ 编译成 Wasm，再用 WebGPU 渲染 [P41]。它说明专业级图形性能也可以在 Electron 里解决，不用换外壳。
   - Slack 当年正是为了摆脱系统 WebView 才换到 Electron [P50]。
5. **R04 的样本放大了 Tauri 和自绘 UI 的比重。**
   - 在 R04 里，Tauri 和自研 GPU UI 各占约 20%，但主要来自开源的小型 ACP 客户端。
   - 在 32 个热门商业 App 里，Tauri 只有元宝 1 个，GPUI 和其他自绘为 0。
   - 热门 App 真正的分界线在三条路线之间：Electron、自持 Chromium，以及原生外壳加 Web 或原生。
   - 不用 Web 的跨平台方案选的是 Qt 或 Flutter（微信、Telegram Desktop、钉钉），不是 SwiftUI。纯原生只出现在单平台专属客户端（Telegram for macOS），或有专门 Apple 团队的大厂。

## 关键取舍

- **分三路并行调研**：G、N、P 各一个分支调研员，我负责本机 bundle、横向资料、复核和综合。这样一天内能覆盖 33 个 App，代价是各组二手资料的口径不完全一致。我对每组抽了 5–6 个关键结论重新取证，都一致；复核记录写在 sources.md 各组末尾。分支调研员的原始清单里有重复的 HEAD 记录和没有增量的旁证，整理时合并或删去了，所以编号有空缺，每组都有说明。
- **本机 bundle 的边界**：按 brief 的允许范围，用了 `ls`、`du`、`plutil`、`otool -L`、`lipo`、`grep -a -o`，以及一个只读的 asar 头部解析脚本（只看了飞书 `main-window.asar` 的条目名）。没有运行 App，没有读用户数据目录，没有解包。分支调研员没有碰本机。
- **统计口径**：
  - Grok 计 Grok Bot，DeepSeek 计 Harness，这两个都不是传统意义上的"聊天桌面端"，表里已写明；
  - 浏览器归入"Chromium 系非 Electron"，所以另给了去掉浏览器的数字；
  - Copilot、Kimi、元宝、豆包这几项只有部分证据，仍按现有最可能的结论计入，表里标了核实程度。
- **没有给 ADR-03 下结论**：按 brief，只提供补充规律。

## 验证方式

对照 brief 的验收标准：

- [x] **一张表，每个 App 一行**：第一节，共 33 行，覆盖 brief 列出的全部 App。国内"等"一项另外补了智谱清言和文心。Telegram 分成两个官方客户端，Genspark 和 Grok 的附属产品写在同一行里。列为：类别、App、框架、渲染方式、版本或时间、包体、迁移史、证据、是否核实。
- [x] **按框架分组的统计，并与 R04 合并**：第二节，8 个框架组。R05 32 个加 R04 41 个，合并为 73 个，计数口径已写明。
- [x] **3 到 5 条补充规律**：第三节，共 5 条，重点写了 AI 新产品和国内大厂的选型倾向，每条都注明与 R04 规律是否一致。
- [x] **sources.md 每条来源都有日期和获取方式**：本机 4 条，网络 183 条。

约束遵守情况：

- **不运行、不下载**：没有运行任何 App；没有下载或运行任何安装包。包体只来自 HEAD、更新源、Release API、iTunes Lookup 和本机 `du`。下载过的只有 yml、appcast、版本 JSON 这类小的元数据文本。
- **外发请求不带个人信息**：curl 一律带 `-A cupper-research`。版本接口要求机器 ID 时，用的是 `default`、`0` 这类占位值。GitHub 只用本机已登录的 `gh` 做只读 GET；请求内容里没有个人信息，但认证头会让 GitHub 知道是哪个账号在查（与 R04 相同），这一点已写在 sources.md 开头。
- **仓库里没有个人信息，也没有本机 App 清单**：对 sources.md 做了个人信息和本机路径检查，没有结果。本机路径一律写成 `/Applications/<App>.app`。仓库里没有列出本机装了哪些 App，只写了被调研 App 的结论，以及证据来自"本机 bundle"。
- **分支状态**：`git diff --stat main...HEAD` 只显示新增了 `docs/tasks/R05-hot-apps-desktop-stack/sources.md`（343 行）。工作区干净，没有 merge、rebase 或 push，没有改 main，也没有创建 `report.md`。

## 提交列表

```
2f99545 docs(task): add R05 hot-app desktop stack source notes
<编排者存档提交>
```

## 遗留问题 / 范围外发现

1. **飞书的迁移史没有官方说明。**
   - 时间只能框出大概：2022-09 还是 Electron（个人博客），`Lark Helper (Iron)` 这个进程名 2023-05 已经出现。
   - "Iron"是整个运行时的名字，还是会议进程的名字，没有核实；原因也不知道。
   - 本机飞书是 7.75.15，比当前的 8.2.14 旧，L1 的数字只代表本机版本。本机 Telegram Desktop 7.1.3 也比当前的 7.3.1 旧。
2. **字节两款产品共用一个 Chromium 版本（147.0.7727.149）只是推测**，依据是三方扫描和三方脚本里的路径。飞书 bundle 里还内置了 `doubao-page`、`doubao-worklet`。
3. **几项框架只有间接证据，或者未核实。**
   - 只有间接证据：Kimi 只有三方扫描；元宝只有三方扫描加社区帖子；Copilot 只有评测观察和招聘描述，也不能排除是 Catalyst；Grok Bot 只有社区移植仓库；Perplexity 新版的 Swift 只有三方扫描；钉钉和 Zoom 里的 Qt 只有三方中等置信度的判断。
   - 未核实：千问商店版 3.x/4.x 的框架；文心的框架；Genspark Claw 的框架；Comet 的包体。
4. **三方扫描有误报。** desktopinsights 把微信和元宝判成了 wxWidgets，与本机检查和其他证据矛盾。三方扫描只能作旁证，表里只采用了它的版本号和装后大小。
5. **范围外，和 ADR-03 有关：Electron 在 macOS 26 上有性能问题。** Qoder 的官方 release notes 说，把 Electron 从 34.5.1 升到 37.7.0 是为了 "address potential performance lag on macOS 26" [N50]。如果 ADR-03 选 Electron，可以把"最低 Electron 版本要绕开 macOS 26 的已知卡顿"写进去，具体问题需要另查。
6. **范围外，和分发有关：agent 类产品在离开 Mac App Store。** Perplexity 新版放弃商店改为直接下载，Gemini、Manus、Granola、Grok Bot 都不上商店，只有 Copilot 还在。推测原因是 agent 需要沙盒外的权限，没有官方表态。cupper 也需要操作本地文件和进程，设计阶段可以确认分发渠道。
7. **范围外，可以补进 R01 和 ADR-03 的背景。** 2026-07-09 起，ChatGPT 桌面 App（也就是 R04 L1 的 Codex/OWL 外壳）接手了 Atlas 的浏览器能力，Atlas 在 08-09 关停 [G34][G35]。Codex 外壳现在既是编码 agent，也是浏览器宿主。
8. **R04 的勘误维持不变。** R05 没有发现与 R04 结论冲突的地方，只在规律 9 上补了一个反例（Gemini）。
