# R04 汇报：桌面客户端框架调研

- 执行者：子 agent（技术调研员，桌面客户端方向）
- 分支：`research/R04-desktop-framework-survey`
- worktree：`$HOME/.../cupper/.claude/worktrees/agent-acc749773d4695e09`（编排者存档时去掉了本机用户名）
- 完成日期：2026-10-16

## 做了什么

调研分五路并行，各由一个分支调研员负责：编码 agent 客户端（A）、ACP 与多 agent 客户端（B）、参照产品与迁移案例（C）、Rust 原生 UI（D）、Web 外壳与其他跨平台框架（E）。另外两块由我直接完成：只读检查本机 `/Applications` 下 12 个 App 的 bundle 结构（L），以及横向资料（X）。

- 来源一共 200 条，都登记在 `docs/tasks/R04-desktop-framework-survey/sources.md`。每条写明了日期、获取方式，以及属于一手资料、二手资料还是只看了搜索摘要。
- 下文用方括号引用来源编号，例如 [L1]、[B15]。"上轮 Sx"指 `docs/meetings/2026-10-16-tech-selection/desktop-engineer.md` 的来源编号。
- 标【推测】的内容来自第三方，或者由间接证据推断；标【判断】的是工程判断，没有外部数据支撑。

### 核心结论

1. **Codex 在 macOS 上已经不带 Electron 运行时了，但它的 UI 代码仍然按 Electron 写。**
   - 本机 ChatGPT.app 26.1007 里没有 `Electron Framework.framework`。取而代之的是 `Codex Framework.framework`，也就是 Chromium 155；主类是 `BrowserCrApplication`；二进制里有一组 `OwlElectron*` 类，配置文件写着 `"runtimeName": "owl"` [L1]。
   - asar 里的 `package.json` 仍是 `openai-codex-electron`，开发依赖是 Electron 42.3.0 和 Forge 7.11 [L1]。
   - OWL 是 OpenAI 为 Atlas 浏览器自研的 Chromium 集成层 [A1]。招聘描述称它是"Electron-like application framework" [A3]。
   - 换运行时的时间大约在 2026 年 5 月到 7 月之间，官方没有解释原因【推测】[A2][R01 T2]。
   - 前端资源的文件名里能看到 mermaid、Pierre 系 diff 主题和编辑器、Shiki、xterm、KaTeX [L1]。这和 ADR-03 初版的组件清单基本一致。
2. **同类"agent 指挥中心"绝大多数用 Web 技术，越成熟、用户越多的越偏 Electron。**
   - 主流产品几乎全是 Web 技术：Codex（OWL）、Claude、Cursor、Devin Desktop（原 Windsurf）、GitHub Desktop、Goose、opencode、AionUi、Happy、Paseo、Lody 都是 Electron 系；Conductor、CodexMonitor、Codeg，以及 Copilot app【推测】是 Tauri。
   - 21 个 ACP 和多 agent 客户端样本里，Electron 9 个、Tauri 7 个、GPUI 3 个，egui 和 Flutter 各 1 个。开源项目按 star 数排前 6 名的都是 Electron [B]。
   - 例外有三类：Zed 自己的 agent 客户端 Delta 用 GPUI 加 ACP [L9]；JetBrains Air 是 JVM 加 Skia 自绘 [A30–A33]；另外是几个小型 GPUI 客户端。
3. **确实有人因为 WKWebView 从 Tauri 换回了 Electron，原因写得很明白。**
   - opencode 在 2026 年 3 月到 5 月间完成迁移。理由有三条：WebKit 渲染性能不如 Chromium，各平台表现不一致，打包的 sidecar 不稳定 [B15]。
   - Happy 也从 Tauri 换到了 Electron，原因没找到 [B16][B17]。
   - 反方向的只有 AionUi 的一份 RFC，至今没有执行 [B10]。
   - Raycast 2 改用 WKWebView 之后，内存反而从 200–300 MB 升到 350–450 MB；官方列出了节流、闪烁、输入法、无障碍、拖放五类 WKWebView 坑 [C8]。
4. **上一轮的一条结论要更正。** Tauri 的 CEF 后端已经发布了：`tauri-runtime-cef` 从 2026-09-13 起随 v3 alpha 发到 crates.io，最新是 alpha.6（2026-10-14，CEF 154）[E12][E13]。这样就有了一条开源路线，可以同时满足"Rust 外壳"和"Chromium 引擎"，但目前还是 alpha。
5. **Rust 原生 UI 里，只有 GPUI 加 longbridge/gpui-kit 能基本撑起 Codex 视图。**
   - 现成能力：流式 Markdown、tree-sitter 高亮、带行内注释的只读 diff、贴底的变高虚拟列表 [D9]；Mermaid 可以用纯 Rust 的 merman [D15]。Zed 的 Delta 正好是这样拼出来的 [L9]。
   - 代价：API 不稳定（crates.io 上停在 0.2.2，实际要钉 git 版本）[D2][D7]；Zed 的终端、Markdown、自动更新等应用层 crate 是 GPL [D6]；AI 可用的语料少；M1 的 UI 工作量估计是 Electron 方案的 1.5–2.5 倍【判断】。
   - 其余 Rust UI 框架（Slint、iced、egui、Makepad、Freya）以及 Dioxus，在 diff、终端、长会话这几块都要大量自研。
6. **"和宿主同语言"值得一些，但远不如"组件现成"。**
   - 类型共享靠代码生成就能解决，ADR-04 已经定了生成管线。
   - 真正值得共享的是同步客户端核心，比如断线续传和投影 reducer。三条 Web 路线都能复用它：Tauri 在外壳里直接用，Electron 通过 napi-rs 加载，只是成本不同。详见第四节。
7. **调研员倾向（供编排者给 ADR-03 下结论时参考）：维持组合一，即 Electron 加 React。** UI 核心代码写成与外壳无关，把 Tauri 2 和 Tauri 3 CEF 留作有条件的后路。如果用户最看重"和宿主同语言、原生性能"，先用一个限时原型验证组合四（GPUI）再定。四个组合的前提和代价见第五节。

---

### 一、同类产品框架地图

包体：dmg、zip、pkg 的大小来自官方下载链接 HEAD 请求或 Release API 的 `size` 字段（十进制 MB）；"装后"是本机 `du -sh` 的结果（1024 进制）或第三方扫描。内存都是公开数字，本次没有运行任何 App。

**编码 agent 客户端**

| 产品 | 框架 | 渲染方式 | 包体 / 内存 | 选型理由、迁移、用户反馈 | 证据 | 核实 |
|---|---|---|---|---|---|---|
| ChatGPT 桌面 App（Codex 视图）26.1007 | 应用代码按 Electron API 写（React，Forge 7.11，开发依赖 Electron 42.3.0）；macOS 运行时是 OpenAI 自研的 OWL（Chromium 155，带 `OwlElectron*` 兼容层）；用 Sparkle 更新 | Chromium | Codex.dmg 788 MB；装后 1.6 GB（其中 codex-cli 322 MB，asar 541 MB）。没有官方内存数字，用户说它"RAM hungry" | 2024 年原生版自称"Not Electron. Not even Catalyst"；2026-02 推出 Codex app 时用 Electron，为了将来支持 Windows；2026-07 并入 ChatGPT，最晚到 07-14 关于页显示"Powered by Codex & OWL"。体积从 159 MB 涨到 1.45 GB 招来批评，也有人觉得新版更快 | L1 A1–A11 | 运行时已核实；切换时间和原因【推测】 |
| Claude 桌面 App（含 Claude Code）1.46388.4 | Electron 42.10.0 + React，本地前端 bundle，原生 Swift 模块，Forge 8 alpha + fuses | Chromium | universal zip 390 MB；装后 833 MB（通用二进制）。用户报告：空闲约 1.18 GB，单会话约 2.4 GB（只看了搜索摘要） | Claude Code 团队负责人（HN 账号 bcherny）说："worked on Electron … share code … same look and feel … Claude is great at it"。一直用 Electron，被 Gruber 等批评 | L2 A12–A16 | 已核实；内存是二手数据 |
| Cursor 3.19.13 | 基于 VS Code 1.128 的 fork，Electron 42.10.0 | Chromium | dmg 279 MB；装后 871 MB。论坛：Agents 窗口跑久了 2.4 GB、CPU 636% | fork 的原因是扩展 API 控制不了界面（论坛，二手） | L3 A17–A19 | 已核实 |
| Windsurf，2026-06-02 改名 Devin Desktop | VS Code fork，Electron | Chromium | 未取得 | 2025-07 被 Cognition 收购，后来改名 | A20 | fork 已核实；Electron【推测】 |
| VS Code 1.137.0 | Electron 42.10.0 | Chromium | arm64 zip 317 MB；装后 1.4 GB（含内置扩展） | 2020–2023 年花了约 3 年把渲染进程迁进沙箱 | L4 A21–A23 | 已核实 |
| Zed 1.24.2 | 自研 GPUI（Rust，Apache-2.0） | Metal 自绘，文字用 CoreText | dmg 119 MB；无内存数字 | 帧预算只有 8.33 ms；"I can't imagine a worse application for Electron than a code editor"；从 Atom 到 Zed 1.0 用了约 5 年 | A24–A27 | 已核实 |
| **Delta 0.1.0**（Zed 公司的 agent 协作客户端，Beta） | GPUI；通过 `agent-client-protocol` 2.0.0 接 agent；merman 渲染 Mermaid；pulldown-cmark；accesskit | Metal 自绘 | 装后 190 MB（单个 197 MB 二进制） | 官网没写技术；crate 名里有 thread_view、diff_view、comment_set、inline_terminal | L9 D43 | 已核实（bundle） |
| JetBrains Air 262.1037 preview | Fleet 平台：Kotlin/JVM + Skia（skiko）+ JetBrains Runtime；终端用 alacritty_terminal | JVM 上 Skia 自绘 | arm64 dmg 298 MB | Fleet 2025-12 停止分发，平台转给 Air；Noria 是因为"当年还没有 Compose" | A30–A33 | 平台已核实；用 Noria【推测】 |
| Warp | 自研 WarpUI（Rust，2026-04 开源，UI 框架部分 MIT） | Metal 自绘 | 装后 451 MB（单个 413 MB 二进制） | 试过 Electron；"lack of a stable UI framework for Rust" | L5 A28 A29 | 已核实 |
| GitHub Desktop 3.6.7 | Electron + TS | Chromium | arm64 zip 482 MB | 2017 年把原生 Mac 和 Windows 两套客户端合并成 Electron："doesn't scale" | A34–A36 | 已核实 |
| GitHub Copilot app 1.1.29（2026-06 GA） | Tauri | 系统 WebView | dmg 352 MB；用户说省内存 | 官方没说用什么框架；Linux 用户报告 WebKitGTK 重绘和输入法问题 | A37–A39 | 【推测】只有拆 Linux 包的用户报告 |

**ACP 与多 agent 客户端**

| 产品 | 框架 / 前端 / 关键组件 | 渲染方式 | 包体 / 内存 | 选型理由、迁移、用户反馈 | 证据 | 核实 |
|---|---|---|---|---|---|---|
| Conductor 0.91（闭源） | Tauri | WKWebView | 装后 115 MB（第三方扫描） | 创始人原话："built in Tauri … Rust backend … native Mac renderer"；changelog 里有十多次性能修复 | B1–B3 | 已核实（推文镜像加第三方扫描） |
| CodexMonitor 0.7.67 | Tauri 2.10 / React 19 / @pierre/diffs、TanStack Virtual、xterm | WKWebView，透明窗口，用私有 API | dmg 25.5 MB；issue：WebContent 进程 1–1.7 GB，有人报 4.3 GB | 根因是 React effect 死循环；修过拼音输入时按 Enter 误发送的问题；2026-03 起停更 | B4 B5 | 已核实 |
| Codeg 0.35.2 | Tauri 2.11（Rust 核心也能跑成无头服务器）/ Next + React 19 / streamdown 2、shiki、xterm 6 | WKWebView | dmg 86.7 MB；issue：3 个会话跑 1.5 天，WebContent 进程从 1.4 GB 涨到 2.2 GB | WebKit 的 `updateEventRegions` 是热点，叠加应用代码的布局抖动；中文输入法组词被打断 | B7 B8 | 已核实 |
| Harnss 0.21.5 | Electron 40 / React 19 / monaco、mermaid、xterm 6 | Chromium | dmg 321 MB | 为了 Liquid Glass，在发布构建里打开 CDP 9222 端口（反面例子） | B6 | 已核实（只读源码） |
| AionUi 2.2.2 | Electron 37 / React 19 / streamdown、react-virtuoso、mermaid；后端已迁到 Rust | Chromium | 约 248 MB（RFC 自述） | 提过换 Tauri 的 RFC，至今没执行 | B9 B10 | 已核实 |
| Goose 1.54 | Electron 43 + Forge + fuses / React 19 | Chromium | zip 209 MB | 试做过 Tauri 版后删掉，理由与 Tauri 无关；长会话会把全部消息都挂到页面上 | B11–B13 | 已核实 |
| **opencode 1.19.2** | Electron 44 / SolidJS / @pierre/diffs、@shikijs/stream、mermaid、ghostty-web | Chromium | dmg 151 MB | **从 Tauri 换回 Electron**：WebKit 渲染慢，跨平台不一致，sidecar 不稳定；包体变大，作者认为"值得" | B14 B15 | 已核实 |
| **Happy desktop 0.0.97** | Electron 43 / React 19 / @pierre/diffs、ghostty-web | Chromium | dmg 133 MB | **原来用 Tauri 包 Expo Web，现在换成 Electron**，原因没找到；iOS 版在改写成 UIKit | B16 B17 | 迁移已核实 |
| Paseo 0.11.4 | Electron 44 + Expo/RN（react-native-web），一套代码覆盖 iOS、Android、Web、桌面 | Chromium | dmg 186 MB | 渲染进程把打开过的每条时间线都留在内存里 | L6 B19 | 已核实 |
| Lody 0.104 | Electron 44.7 / React / @pierre/diffs、streamdown、shiki 4、xterm、mermaid、TanStack Virtual | Chromium | 装后 461 MB（本机 0.105） | 有长会话内存方面的 issue | L7 B20 | 已核实 |
| 其他 ACP GUI 客户端 | Tauri：ACP UI（dmg 4.1 MB）、Gold Band、Jockey、littleguys；Electron：DeepChat、Threadlines；GPUI：Agent Studio、Vibex、Superlite；egui：oxi；Flutter + Rust：AcpAgentClient；Poolside 框架不明 | — | — | AcpAgentClient 作者说：GPUI 版"大量时间花在手工复刻 UI"，Electron 版"500MB+" | B18 B21–B33 | 已核实（Poolside 未知） |

**可作参照的开发者桌面工具**

| 产品 | 框架 | 渲染方式 | 包体 / 内存 | 选型理由、迁移、用户反馈 | 证据 | 核实 |
|---|---|---|---|---|---|---|
| 1Password 8 | Electron + React，Rust 核心（Neon 绑定）；iOS 用 SwiftUI；用 typeshare 生成类型 | Chromium | pkg 398 MB；装后 913 MB（第三方扫描） | 2021 年从 AppKit 改成 Electron，中途停掉了 SwiftUI 版 Mac App；用户批评"不原生" | C1–C7 | 已核实 |
| Raycast 2.8 | Swift/AppKit 外壳 + WKWebView + React/TS + 常驻 Node + Rust | WKWebView | dmg 145 MB；官方给的内存：v1 200–300 MB，v2 350–450 MB | 2026 年从原生改成混合架构；否决了 Electron（不想在 macOS 上带 Chromium）和 Tauri（"less control … still young"） | C8–C12 | 已核实 |
| Ghostty 1.3.1 | Zig 核心 + macOS 上的 SwiftUI/AppKit | Metal + CoreText | dmg 34 MB | 一开始就是原生，强调"built for their platform first" | C13–C16 | 已核实 |
| Arc / Dia | Swift + Chromium（自研 ADK）；Dia 放弃了 SwiftUI 和 TCA | Chromium + 原生 UI | dmg：Arc 455 MB，Dia 854 MB | CEO 原话："Arc was bloated"；Dia 为了性能弃用 SwiftUI/TCA | C18–C24 | 已核实 |
| Superhuman | Electron + React | Chromium | 装后 475 MB（第三方扫描） | 从 node-webkit 换到 Electron；CTO 承认 RAM 和 CPU 占用是问题 | C25 C26 | 已核实 |
| Linear | Electron | Chromium | dmg 213 MB | 速度主要来自 local-first 同步引擎（二手资料） | C27 C28 | 【推测】只有第三方扫描 |
| Notion Mail / Notion Calendar | Electron | Chromium | 装后 425 / 488 MB（第三方扫描） | Calendar 的前身 Cron 起步就是 Electron | C29 C30 | 【推测】只有第三方扫描 |

**体积参照**（本机 bundle）

- 一个 arm64 Electron 应用，光运行时装后就占 240–275 MB；做成通用二进制约 490 MB [L10]。
- Tauri 应用 CC Switch 装后 72 MB [L8]，Wails 应用 Tiny RDM 装后 26 MB [L8]。
- 原生加 Metal 的 Otty 装后 54 MB，它的设置页用的是 WebView [L8]。

---

### 二、候选框架全景表

范围：上一轮比较过的四种方案，加上 brief 新增的框架。OWL 不对外提供，不列入。

**表 2-1 成熟度与 macOS 表现**

| 框架 | 版本与维护 | 生产用户 | 原生观感 | 半透明材质 | 中文输入法 | 无障碍 | 多窗口 | 来源 |
|---|---|---|---|---|---|---|---|---|
| Electron | 44.8.0（2026-10-16）；每 8 周一个大版本，官方只支持最新 3 个 | Claude、Cursor、VS Code、opencode、Goose、1Password、Linear | Web 自定义 | `vibrancy: 'sidebar'`；没有内建 Liquid Glass | Chromium，正常 | Chromium 无障碍树【判断：成熟】 | 支持 | 上轮 S1–S5；L2–L4 |
| Tauri 2（wry） | 2.12.4；v2 自 2024-10 稳定 | Conductor、CodexMonitor、Codeg、Copilot app【推测】 | Web 自定义 | `windowEffects`（macOS 26 上有 liquidGlass） | WKWebView 的 Enter 问题（WebKit bug 165004）；Codeg 和 CodexMonitor 都踩过 | WebKit | 支持 | 上轮 S12 S13 S33；B5 B8 |
| Tauri 3（可选 CEF） | 3.0.0-alpha.5；runtime-cef alpha.6（CEF 154）；没有正式版时间表 | 无（alpha） | Web 自定义 | 支持红绿灯定位；材质能否用待实测 | 选 CEF 时走 Chromium【判断】 | 选 CEF 时走 Chromium | 支持 | E12–E14 |
| SwiftUI/AppKit 原生 | 系统框架。SwiftUI 自带的 WebView 要 macOS 26；按新工程原则 D4，需要时可以直接提高最低版本 | Ghostty、Otty；ChatGPT Classic 已被替换 | 最好 | NavigationSplitView 自带；macOS 26 上是 Liquid Glass | NSTextView 最稳 | 最好 | WindowGroup | 上轮 S31；C13；L8 |
| 原生外壳 + WKWebView | — | Raycast 2；Atlas（SwiftUI + OWL）；Otty | 外壳原生，内容区是 Web | 外壳用原生材质 | 内容区有 WKWebView 的问题 | 内容区难做，Raycast 列为难点之一 | 支持 | C8 A1 L8 |
| GPUI + gpui-kit | crates.io 停在 0.2.2（2025-10）；实际钉 git 版本或用第三方每周快照 gpui-pre；README 写明"pre-1.0, breaking changes" | Zed、Delta、Longbridge Pro、Superlite、Vibex | Metal 自绘，文字用 CoreText，不是系统控件 | 只能整窗 NSVisualEffectView | 实现了 NSTextInputClient；有一条中文 IME 的 issue 未关 | AccessKit 2026-05 才合入 | 支持 | D1–D12 L9 |
| Dioxus 桌面版 | 0.7.10 / 0.8 alpha，每个 0.x 都有破坏性变更 | 自称有 Airbus 等，未核实 | WKWebView + Rust 写的 rsx | 没有内建，可以借 window-vibrancy | 同 WKWebView | WebKit | 支持 | D17–D19 D41 |
| Dioxus Native / Blitz | 0.3 beta，官方原话"many bugs" | 无 | 自绘 | — | — | accesskit 可选 | 在做 | D20 |
| Slint | 1.18.1（2026-09-21），1.x 稳定 | LibrePCB 2.0；嵌入式设备 | 自绘，仿 cupertino 风格 | 无 | winit 的 preedit | AccessKit 默认开启 | 支持 | D22–D29 |
| iced | 0.14.0（2025-12），大约一年一版 | COSMIC（用的是 fork）、Halloy | 自绘 | 只有透明窗口 | 0.14 起支持 | **上游没有** | 支持（未核实） | D30–D33 |
| egui | 0.37.0（2026-10-15） | Rerun | 官方明说不追求原生观感 | 无 | 2026-03 大改 | AccessKit | viewports | D34–D36 |
| Makepad | crates.io 1.0（2025-05）；dev 分支 2.0 未发布 | Robrix、Moly | 自绘 | 有 NSVisualEffectView | 有 setMarkedText | **无** | 从源码推断支持 | D37 D38 |
| Freya | 0.4.3 / 0.5 rc，主要一人维护 | 未列出 | Skia 自绘 | 未见 | 未见 | accesskit | 未核实 | D39 |
| Electrobun | 2.0.2（2026-09-29），一年内经历了 0.x、1.x、2.x 三个大版本 | Dash，其余多是个人和独立项目 | Web | **无** vibrancy | 默认 WKWebView，可选 CEF | WebView | 支持 | E1–E5 |
| Wails 3 | beta.30（2026-10-15，每晚自动发版） | v3 没有核实到生产用户（v2 有 Tiny RDM） | Web | 最完整：Translucent 和 LiquidGlass | WKWebView | WebKit | 支持，是主打功能 | E7–E11 |
| Neutralinojs | 6.10.0 | 官网没有点名生产用户 | Web | 只有透明 | WKWebView | — | 有限 | E15–E17 |
| Flutter | 3.47.7 | RustDesk、AppFlowy | 自绘；macos_ui 是模仿 | 靠第三方插件 | 实现了 NSTextInputClient，质量没实测 | 有 VoiceOver 桥 | **官方仍是 experimental** | E19–E23 |
| Compose MP | 1.12.2 | JetBrains Toolbox | Skia 自绘 | 无 | 走 AWT 输入法框架，质量未核实 | 官方称"Fully supported" | 支持 | E24–E28 |
| RN macOS | 0.83，**落后上游 0.88 五个小版本** | Office 部分界面；Messenger（已放弃） | AppKit 原生视图 | 没有官方 API | 原生【判断】 | 原生 | 没有高层 API | E30 E31 C33 |
| Avalonia | 12.1.4 | dotMemory、Plastic SCM、Lunacy | 自绘 | NSVisualEffectView（AcrylicBlur） | NSTextInputClient | NSAccessibility | 支持，另有 NativeMenu | E34–E39 |

**表 2-2 我们需要的组件**

| 框架 | 流式 Markdown | 代码高亮 | diff（hunk 操作、行内评论） | 长列表虚拟化 | 终端 | Mermaid | 来源 |
|---|---|---|---|---|---|---|---|
| Web 系（Electron、Tauri 2/3、混合方案的内容区、Electrobun、Wails、Neutralino） | streamdown 2.7（Codeg、AionUi、Gold Band、Lody 在用） | Shiki 4（Codex、opencode 在用） | @pierre/diffs 1.5，注释框架能放行内评论（Codex【文件名推断】加 6 个 ACP 客户端在用） | TanStack Virtual（7 家在用） | xterm.js 6（Codex 加 8 家）；新出现 ghostty-web（opencode、Happy） | mermaid 12（Codex 加 8 家） | L1；B4–B31；上轮 S22–S27 |
| SwiftUI/AppKit | Textual 0.5，还在 0.x | Textual 内建 | 没有，要自研或嵌 WebView | LazyVStack 处理变高行有已知问题，稳妥做法是 NSTableView | SwiftTerm 1.99 | 没有，要嵌 WebView | 上轮 S28 S29 S32 |
| GPUI + gpui-kit | TextView 的 markdown 加 `stream_fade` | tree-sitter 或 syntect | gpui-kit 的 `Diff`：只读、虚拟化，`DiffAnnotation` 放行内注释；hunk 暂存等交互要自己写 | GPUI `list` 的 FollowMode::Tail；gpui-kit 的 VirtualList 和 MessageScroller | 基于 alacritty_terminal 自己封装，或用 gpui-terminal 0.1；Zed 的 terminal 是 GPL，不能用 | merman，纯 Rust，对齐 Mermaid 12.2，Delta 在用 | D6 D9 D15 D16 L9 |
| Dioxus 桌面版 | 没有现成的，React 组件用不上 | 通过 eval 调 JS 库 | 没有现成的 | 没有现成的 | 可以嵌 xterm.js | 可以嵌 mermaid.js | D36【判断】 |
| Slint | `@markdown`，官方称"first step" | 无 | 无 | ListView 只实例化可见项 | 无 | 用 merman 出 SVG【判断】 | D26 D27 |
| iced | 内建 markdown，支持增量解析 | iced_highlighter | 无 | 没有官方的变高虚拟列表 | iced_term 0.8 | merman【判断】 | D30 D33 |
| egui | egui_commonmark，每帧重新排版 | egui_extras | 无 | egui_virtual_list | egui_term 0.1，已不活跃 | merman【判断】 | D34 D36 |
| Makepad | rich_text（只在 dev 分支） | code_editor | 无 | portal_list | terminal_core（只在 dev 分支） | 无 | D37 |
| Freya | MarkdownViewer | CodeEditor（tree-sitter） | 无 | VirtualScrollView | freya-terminal | merman【判断】 | D39 |
| Flutter | gpt_markdown 1.3.4；flutter_markdown 已停更 | re_editor | 无 | 内建 Sliver | xterm.dart 停在 2024-02 | 没有，要嵌 WebView | E22 |
| Compose MP | multiplatform-markdown-renderer 0.46 | 只有零散的库 | 无 | LazyColumn | JediTerm 是 Swing 组件，要用 SwingPanel 嵌 | 没有 | E29 |
| RN macOS | react-native-enriched-markdown 1.1.1，支持 macOS 和流式 | 没有成熟的原生方案 | 无 | FlatList | 只能 WebView 加 xterm.js | 走 WebView | E32 |
| Avalonia | LiveMarkdown.Avalonia 2.4.3，面向 LLM 流式输出 | AvaloniaEdit + TextMate | 无 | 内建 | 基本没有 | WebView 加 mermaid | E38 |

**表 2-3 工程维度**

| 框架 | 包体 / 内存（参照） | 签名、公证、自动更新 | AI 编码效率【判断】 | 和 iOS 共享 | 和 Rust 宿主协同 | 来源 |
|---|---|---|---|---|---|---|
| Electron | 运行时装后约 250 MB（arm64），同类产品 dmg 在 130–480 MB；Better Stack 测得空闲 128 MB；agent 客户端长会话常见 1–2 GB 以上，多数是应用代码造成的 | Forge 或 electron-builder 集成签名和公证；更新用 Squirrel.Mac 或 electron-updater，Codex 用的是 Sparkle | **高**：React/TS 语料最多，能用 Playwright 和 CDP 验证；Octoverse 显示 TS 已是第一语言，类型系统帮 AI 更早暴露错误 | 渲染组件可以放进 iOS 的 WKWebView；TS 写的同步核心 iOS 用不上 | 中：主进程可以通过 napi-rs 加载 Rust（1Password 用的是 Neon），渲染进程也可以用 WASM | L10 E6 A15 A19 B13 X1 X2 C2 |
| Tauri 2 | 外壳只有几 MB，CodexMonitor dmg 25.5 MB，Codeg 86.7 MB；内置 CLI 后 Copilot app 达到 352 MB；空闲 109 MB；WebContent 进程同样会长到 1–2 GB | `tauri build` 自动签名公证；更新插件强制使用自有密钥 | 中：前端和 Electron 一样，但 Rust 侧的 v1、v2、v3 API 容易混用；macOS 上的端到端测试只能走 WebDriverIO | 有 iOS 目标；CodexMonitor 的 iOS 版还是 WIP | **强（外壳层）**：外壳可以直接依赖 `cupper-protocol` 和 `cupper-client`；TS 类型用 tauri-specta（2.0 仍是 rc）生成 | 上轮 S14 S16–S18；B4 B7 A38 X2 |
| Tauri 3 + CEF | 【判断】接近 Electron，CEF 约多出 200 MB | 同 v2 | 中偏低：还在 alpha，破坏性变更多 | v3 的 iOS 构建还在修 | 强（同 Tauri 2） | E6 E12 E13 |
| SwiftUI/AppKit | 最小：Ghostty dmg 34 MB，Otty 装后 54 MB | Xcode + notarytool + Sparkle | 低：语料少，Swift 6 并发检查容易卡住，pbxproj 改动容易冲突 | 最高 | 中：通过 UniFFI 0.32（仍是 0.x） | 上轮 S30；C16 L8 X2 |
| 原生外壳 + WKWebView | Raycast dmg 145 MB（含 Node）；内存 350–450 MB，比 v1 高 | 同原生 | 低：要维护两套栈 | 外壳经验可迁移到 iOS，内容组件可以进 WKWebView | 中 | C8 C10 |
| GPUI + gpui-kit | 都是完整产品的数字：Zed dmg 119 MB，Delta 装后 190 MB；没有可信的内存数据 | 没有官方工具；Zed 自写打包脚本，auto_update 是 GPL；要自己接 Sparkle 或 cargo-packager | **偏低**：API 每周都在变，语料少，主要靠读 Zed 源码；gpui-kit 附带了 AGENTS.md 和 skills；AcpAgentClient 的经历是反例 | gpui_ios 2026-10-14 才合入，太新 | **最强**：和宿主在同一个 workspace | D2 D7 D9 D13 D14 B26 L9 |
| Dioxus 桌面版 | 和 Tauri 同一量级 | `dx bundle`，签名参照 Tauri 的文档 | 中：rsx 写法类似 React，有 llms.txt | 有 iOS（WKWebView） | 很强 | D17–D21 |
| Slint | 嵌入式口径下运行时很小；二手数据：从 egui 换到 Slint 后，内存从 130 MB 降到 30–50 MB | 用 cargo-packager | 中：DSL 语料少，但 1.x 稳定；App 内可以嵌 MCP，让 AI 查看界面 | iOS 只支持 Rust API | 很强；许可证可选 GPLv3，或免版税许可（需署名） | D22 D25 D28 D42 |
| iced / egui / Makepad / Freya | 没有官方数据 | egui 能用 cargo-packager；Makepad 有自带工具 | 偏低 | iced 不支持；egui 不是一等支持；Makepad 支持；Freya 还在 PR | 很强 | D30–D40 |
| Electrobun | 压缩后 1.28 MiB 到 15 MiB；开 CEF 后 418 MB，空闲 208 MB | 内建签名和公证；bsdiff 差分更新 | 低：一年三个大版本 | 无 | 2.0 起主进程可以用 Rust 写，但很新 | E1–E6 |
| Wails 3 | v2 的例子是 26 MB | 内建签名公证和 updater | 中低：还在 beta | iOS 是 experimental | 弱：Go 会成为第三种语言 | E7–E11 |
| Neutralinojs | 约 2 MB | 没有签名和公证的说明 | 中 | 无 | 弱；原生能力走本地 WebSocket，和我们的威胁模型冲突 | E16 E17 |
| Flutter | 没有官方数据 | 走 Xcode 流程；auto_updater 2024 年后没更新 | 中 | **高** | **强**：flutter_rust_bridge 2.13，RustDesk 和 AppFlowy 是先例 | E20–E23 |
| Compose MP | 要打包 JRE【判断：偏大】 | Gradle 能签名公证；**没有官方自动更新** | 中 | iOS 从 1.8 起 stable | 中：UniFFI 有 Kotlin 绑定 | E25 E27 |
| RN macOS | 没有数据 | 没有官方更新方案；Messenger 当年自己集成了 Sparkle | 中 | 高，但 macOS fork 落后上游 | 中低：uniffi-bindgen-react-native 没列 macOS | E30 E33 C33 |
| Avalonia | 没有数据 | 没有官方更新方案 | 中 | 支持（需 .NET 10） | 中 | E35 E40 |

**可以排除的**【判断】

- **Electrobun**：变化太快，生产用户少，macOS 上没有材质，通知还在用已弃用的 API。
- **Wails 3**：Go 会成为第三种语言，还在 beta。
- **Neutralinojs**：架构上走本地 WebSocket，和"agent 输出不能让渲染进程替自己批准"的威胁模型冲突，也没有签名和公证工具。
- **Deno Desktop**：还是 experimental。
- **Dioxus**：用的同样是 WKWebView，却丢掉了 React 生态；Native 版还是 beta。
- **Slint、iced、egui、Makepad、Freya**：diff、终端、长会话都要大量自研；iced 和 Makepad 的上游没有无障碍支持。
- **Compose MP**：要带 JVM，没有官方自动更新，终端只能嵌 Swing 组件。
- **RN macOS**：版本落后上游；终端、diff、Mermaid 都要走 WebView；Messenger 已经放弃了桌面版。
- **Avalonia**：C# 栈，Markdown 和终端组件不齐。
- **Flutter**：只有在"iOS 共享 + Rust 客户端核心"压倒一切时才值得考虑。它缺 diff、Mermaid，终端组件已停更，官方多窗口还是 experimental，最后还是会回到嵌 WebView。

---

### 三、同类产品选型的规律与教训

**迁移案例**

| 产品 | 时间 | 从 → 到 | 官方原因 | 证据 |
|---|---|---|---|---|
| ChatGPT Mac | 2026-07 | 原生 AppKit（Classic，159 MB）→ Codex 外壳（Electron 编程模型，运行时是 OWL） | 没有官方说明框架层面的原因；媒体只说"merging Codex and ChatGPT" | L1 A1–A10 C31 C32 |
| 1Password 8 | 2021 | AppKit → Electron + React + Rust 核心；中途停掉 SwiftUI 版 | 统一平台，按功能组织团队；SwiftUI 版仍要为各平台分别实现组件 | C1 C2 C5 |
| GitHub Desktop | 2017 | 原生 Mac 和 Windows 两套 → Electron | "doesn't scale"；反馈周期从几分钟缩短到几秒 | A34 A35 |
| Raycast 2 | 2026-05/08 | Swift/AppKit → Swift 外壳 + WKWebView + React + Node + Rust | 要做 Windows 版；编译慢；AppKit 掣肘；原生人才难招。明确否决 Electron 和 Tauri | C8 C9 |
| Arc → Dia | 2025 | 弃用 SwiftUI/TCA，保留 Swift + Chromium | 为了"lightweight, snappy" | C18 |
| Zed | 2017 起，2026-04 发布 1.0 | Atom/Electron 的经验 → 自研 GPUI | 帧预算；Web 技术"imposed a ceiling" | A24–A26 |
| opencode | 2026-03/05 | Tauri → Electron | WebKit 性能和一致性，sidecar 不稳 | B15 |
| Happy | 2026-05/08 | Tauri（包 Expo Web）→ Electron | 没找到 | B16 B17 |
| Goose | 2026-03/05 | 试做 Tauri 版后删除，留在 Electron | 不想同时做两件大事 | B12 |
| Teams | 2023 | Electron → WebView2 | 内存、磁盘占用 | C35 |
| Messenger | 2021→2025 | Electron → RN Desktop → 关停 | 包体和启动速度；代价是多窗口、更新、菜单都要自建 | C33 C34 |
| WhatsApp Mac | 2024 | Electron → Catalyst | 更快、和系统集成更好 | C36 |

**规律**

1. **从原生换到 Web 的驱动力，是团队规模和跨平台，不是性能。**
   - ChatGPT、1Password、GitHub Desktop、Raycast 都是这个方向。理由集中在一套代码多平台、按功能组织团队、招人、迭代速度。
   - Anthropic 还多了一条："Claude is great at it" [A12]。
   - 代价同样一致：被批评"不像 Mac App"、体积膨胀（ChatGPT 从 159 MB 到 1.45 GB）、内存偏高 [A6][A7][C5][C12]。
   - cupper 本来就以"像 Codex"为目标，观感上的批评在这里不是新增代价。
2. **OpenAI 换的是运行时，没换编程模型。** 原生版（2024）→ Electron（2026-02）→ OWL（2026 年中）。UI 代码一直保留 Electron API 加 React [L1][A3][A4]。
   - 这说明按 Electron 模型写 UI 不是死路，大厂"超出 Electron"之后，换的是底下的运行时，没有重写界面。
   - 开源世界里最接近这个思路的是 Tauri 3 的 CEF runtime：Rust 外壳加 Chromium，构建时选择 [E12][E13]。
3. **系统 WebView 不一定更省资源，而且会带来引擎特有的补丁代码。**
   - Raycast 改用 WKWebView 后内存升高，空 WebView 的基线约 50 MB [C8]。
   - Better Stack 的空闲内存对比是 Tauri 109 MB、Electron 128 MB [E6]。
   - Tauri 项目的 WebContent 进程同样会长到 1–2 GB [B5][B8]。
   - WKWebView 的输入法、节流、闪烁问题都需要专门写绕过代码 [C8][上轮 S33]。这和用户新提的"不产生技术债和垃圾代码"原则（feedback 会议 D4）有张力。
   - 包体的差距是真的：Tauri 应用 dmg 25–90 MB，Electron 应用 130–480 MB。但只要内置 CLI 或运行时，差距就会被吃掉（Copilot app 352 MB）[A38][B4][B7]。
4. **性能问题多半出在应用代码和数据架构上，不在外壳。**
   - 已公开的严重卡顿和内存问题，根因都在应用里：CodexMonitor 的 React effect 死循环 [B5]、Codeg 的布局抖动 [B8]、Goose 长会话全量挂载 [B13]、Happy 的渲染风暴 [B17]、Paseo 时间线常驻内存 [B19]、Cursor 长会话泄漏 [A19]。
   - 反过来，Linear 的快来自 local-first 同步 [C28]，Slack 是在 Electron 里重构了一遍 [C38]。
   - 对 SP-6 的启示：浸泡测试、多会话测试和长会话窗口化，比测空闲内存更重要。也说明 AI 写的 React 容易犯"effect 依赖不稳""根组件被高频更新"这类错，需要 lint 规则加性能预算来兜底。
5. **组件选型已经趋同，ADR-03 的初版清单被验证了。**
   - @pierre/diffs、xterm.js、Shiki、mermaid、TanStack Virtual、streamdown 在 Codex 和多个 ACP 客户端里反复出现 [L1][B]。
   - 新出现的值得关注的选项是 ghostty-web，一个 WASM 终端，API 和 xterm.js 兼容，opencode 和 Happy 已经在用 [B14][B17][C17]。
6. **"共享核心 + 各端 UI"是主流形态，和 cupper 的架构同构。**
   - 1Password：Rust 核心，桌面 Electron，iOS 用 SwiftUI，typeshare 生成类型 [C1–C3]。
   - Raycast：Rust 数据层和 iOS 共享，接口在一处定义，为每一端生成带类型的客户端 [C8]。
   - AionUi：后端迁到 Rust 后，Electron 只剩一层薄壳 [B10]。
   - Codeg：同一个 Rust 核心，既能跑桌面版，也能跑无头服务器 [B7]。
   - Ghostty：Zig 核心加各平台原生 UI [C13]。
7. **复杂的 Mac App，有三个独立证据不看好纯 SwiftUI。**
   - 1Password 停掉了 SwiftUI 版 Mac App [C1]。
   - Raycast v1 认为 SwiftUI"never cleared our bar" [C8]。
   - Dia 为了性能弃用 SwiftUI/TCA [C18]。
   - 原生方案里口碑好的（Ghostty、Zed、Warp）都靠 AppKit 加 Metal 自绘。
8. **自研 GPU UI 只在帧预算敏感的产品里出现，而且代价巨大。**
   - Zed 从开始到 1.0 用了约 5 年 [A26]。Warp 的 UI 框架做了 5 年才开源 [A29]。
   - 但 Zed 有了 GPUI 之后，做 agent 客户端 Delta 就顺理成章 [L9]，社区也冒出十几个 GPUI 编码 agent 客户端 [D11]。
   - 对新团队来说，门槛在于 API 不稳定和语料少。AcpAgentClient 作者在 GPUI 上"大量时间花在手工复刻 UI" [B26]。
9. **AI 编码的视角，现有证据都偏向 Web 和 TS。**
   - Claude 团队说"Claude is great at it" [A12]；opencode 作者说用 agent 迁移 UI"was easy" [B15]；Octoverse 显示 TS 成为第一语言，并把原因归到类型系统和 AI [X1]。
   - 反例是 GPUI 和 SwiftUI 的语料少 [B26][上轮 5.1]。
   - 新现象：gpui-kit 附带 AGENTS.md 和 skills [D9]，Slint 提供给 AI 用的 MCP [D28]，Wails 自己在用 agent 开发 [E 组笔记]。框架方开始主动适配 AI。
10. **安全上的反面例子。**
    - Harnss 为了 Liquid Glass，在发布构建里打开了 CDP 端口 [B6]。
    - Happy 和 Claude 从远端加载渲染层 [B17]。
    - 这两种做法都和 ADR-03 的 fuses、CSP 要求冲突，不要学。
    - VS Code 把渲染进程迁进沙箱花了约 3 年 [A22]。cupper 从第一天就开沙箱，能避开这笔债。

---

### 四、和"宿主用 Rust"的协同

| 方案 | 能直接复用宿主的什么 | 机制 | 还要不要 TS 或 Swift 代码生成 |
|---|---|---|---|
| GPUI / Dioxus / Slint / iced / egui / Makepad / Freya | 协议类型、帧解析、同步客户端核心（续传、幂等、投影 reducer）都能直接用 | 同一个 cargo workspace，直接依赖 crate | 桌面不需要；iOS 仍要（UniFFI 或 JSON Schema） |
| Tauri 2 / 3 | 外壳进程里能用上面所有东西，宿主连接和凭据都留在 Rust 里；UI 是 TS | 外壳依赖 crate，再通过 event 或 channel 推给 WebView | 要：TS 类型通过 tauri-specta（rc）或 schemars → JSON Schema → TS 生成 |
| Electron | 主进程能用同步核心和帧解析 | napi-rs 3.14 原生插件（1Password 用 Neon，Raycast 用 napi）；或者渲染进程用 WASM | 要（ADR-04 已定的管线） |
| Flutter / RN / Compose / Avalonia | 通过 FFI 调用同步核心 | flutter_rust_bridge、uniffi-bindgen-react-native、UniFFI Kotlin、uniffi-bindgen-cs | 要 |
| Wails / Electrobun | 只能经 C ABI 调用；Electrobun 2.0 的主进程可以用 Rust 写 | cgo 等 | 要 |

**这一点值多少**【判断】

- **类型共享**：ADR-04 已经定了"Rust → schemars → JSON Schema → TS + zod"，一次性搭好之后只剩 CI 维护，大约是几人日加上持续的小成本。全 Rust UI 能省掉的主要就是这条管线。iOS 不用 Rust UI 的话，JSON Schema 或 UniFFI 照样得有。
- **同步核心共享**：这是真正有价值的部分。续传、合并、重新同步这些边界条件，只写一份实现、测一次，CLI、桌面、iOS（经 UniFFI）都能用。不过 Tauri（外壳持有）和 Electron（napi-rs）也能拿到这份收益，只是要多一层"Rust → UI 推快照"的胶水。
- **语言数量**：全 Rust UI 能减少 AI agent 在 Rust 和 TS 之间切换。但我们的语言切换主要发生在"宿主和 UI 之间"，这本身就是一条清晰的协议边界。
- **安全**：非 HTML 的原生渲染，基本消除了"agent 输出 XSS 让渲染进程替自己批准"这类威胁。这是原生方案独有的加分项，和"同语言"无关。
- **结论**：同语言的增量价值偏低到中等，远小于组件缺口带来的差距。GPUI 方案的 UI 工作量估计会多出 0.5 到 1.5 倍。

---

### 五、推荐组合

组合一、二、三可以共用同一份与外壳无关的 React `ui-core`。所以 ADR-03 可以先定组件和 ui-core，外壳的最终选择交给 SP-6。

**组合一：最看重"和 Codex 一样"，同时最稳。Electron + React 19 + TS（即 ADR-03 现方案）**

- 组成：
  - 组件就用 Codex 包里看到的同款：streamdown/Shiki、@pierre/diffs、xterm.js（备选 ghostty-web）、mermaid、KaTeX、TanStack Virtual。
  - 可选：主进程通过 napi-rs 加载宿主的 Rust 同步核心。
- 前提：
  - 最看重复刻保真度、M1 速度和 AI 编码效率；
  - 接受运行时约 250 MB、整包几百 MB，以及"不像原生 Mac App"的批评；
  - 有人负责大约每半年升级一次 Electron 大版本。
- 依据：
  - Codex 的 UI 代码和组件 [L1]；
  - Claude、Cursor、Goose、opencode 都选了 Electron，opencode 是从 Tauri 换回来的 [A12][B11][B15]；
  - 测试和生产用同一个引擎，AI 可以用 Playwright 和 CDP 自己验证界面。
- 代价：体积和内存偏大；每 8 周一个大版本；安全加固工作量（fuses、CSP、沙箱）。
- 注意：Codex 在 macOS 上用的是自研的 OWL，我们拿不到，只能用官方 Electron。第三方项目已经证明，Codex 的代码在官方 Electron 42.3 上就能跑 [A4]。

**组合二：最看重轻量，外壳和宿主同语言。Tauri 2 + 同一份 React ui-core，Tauri 3 CEF 作后路**

- 组成：
  - Rust 外壳直接依赖 `cupper-protocol` 和 `cupper-client`，由外壳持有宿主连接；
  - WebKit 在性能或输入法上不达标时，切到 Tauri 3 的 CEF runtime，不用重写外壳 [E12]。
- 前提：
  - 对包体或空闲内存有硬性要求；
  - SP-6 证明 WKWebView 在长会话、流式输出、中文输入法上达标；
  - 接受 macOS 上的端到端测试链路较弱，只能用 WebDriverIO。
- 依据：
  - Conductor、CodexMonitor、Codeg 都在用 Tauri [B1][B4][B7]；
  - Tauri 外壳本身很小 [L8]。
- 代价：
  - 已有 opencode 和 Happy 换回 Electron 的先例 [B15][B17]；
  - WKWebView 需要专门的绕过代码，比如输入法 Enter 判断 `keyCode 229`，和"不写补丁代码"的原则有张力；
  - 开发和生产用的是不同引擎；
  - 一年内要从 v2 迁到 v3；
  - 打包 Node 和适配器之后，体积优势会大幅缩小。

**组合三：最看重原生观感。SwiftUI/AppKit 外壳 + WKWebView 内容区（Raycast 2 和 Atlas 的模式）**

- 组成：
  - 原生部分负责窗口、侧边栏、工具栏、设置、菜单、通知，在 macOS 26 上自动获得 Liquid Glass；
  - WKWebView 用同一份 React ui-core 渲染消息流、diff、终端和 Mermaid；
  - 按新工程原则 D4，需要时可以直接把最低版本提到 macOS 26，使用 SwiftUI 自带的 WebView。
- 前提：
  - 用户把"像原生 Mac 应用"放在"和 Codex 一模一样"之前；
  - M3 的 iOS 走 SwiftUI，希望外壳经验和 Swift 协议类型在桌面和 iOS 之间复用。
- 依据：Raycast 2 [C8]、Atlas [A1]，以及 1Password、Raycast、Dia 对纯 SwiftUI 的负面经验 [C1][C8][C18]。
- 代价：
  - 两套 UI 栈之间的协作：焦点、快捷键、拖放、主题；
  - Raycast 列出的 WKWebView 五类坑；
  - 内存未必更低，Raycast v2 就比 v1 高；
  - AI 写 SwiftUI 的效率低；
  - 测试要覆盖两套栈。

上一轮把这个组合打了最低分，这次的新证据（Raycast 2）证明它能做成，但没有改变"成本最高"的判断。

**组合四：最看重和宿主同语言，全部用 Rust。GPUI + gpui-kit + merman + alacritty_terminal**

- 组成：
  - 宿主、CLI、桌面 UI 放在同一个 cargo workspace；
  - 缺的部分用 gpui-kit 的 WKWebView 嵌入兜底 [D9]；
  - GPUI 钉 git 版本。
- 前提：
  - 用户最看重同语言、性能和小包体，愿意让 M1 慢一些；
  - 接受钉 git 版本、每周跟进上游；
  - 先做一个 1–2 周的限时原型：消息列表、流式 Markdown、带注释的 diff，看 AI agent 实际写得怎么样，再定。
- 依据：
  - Zed 和 Delta 证明这条路可行 [L9][A24]；
  - gpui-kit 的组件覆盖面 [D9]；
  - Superlite、Vibex 等 GPUI 客户端 [B22–B24]。
- 好处：包体小；没有 HTML 渲染层，XSS 自我批准这类威胁基本不存在；和宿主共享类型和同步核心。
- 代价：
  - M1 的 UI 工作量估计是组合一的 1.5–2.5 倍【判断】；
  - diff 的 hunk 操作和评论交互要自己写；
  - 没有 Playwright/CDP 这样的外部驱动；
  - 无障碍（2026-05 合入）和 iOS 后端（2026-10 合入）都很新；
  - 不能复用 Zed 的 GPL crate；
  - 观感是自绘的，不是系统控件，材质只能整窗；
  - 有 AcpAgentClient 这个反例 [B26]。

**调研员倾向**（feedback 会议 D7 已决定由编排者直接给结论，这里只供参考）

- 默认选组合一。
- 组合二只在包体或内存成为硬性要求、而且 SP-6 证明 WebKit 达标时才考虑；Tauri 3 CEF 值得跟踪，但 M1 不要用 alpha。
- 用户如果明确偏好"同语言、原生性能"，先给组合四做限时原型，再决定。
- 组合三只在"原生观感"是第一优先级时才选。

**对 SP-6 的建议**

- 同一份 bundle 增加一个 Tauri 3 CEF 变体；
- 把浸泡测试（1 小时以上、6 个会话、5,000 条消息）和 WebContent 进程的内存列为主指标；
- 如果组合四仍在候选，另做一个 GPUI 消息列表的原型。

## 关键取舍

- **并行拆分。** 调研按五个方向拆给分支调研员并行执行，我负责本机 bundle 检查、横向资料和最终综合。这样能覆盖 40 多个产品和 20 个框架；代价是各分支用的二手资料口径不完全一致，我在 sources.md 里统一了编号和"一手、二手、搜索摘要"的标注。
- **bundle 检查的边界。** 除了 `ls` 和 Info.plist，我还解析了 `app.asar` 的头部目录，读出 `package.json` 的依赖名和前端资源文件名，也用 `grep -a` 在框架二进制里搜了版本串。我认为这仍属于"bundle 结构元数据"，没有解包，没有执行，也没有读任何用户数据目录。Codex 运行时是 OWL 这一关键结论就是这样得出的。如果编排者认为这超出了 brief 允许的范围，可以只保留 L 组里 Info.plist 和 Frameworks 的部分。
- **内存数据。** 按要求没有运行任何 App，所以所有内存数字都来自公开资料：官方（只有 Raycast 给了）、issue 和第三方测评。没有任何来源给出"同一 UI 在 Electron 和 Tauri 上"的受控对比，这只能交给 SP-6。
- **第三方扫描。** desktopinsights.com 的 bundle 扫描只作旁证。只有它一个来源的结论（Linear、Notion 系、Superhuman 的包体）在表里标为推测级。
- **不做最终决定。** 按 brief，给出四个组合和倾向，最终结论由编排者在 ADR-03 中给出。

## 验证方式

对照 brief 的验收标准：

- [x] **同类产品框架表**：第一节，共 29 行，覆盖 brief 点名的全部产品，另加 Delta、Paseo、Lody、Linear、1Password、Raycast。表里有框架、渲染方式、包体与内存、证据编号和核实状态。brief 要求的"官方理由、迁移史、用户反馈"合并在同一张表里。
- [x] **候选框架全景表**：第二节的表 2-1 到 2-3，覆盖上一轮的四种方案和 brief 新增的全部框架，按 brief 列的维度逐项填写，每格附来源编号或标【判断】。
- [x] **"同类产品选型的规律与教训"**：第三节，包括迁移案例表和 10 条规律。
- [x] **3 到 4 个推荐组合**：第五节给出 4 个，每个都写了前提、依据和代价，另附调研员倾向。
- [x] **每条事实附来源和日期，区分已核实和推测**：sources.md 共 200 条来源，都带资料日期和获取方式；正文用【推测】和【判断】标注。
- 个人信息检查：对 sources.md 做了个人信息关键词检查，无结果；本机路径一律写成 `/Applications/<App>.app`。（编排者存档时去掉了这里原有的具体检索词，因为其中含有用户的个人信息。）
- 分支状态：`git diff --stat main...HEAD` 显示只改了 `docs/tasks/R04-desktop-framework-survey/sources.md`（新增 420 行）；工作区干净；没有 merge、rebase 或 push。
- 外发请求：curl 统一带 `-A cupper-research`；crates.io 的 User-Agent 也用这个占位写法。GitHub 匿名 API 额度用完后，分支调研员改用本机已登录的 `gh api` 做只读 GET。请求内容里没有个人信息，但认证头会让 GitHub 知道是哪个账号在查询。这一点已写进 sources.md 开头，请编排者知悉。
- 没有下载或运行任何第三方安装包；包体都来自 HEAD 请求、Release API 的 `size` 字段，或本机已安装 App 的 `du` 结果。克隆的仓库都在 worktree 之外的临时目录，只读，没有执行。

## 提交列表

```
e65da71 docs(task): add R04 framework and ACP client source notes
2bdca5f docs(task): add R04 product source notes for agent clients and reference apps
568aaa2 docs(task): add R04 local bundle inspection notes
<编排者存档提交>
```

## 遗留问题 / 范围外发现

1. **要改已有文档里的两处表述。**
   - R01 §十二和 ADR-03 都说"Codex 本身就是 Electron + React"。准确说法应该是"Electron 编程模型加 React；macOS 版运行时是 OpenAI 自研的 OWL（Chromium 155）"[L1]。
   - desktop-engineer.md 2.1 说"CEF 后端没有进入发布版"，已经过时：`tauri-runtime-cef` 已随 v3 alpha 发布 [E13]。
2. **OWL 切换的细节没有官方说明。** 时间只能确定在 26.519（Electron 42.1）和 26.707（OWL）之间；Linux 版是不是仍用 Electron 也没核实 [A5]。
3. **几处框架判断只有间接证据。**
   - GitHub Copilot app 是 Tauri：只有一个用户拆 Linux 包的报告，macOS 包没核实 [A39]。
   - Conductor 是 Tauri：靠创始人推文的镜像加第三方扫描 [B1][B2]。
   - Linear、Notion 系是 Electron：只有第三方扫描 [C27][C29]。
   - Devin Desktop 是 Electron：从 FAQ 推断 [A20]。
   - JetBrains Air 用 Noria：推测 [A33]。
4. **常驻内存缺乏可比数据。** 只有 Raycast 给了官方数字。agent 客户端的公开数字都是用户在长会话下报的，测法各不相同。建议 SP-6 按第五节的建议补测。
5. **新的组件选项。** ghostty-web（WASM 终端，API 兼容 xterm.js）已经有 opencode 和 Happy 在用 [B14][B17][C17]，建议在 ADR-03 组件表里列为终端的备选。
6. **打包工具。** Codex、Claude、Goose 用 Electron Forge（Codex 配 Sparkle，Claude 配 Forge 8 alpha 加 fuses 插件）；Paseo、Lody、Superhuman、Linear 用 electron-builder [L1][L2][B11][B19][B20]。ADR-03 现在写的是 electron-builder，两条路线都有大产品在用，可以在设计阶段再确认一次。
7. **安全反面例子，供 ADR-03 加固清单参考。** Harnss 为了 Liquid Glass 在发布构建里开 CDP 端口 [B6]；Happy 和 Claude 从远端加载渲染层 [B17]。
8. **组合四的许可风险。** Zed 的 terminal、markdown、editor、mermaid_render、auto_update 都是 GPL-3.0 [D6]。如果选组合四，要在 CLAUDE.md 里明确禁止复制这些 crate 的代码。Slint 对开源项目的许可条件见 D22。
9. **范围外：和 ADR-15（iOS 方向）相关的先例。**
   - Paseo 用一套 Expo/RN 代码覆盖 iOS、Android、Web 和桌面（Electron 包 react-native-web）[B19]。
   - Happy 正在把 RN 版 iOS 改写成 UIKit 原生 [B16]。
10. **范围外：Delta.app。** 本机装有 Zed 公司的 agent 协作客户端 Delta 0.1.0（GPUI + ACP，有 diff 评论、内嵌终端、Mermaid）[L9]。它在交互上和 cupper 很接近，如果以后要补一份 UI 参照拆解，值得看一下。
11. **范围外：本机的 Paseo 和 Lody 版本较旧**（0.2.3 和 0.105），L 组的数字只代表本机安装的版本，当前版本的包体见 B19、B20。
