# 技术选型讨论会：桌面客户端工程师发言

- 日期：2026-10-16
- 角色：桌面客户端工程师（子 agent），第一轮独立发言
- 重点：ADR-03（桌面 UI 技术），同时谈 ADR-02（宿主语言）和 ADR-04（宿主协议）
- 依据：议程；PRD v1.0（6.1、UI-6、NFR-6、NFR-11）；R01 第二、三、八、九、十二、十三节；截图观察笔记；architect.md 1.3、1.4、2.1、2.3、2.6；R02 第 2、8 节
- 外部事实都在 2026-10-16 查证，编号 [Sx] 对应文末"来源"。标"判断"的地方是我的工程经验，没有外部数据支撑。本次没有修改仓库里的任何文件。

## 核心结论

1. **ADR-03 推荐 Electron + React + TypeScript。** 理由有三条：
   - Codex 本身就是 Electron + React（R01 §十二）。用同一套技术复刻，保真度最高，成本最低。
   - PRD 里最难的几块界面（流式 Markdown、带行内评论的 diff、终端、Mermaid）在 Web 生态里都有成熟的开源组件。原生方案要么没有，要么只能嵌 WebView。
   - React/TS 是 AI 编码 agent 最熟悉的组合，也最容易做自动化验证：测试和生产用的都是 Chromium，可以用 CDP 和 Playwright。
2. **Tauri 2 是很接近的第二名，不是否决。** 它的优势是包体小、内存略低，同类产品 Conductor 和 CodexMonitor 都在用。我没有选它，有四个原因：
   - 开发时用 Chromium，生产环境是 WKWebView，两者不一致；
   - macOS 上的应用内端到端测试只能走 WebdriverIO 内嵌驱动，不能用 Playwright；
   - 桌面通知的点击回调是 2026-10-10 才发布的；
   - Tauri 3 已经进入 alpha。

   我建议把 UI 代码写成与外壳无关，这样将来如果要切到 Tauri，改动只在外壳层。
3. **纯原生（SwiftUI/AppKit）和"原生外壳 + WebView"都不适合 M1/M2。** 原生方案在观感、和 iOS 共享代码、资源占用上的优势是真实的。但它存在这些缺口：
   - Mermaid 没有原生实现；
   - 没有可用的 diff 组件；
   - 流式 Markdown 只有 0.x 版本的库；
   - SwiftUI 的变高长列表有已知问题。

   这些部分最后还是要嵌 WKWebView，结果就成了混合方案。混合方案要维护两套 UI 栈，成本最高。
4. **NFR-11 的瓶颈不在 UI。** Electron 的界面进程比 Tauri 多占几十到一两百 MB。但 6 个会话就是 6 组"适配器 Node 进程 + agent CLI"，总量在 GB 级（判断，待 SP-1/SP-4 实测）。NFR-6 的 2 秒冷启动三种方案都能达到，前提是：启动路径上不同步加载 Shiki、Mermaid 这类重组件，宿主本身也要启动得够快（UI 不能自己缓存会话数据）。
5. **Web 类方案有一个必须正视的安全问题。** agent 的输出（Markdown、命令输出、Mermaid 源码、diff 内容）是渲染进程的不可信输入。一旦在渲染进程里拿到脚本执行权，它就能用 UI 的满权限身份发出"批准"命令，这正是 SEC-8 要防的"agent 替自己批准"。对策是严格 CSP、HTML 清洗、Mermaid 隔离渲染、凭据不进渲染进程。详见第 7 节，请安全工程师回应。
6. **对 ADR-02，UI 侧倾向 TypeScript 宿主。** 好处是：协议类型直接共享，不需要代码生成；只有一套工具链；Node 反正要带上（claude-agent-acp 要求 Node ≥ 22）。这不是硬性要求：如果为了和 iOS 共享加密代码而选 Rust，我只要求协议以 schema 为唯一来源，生成 TS 类型，并在 CI 里校验生成结果没有过期。我不建议用 Swift 写宿主。
7. **对 ADR-04，UI 侧关心的是语义，不是编码。** 具体要求：
   - 条目按 ID upsert，也能单独请求快照；
   - delta 可以合并；
   - 历史可以向前分页；
   - 大内容通过引用按需获取；
   - 会话内搜索由宿主提供（虚拟滚动之后，浏览器的查找看不到未渲染的条目）；
   - 遇到未知的事件类型可以降级显示。

   M1 用 JSON 加长度前缀帧就够了。
8. **SP-6 不是 ADR-03 的前置条件，但建议在 M1b 打磨界面之前做。** 做法是把同一份 React 原型分别装进 Electron 和 Tauri 两个外壳，验证流式渲染、1000 条以上的滚动、冷启动和中文输入法，同时拿到实测的内存数字。如果讨论会在 Electron 和 Tauri 之间僵持，就用 SP-6 的结果来定。

---

## 1. 问题的范围与候选方案

### 1.1 几个前提

- **UI 在架构中的位置。** 按 architect.md 2.1，桌面 UI 只渲染宿主下发的投影、发送命令；在本地只处理窗口、快捷键、主题、多语言、输入草稿和系统通知的展示，不接触 ACP、git 和 PTY。所以 ADR-03 不影响 M3 会不会返工（architect.md 1.4），只影响 M1/M2 的成本，以及 iOS 能复用多少。
- **首期只做 macOS**（PRD 第 2 节"非目标"）。这一点既削弱了 Electron 的跨平台优势，也削弱了 Tauri"各平台 WebView 不一致"的劣势，因为只需要对付 WKWebView 一种引擎。
- **复刻目标**（UI-6、截图笔记）：
  - 四列布局：图标栏约 40pt、侧边栏约 170pt、会话区内容最大宽度约 520pt、右侧浮起的圆角任务卡片；
  - 融合式标题栏，后退、前进、侧边栏开关都挨着红绿灯；
  - 大圆角（12–16pt），边框极少；
  - 半透明窗口材质（R01 §八）；
  - 系统字体（SF Pro，中文用苹方）。

  这个目标本身就是一个 Electron 应用。
- **代码主要由 AI 编码 agent 编写**，每个 agent 在自己的 worktree 里并行开发。

### 1.2 候选方案

| 代号 | 方案 | 说明 |
|---|---|---|
| E | Electron + React | 主进程加沙箱化的渲染进程，Chromium 引擎 |
| T | Tauri 2 + React | Rust 外壳，使用系统的 WKWebView |
| N | SwiftUI/AppKit 原生 | 需要时下沉到 AppKit（NSTableView、NSTextView） |
| H | 原生外壳 + WebView 混合 | SwiftUI/AppKit 负责窗口、侧边栏、工具栏、设置、菜单、通知；WKWebView 负责消息流、diff、终端 |

以下方案不纳入：
- Electrobun 和 Deno Desktop（2026-06 随 Deno 2.9 发布）：太新 [S18]。
- Flutter、Compose Multiplatform、React Native macOS：在上述几块关键组件上比 Web 生态更缺，除了 iOS 之外也没有额外好处。

---

## 2. 事实核查（截至 2026-10-16）

### 2.1 框架与关键组件的版本和状态

| 项目 | 当前版本（日期） | 状态与要点 | 来源 |
|---|---|---|---|
| Electron | 44.8.0（2026-10-16）；45 处于 beta，计划 2026-10-20 发布稳定版 | 44 内置 Chromium 152、Node 24.22.0。每 8 周发布一个大版本，官方只支持最新的 3 个；44 的支持截止日是 2027-03-02 | [S1][S2][S3] |
| Electron 42 的破坏性变更 | 42.0.0（2026-05-05） | macOS 通知改用 UNNotification，**应用必须签名，通知才会显示**；npm 包改为首次运行时才下载二进制 | [S4] |
| Electron 在 macOS 26 上的事故 | 2025-09/10 | 因覆盖私有 API `_cornerMask` 导致全系统卡顿，修复版本为 36.9.2、37.6.0、38.2.0 | [S9] |
| Electron Forge / electron-builder | Forge 8.0.1（2026-09-29）；electron-builder 26.17.0、electron-updater 6.8.10（2026-09-26） | 都已集成签名、公证和发布流程 | [S36] |
| Playwright 驱动 Electron | — | 官方标注为"experimental"，支持 Electron v21 及以上 | [S11] |
| Tauri | 2.12.4（2026-10-14）；3.0 处于 alpha（alpha.0 发布于 2026-09-13，alpha.5 发布于 2026-10-14） | v2 自 2024-10 起稳定。仓库里有 `feat/cef` 分支，但 CEF 后端没有进入发布版（根据仓库分支和 crate 列表推断） | [S12] |
| Tauri 窗口样式 | 2.12 | 支持 `titleBarStyle: Overlay`、`trafficLightPosition`、`windowEffects`（包括 sidebar，macOS 26 上还有 liquidGlassRegular/Clear）。`macOSPrivateApi` 从 2.12.1 起始终开启，配置项已无作用 | [S13] |
| Tauri 通知插件 | 2.6.0（2026-10-10） | 从这一版起，桌面端才上报通知点击和 action，而且是破坏性变更；文档页仍写着"Actions 仅移动端" | [S15] |
| Tauri 在 macOS 上的 WebDriver | — | `tauri-driver` 不支持 macOS；macOS 只能用 `@wdio/tauri-service` 内嵌驱动（需在应用里加插件），或付费的 CrabNebula | [S14] |
| Tauri 更新插件 | 2.14.0（2026-10-14） | 使用自己的签名密钥对，不能关闭；私钥丢失后无法再给已安装的用户推送更新 | [S16] |
| SwiftUI 内嵌 Web | macOS 26（WWDC25） | 新增原生的 `WebView` / `WebPage` API | [S31] |
| Sparkle | 2.10.0（2026-09-13） | 非 App Store 原生应用事实上的标准更新框架 | [S30] |
| streamdown | 2.7.0（2026-09-30），Apache-2.0 | 可直接替换 react-markdown，专为流式输出设计（remend 补全未闭合的块、按块缓存渲染结果、Shiki、GFM、KaTeX，Mermaid 是插件），用 rehype-harden 做清洗 | [S22] |
| @pierre/diffs | 1.5.2（2026-10-05），Apache-2.0 | 基于 Shiki；支持分栏和堆叠两种布局；有注释框架，可以插入行内评论；可以自定义接受/拒绝的界面；支持选中行 | [S23] |
| xterm.js | 6.0.0（2025-12-22） | VS Code 终端用的就是它，有 WebGL 渲染器 | [S24] |
| mermaid | 12.2.0（2026-10-13） | — | [S25] |
| Shiki | 4.5.0（2026-10-01） | — | [S26] |
| TanStack Virtual / react-virtuoso | virtual-core 3.19.5（2026-10-15）/ 4.18.19（2026-10-13） | 两者都是 MIT。react-virtuoso 专为聊天做的 Message List 是**商业许可** | [S27] |
| MarkdownUI / Textual | 2.4.1（2024-10，已进入维护模式）/ 0.5.0（2026-06-15） | Textual 是 MarkdownUI 的继任者，支持代码高亮和表格，还在 0.x 阶段，约 900 star | [S28] |
| SwiftTerm | v1.99.0（2026-10-08） | 原生终端组件，可用 | [S29] |

### 2.2 包体、内存、启动时间的大致数字

| 指标 | Electron | Tauri | 原生 | 来源 |
|---|---|---|---|---|
| 框架本体 | Electron 44.8.0 的 darwin-arm64 压缩包为 124 MB | 外壳通常几 MB 到十几 MB，渲染用系统自带的 WKWebView | 0（使用系统框架） | [S1][S18] |
| 同一个应用的三方实测（Better Stack，2026-07-20 更新，没有注明机型和框架版本） | 包体 323 MB，启动 273 ms，空闲内存 128 MB | 包体 57 MB，启动 311 ms，空闲内存 109 MB（主进程约 36 MB，其余是 WebKit 辅助进程） | — | [S18] |
| 真实产品 | 新版 ChatGPT 桌面 App（Electron）约 1.5 GB | — | 旧的原生版（ChatGPT Classic）159 MB | [S19] |
| Node 运行时（适配器需要，来源由 ADR-08 决定） | Node 24.22.0 darwin-arm64：xz 包 26 MB，gz 包 51 MB | 同左 | 同左 | [S35] |

怎么看这些数字：

- 网上常见的"Tauri 30 MB 对 Electron 300 MB 内存"这类说法偏营销。把 WebKit 的辅助进程算进去之后，空闲内存的差距在一个控制较好的测试里只有约 20 MB [S18]。包体的差距是真的。
- ChatGPT 的 1.5 GB 包含内置的 codex 二进制等组件（R01 §十二），不能全算在 Electron 头上。
- cupper 无论选哪种外壳都要带 Node 运行时（除非 ADR-08 决定使用系统 Node），所以 Tauri 在包体上的倍数优势会缩小（判断）。
- 和 agent 进程相比，UI 的内存差异不是 NFR-11 的主要矛盾（判断，待 SP-1/SP-4 实测）。

---

## 3. 按具体 UI 需求评估

混合方案（H）的内容区和"Web 组件"一栏相同，窗口外壳部分和"原生"一栏相同，下表不单独列出。

### 3.1 对照表

| 需求 | Web 组件（E、T 共用） | E 与 T 的差别 | 原生（N） |
|---|---|---|---|
| **流式 Markdown + 代码高亮**（MS-1） | streamdown 2.7（remend、按块缓存、Shiki、表格、KaTeX）[S22]；也可以用 react-markdown + Shiki 4.5 自己组装 | 没有本质差别；T 上要在 WebKit 里测一次高亮性能 | Textual 0.5 [S28]，支持高亮和表格。以下几点要自己验证：流式输出中未闭合的块、长文档性能、跨消息的连续选择（SwiftUI 的文本选择以单个视图为单位；Textual 声称支持原生选择，范围待确认） |
| **推理摘要、活动日志、计划、审批和提问卡片**（MS-2～4、AP-1、AP-5） | 自定义组件，配合 Radix 一类的无样式组件库；没有难点 | 无 | 自定义 SwiftUI 视图；没有难点 |
| **1000 条以上的长会话**（NFR-6） | TanStack Virtual 3 或 react-virtuoso 4（都是 MIT）[S27]。以下行为要自己处理：流式输出时最后一条变高仍然贴底、向前分页插入时锚点不动、"回到底部"按钮 | 没有本质差别；大 DOM 在 WebKit 和 Chromium 上的表现，要在 SP-6 里各测一次 | LazyVStack/List 对变高行的抖动、贴底和前插时保持位置，在 Apple 论坛上跨多个系统版本反复出问题（主要是 iOS 上的报告）[S32]。稳妥的做法是用 NSTableView 自己管理行高，代码量明显更大 |
| **diff 视图**（GT-1～3：按 hunk 操作、行内评论） | @pierre/diffs 1.5，注释框架可以直接放行内评论，也可以自定义接受/拒绝的界面 [S23]；备选 @git-diff-view/react、Monaco DiffEditor（体积大，但大文件稳定） | 无 | 没有主流的开源组件，要基于 NSTextView/TextKit 2 自己实现，或者嵌 WKWebView |
| **集成终端**（UI-1，M2） | xterm.js 6.0（WebGL）[S24]。PTY 由宿主持有，字节流经协议转发 | 无 | SwiftTerm v1.99 [S29]，可用 |
| **Mermaid**（MS-7，P1） | mermaid 12.2 [S25]，streamdown 有现成插件；懒加载，使用严格安全级别，隔离渲染 | 无 | 没有原生实现，只能嵌 WKWebView |
| **半透明材质 + 融合式标题栏**（UI-6） | 内容区由 CSS 负责，窗口材质由外壳提供 | **E**：`titleBarStyle: hidden/hiddenInset`、`trafficLightPosition`、`vibrancy: 'sidebar'` 等 [S5]。内容区在 CSS 里涂不透明底色，只让侧边栏透出材质，有现成的示例 [S10]。Liquid Glass 没有内建支持，第三方包依赖私有 API，不建议使用；截图里 Codex 的侧边栏是浅灰色，并不需要它。**T**：`Overlay`、`hiddenTitle`、`trafficLightPosition`、`windowEffects`（macOS 26 上有 liquidGlass）[S13]。Overlay 的已知限制：要自己定义拖动区域，窗口失焦时不能拖动 | NavigationSplitView 自带侧边栏材质和统一工具栏，macOS 26 上自动使用 Liquid Glass。三者中效果最好 |
| **中英文本地化**（UI-5） | i18next 或 FormatJS（支持 ICU 复数）配合 Intl 格式化时长和日期。宿主下发语义化的事件，文案只在 UI 里出现（architect.md 1.3 #8）；agent 给的选项文字原样显示 | E 用 `app.getPreferredSystemLanguages()` 获取系统语言，T 用 os 插件。两者的 App 包都要声明 zh-Hans 本地化，系统提供的菜单项和对话框才会显示中文（判断，需验证） | String Catalog（.xcstrings），系统菜单自动本地化 |
| **亮暗主题**（UI-3） | CSS 变量配合 `prefers-color-scheme`；Shiki 用双主题；xterm 和 Mermaid 的主题要跟着切换 | E 可以用 `nativeTheme` 强制指定；T 在窗口的 theme 设置里指定 | 自动跟随 |
| **快捷键**（UI-4） | 自己写一套 keymap，处理上下文相关的快捷键（Enter 批准、Esc 拒绝、↑ 恢复上一条等）；⌘K 用 cmdk | **E**：原生菜单的 accelerator；Chromium 的输入法事件正常。**T**：原生菜单（muda）。WKWebView 有一个输入法问题：用 Enter 确认候选词时，keydown 事件的 `isComposing` 是 false（WebKit bug 165004），必须同时判断 `keyCode === 229`，否则用中文输入法时会误发送 [S33] | `.keyboardShortcut` / `Commands`；NSTextView 对输入法的处理最稳 |
| **系统通知**（AP-3） | — | **E**：从 Electron 42 起使用 UNNotification，支持点击、action 按钮和回复；**必须签名**才会显示 [S4]。**T**：桌面端的点击和 action 从 2.6.0（2026-10-10）才开始支持 [S15] | UNUserNotificationCenter，功能完整 |
| **多窗口**（PS-7，P2） | 每个窗口都是宿主的又一个订阅者，状态天然一致（architect.md 2.5） | E 每个窗口一个渲染进程，T 每个窗口一个 WebKit 内容进程（判断：每个窗口多占几十 MB） | WindowGroup，开销最小 |
| **输入框**（CP-1～3） | 先用 textarea 加补全浮层（对输入法最稳）；需要内联 chip 时再引入 Lexical 或 TipTap；粘贴和拖入图片用标准 DOM 事件 | T 要处理上面提到的输入法问题 | NSTextView 加自定义补全，工作量中等 |

### 3.2 各方案缺什么

- **E 和 T 都缺的**都是胶水代码，没有一项需要自研底层组件：
  - 代码块工具条（复制、在编辑器中打开）和文件路径的链接化；
  - 把虚拟列表和"流式贴底、分页锚点"组合起来的逻辑；
  - 会话内查找（⌘F），依赖宿主提供搜索；
  - diff 上的 hunk 操作和宿主命令的对接；
  - 标题栏细节：红绿灯位置、双击缩放、拖动区域，要逐项对照截图；
  - 渲染进程的安全加固（第 7 节）。
- **T 额外缺的**：
  - macOS 上成熟的应用内端到端测试链路 [S14]；
  - 刚发布的通知点击回调；
  - 中文输入法 Enter 的兼容处理；
  - 开发时用 Chromium、运行时用 WebKit，两者在样式和性能上的差异。
- **N 缺的**：Mermaid、diff、成熟的流式 Markdown、跨消息文本选择、稳定的变高长列表。其中前两项实际上只能靠嵌 WebView 解决。
- **H 缺的**：
  - 外壳和内容区之间的协作：焦点、快捷键、拖放、右键菜单、查找和主题同步都要自己定义一套规则；
  - 原生和 Web 两份状态之间的同步；
  - 测试要覆盖两套技术栈。

---

## 4. 按议程的 8 条标准逐条比较

评分 1–5 分，5 分最好。下表是不加权的分数，我认为第 1、3、4 条的权重最高。

| 标准 | E Electron | T Tauri | N 原生 | H 混合 |
|---|---|---|---|---|
| 1 复刻保真度与视觉打磨成本 | 5 | 4 | 3 | 3 |
| 2 M3 时与 iOS 共享代码 | 2 | 3 | 4 | 3 |
| 3 关键依赖的生态成熟度 | 5 | 4 | 2 | 3 |
| 4 AI agent 的效率和出错率 | 5 | 3 | 2 | 2 |
| 5 性能和资源占用 | 3 | 4 | 4 | 4 |
| 6 签名、公证、更新、分发 | 4 | 4 | 5 | 5 |
| 7 开源贡献者上手难度 | 5 | 4 | 3 | 2 |
| 8 长期维护成本 | 3 | 3 | 4 | 2 |
| **合计（不加权）** | **32** | **29** | **27** | **24** |

**4.1 保真度。**
- 据 R01 §八和 §十二的第三方逆向（推测），Codex 的技术栈是 React 19、Tailwind v4 风格的工具类加 CSS Modules、OKLCH 色彩 token。截图里的大圆角、浮起的卡片、极少的边框、活动日志底部的渐隐遮罩，都是 CSS 的强项。字体写 `-apple-system, "PingFang SC"`，就能和截图一致。
- T 用的是同一套 CSS，但 Overlay 标题栏有限制，WebKit 和 Chromium 也有差异，需要额外打磨。
- N 的原生观感最好，但我们的目标恰恰不是原生外观。要把 SwiftUI 的默认样式改成 Codex 的样子（列表行、气泡、输入框、浮起的卡片），需要大量自定义；Mermaid 和 diff 则做不到。
- 旁证：新版 ChatGPT App 发布时，有评论认为旧的原生版"looks more native Mac-like" [S19]，说明两种观感确实看得出差别。

**4.2 iOS 共享。**
- 协议类型在任何方案下都要为 Swift 生成，这一点各方案持平。差别在"同步客户端核心"（握手、游标续传、命令幂等、投影 reducer）和渲染组件这两块。
- N：这两块都能和 SwiftUI 版的 iOS 共享。粗估 iOS MVP 客户端代码的三到五成可以复用（判断）。
- E/T：同步核心用 TS 写，iOS 用不上，除非 iOS 也采用 React Native 或 WebView。渲染组件可以单独打包放进 iOS 的 WKWebView，前提是 CI 里也用 WebKit 跑一遍渲染测试。
- T 还多一个 iOS 构建目标。CodexMonitor 正在用 Tauri 把同一套 React UI 做到 iOS 上，目前是 WIP [S21]。但通知扩展（NSE）和钥匙串仍然要用原生代码。
- 另外要注意，architect.md 1.4 已经指出 UI 技术不影响 M3 会不会返工，而且 M3 的 iOS 范围很小（MB-1～7）。

**4.3 生态。**
- UI 不直接接触 ACP（ACP 在宿主的驱动层），这里比较的是 UI 组件的生态。
- 2026-10-06 到 10-12 一周的 npm 下载量 [S34]：electron 805 万，@tauri-apps/api 304 万，streamdown 737 万，@pierre/diffs 361 万，@xterm/xterm 562 万，mermaid 1878 万，shiki 2592 万，@tanstack/react-virtual 2798 万。
- 原生侧：Textual 约 900 star，SwiftTerm 约 1700 star，diff 和 Mermaid 没有可用的组件。
- 同类产品的选择：Codex 用 Electron（R01）；Conductor 用 Tauri [S20]；CodexMonitor 用 Tauri 2 + React 19 [S21]。

**4.4 AI agent。** 见第 5 节。

**4.5 性能。**
- 冷启动：三者都能做到 2 秒以内。Electron 空窗口启动只要几百毫秒 [S18]。
- 关键是 NFR-1 规定 UI 不能自己读写会话数据，所以冷启动时间等于"外壳启动"和"宿主启动加首屏快照"中较慢的一路，宿主启动也计入这 2 秒。
- 流式输出卡不卡，取决于渲染管线，三个点做到就够了：
  - 按帧合并 delta；
  - Markdown 只重新解析最后一个块；
  - Shiki 和 Mermaid 懒加载。
- 在 Electron 上，主进程不能出现在数据路径上（见第 7 节）。
- 资源占用：N 理论上最省，但长列表有风险；T 比 E 小一些。

**4.6 分发。**
- 四种方案都需要 Developer ID（付费开发者账号）和公证。打包进去的辅助可执行文件（宿主、Node 运行时）都要用 hardened runtime 签名，并一起公证。这与 UI 选型无关，属于 ADR-08 的范围。
- E：electron-builder 或 Forge 都集成了签名和 notarytool [S36]。自动更新可以用内置的 Squirrel.Mac：update.electronjs.org 对公开的 GitHub 仓库免费，要求构建已签名 [S6]；也可以用 electron-updater，它支持 beta 通道。要注意的是，从 Electron 42 起通知必须签名才能显示，贡献者本地的未签名构建收不到通知 [S4]。
- T：`tauri build` 读取环境变量，自动完成签名和公证 [S17]。更新插件强制使用自己的密钥对 [S16]。
- N/H：xcodebuild 加 notarytool 加 Sparkle 2.10 [S30]，是最标准的组合。

**4.7 贡献者。**
- E：只需要 Node 和 pnpm。
- T：还要装 Rust 工具链。
- N：要装 Xcode。
- H：两者都要。

**4.8 长期维护。**
- E：每 8 周一个大版本，只支持最新的 3 个 [S2][S3]，相当于大约每半年必须升级一次。2025 年在 macOS 26 上的卡顿事故 [S9] 说明，从第一个公开版本起就要有自动更新。
- T：v3 正在 alpha [S12]，预计一年内要迁移一次。WKWebView 的版本跟着用户的系统和 Safari 走，不由我们控制。
- N：每年都要适配系统更新，SwiftUI 在不同系统版本上的行为也不一样。
- H：两套栈，维护成本叠加。

---

## 5. AI 编码 agent 的视角

### 5.1 训练数据和常见的出错方式（判断）

- **React + TS**：训练语料最多，Codex 本身也是 React。agent 常见的错误有：流式列表里的重复渲染风暴、不稳定的 key、副作用写错。这些都能被测试和性能剖析发现。
  - Electron 特有的问题是：agent 常写出不安全的老写法，比如 `nodeIntegration: true`、`remote` 模块、把 `ipcRenderer` 整个暴露给页面。对策是一开始就搭好安全模板，再加上 lint 规则和安全测试。
- **Tauri**：前端部分和上面一样。Rust 和 Tauri API 这一层，v1 到 v2 的变化很大（allowlist 改成了 capabilities/permissions，模块路径也变了），现在又有 v3 alpha，agent 容易把几个版本的 API 混着用。Rust 的编译周期更长，但编译器的报错信息对 agent 很有用。
- **SwiftUI**：语料少，macOS 专属的部分更少。不同系统版本的 API 可用性不同。agent 容易在 Swift 6 严格并发检查（Sendable、actor 隔离）的报错上来回打转。agent 编辑 `.pbxproj` 文件容易出错，并行 worktree 时还会冲突，需要改用 SwiftPM 或 XcodeGen/Tuist 生成工程。

### 5.2 自动化验证

- **E**：
  - 渲染进程就是普通网页，可以在 Chromium 里接一个模拟宿主运行。
  - 单元测试和组件测试用 Vitest。
  - 截图回归用 Playwright 在无头 Chromium 里跑，和生产环境是同一个引擎，结果确定，适合 UI-6 复刻时反复对照截图。
  - 端到端测试用 Playwright `_electron`（实验性 [S11]），配合真实宿主和假 agent（architect.md 2.1）。
  - agent 还可以通过 `--remote-debugging-port` 用 CDP 连上正在运行的 App，查看 DOM、截图、采集性能 trace。
- **T**：浏览器层的测试可以照搬，但测试引擎（Chromium）和生产引擎（WebKit）不一致，除非改用 Playwright 的 WebKit。macOS 上的应用内端到端测试只能用 `@wdio/tauri-service` 的内嵌驱动 [S14]，这个方案较新，agent 熟悉的人也少。
- **N**：XCTest 做单元测试，swift-snapshot-testing 做截图测试；XCUITest 慢，也更容易不稳定，而且需要图形会话，在 CI 上无头运行更麻烦。

### 5.3 并行 worktree 下的开发循环

- **Web 方案**：每个 worktree 跑一次 `pnpm i`，pnpm 的全局存储用硬链接，安装很快；然后起 Vite，热更新也快。多个 agent 并行开发的成本很低。Electron 的端到端测试会真的打开窗口，所以要串行执行，或者放到 CI 上跑。
- **原生方案**：每个 worktree 有自己的 DerivedData，一次 xcodebuild 要好几分钟，开销大得多。

**结论**：从 AI agent 的效率看，E > T > N ≈ H。

---

## 6. 推荐，以及什么情况下改变推荐

### 6.1 推荐

ADR-03 提议采用 **Electron + React 19 + TypeScript**，具体组合见第 11 节。

### 6.2 为什么不是 Tauri，以及怎么保留切换的余地

在只做 macOS 的前提下，Tauri 和 Electron 的差距不大。我选 Electron，是因为下面这些事情对 cupper 的影响比包体大：

- 测试和生产用同一个引擎；
- agent 可以通过 CDP 和 Playwright 自动验证界面；
- 通知、中文输入法这些都不需要额外处理；
- Codex 本身就是 Electron，复刻时可以直接对照。

为了保留切换的余地，UI 代码分成两层：

- `ui-core`：纯 React，不调用任何外壳 API；
- `ShellAPI`：外壳接口，包括窗口控制、菜单、通知、主题、语言、打开外部链接、剪贴板图片、更新。Electron 版本通过 preload 实现，另有一个浏览器 mock 版本供测试使用。

这样将来如果切到 Tauri，只需要重写外壳层和 Rust 胶水，界面代码不用动。

### 6.3 什么情况下改变推荐

1. **用户的优先级不同。** 如果用户更看重"原生 Mac 观感"而不是"像 Codex"，或者对安装包大小（例如要求小于 50 MB）、空闲内存有硬性要求：在乎体积就改选 T，在乎观感就改选 N。
2. **ADR-02 选了 Rust，并且希望减少语言种类。** 这时 T 的 Rust 胶水层可以直接复用宿主的 crate 和类型（例如用 specta 生成 TS 类型），我会结合 SP-6 的结果，重新比较 E 和 T。
3. **SP-6 的结果。** 如果 WebKit 在性能和输入法上与 Chromium 持平，并且 `@wdio/tauri-service` 能在 CI 上稳定运行，T 就可以接受。反过来，如果 Electron 达不到 NFR-6 的门槛（我认为可能性很小），就要重新评估。
4. **ADR-15 的方向。** 如果 ADR-15 决定 iOS 完全用 SwiftUI 原生开发，并且把"与桌面共享同步核心和渲染组件"看得比 M1 的速度更重要，就改选 N 或 H。前提是原生版消息列表的原型通过了 SP-6。
5. **维护能力。** 如果没人能保证每半年升级一次 Electron 大版本，Electron 的安全风险会上升，这时 T（WebKit 随系统更新）更省心。

---

## 7. Electron 方案的进程结构与安全要点

**进程分工：**
- **主进程**：负责窗口、菜单、通知、更新，以及拉起并监护宿主（ADR-01 方案 b）。主进程不在数据路径上，否则它一卡，整个窗口和菜单都会卡。
- **utilityProcess（宿主桥）**：持有 Unix 域套接字和本地凭据（SEC-8），负责分帧、校验和合并 delta，通过 `MessageChannelMain` 把 MessagePort 交给渲染进程，与之直接通信 [S8]。凭据始终不进入渲染进程。
- **渲染进程**：开启 `sandbox` 和 `contextIsolation`。preload 只暴露有类型的 `host.subscribe` / `host.command`，不暴露原始的 `ipcRenderer`。

**Fuses 设置** [S7]：
- 关闭 `runAsNode` 和 `nodeCliInspect`；
- 开启 `embeddedAsarIntegrityValidation` 和 `onlyLoadAppFromAsar`；
- 如果不用 `file://` 加载页面，关闭 `grantFileProtocolExtraPrivileges`。

其中关闭 `runAsNode` 很重要：不关的话，任何本地进程都可以借我们已签名的二进制来执行任意 JS。

**威胁：agent 输出被利用来"自我批准"。** agent 产生的 Markdown、文件内容、命令输出、Mermaid 源码、diff，对渲染进程来说都是不可信的输入。如果其中的脚本能在渲染进程里执行，它就能以 UI 的满权限身份发出批准命令，或者把会话切到完全访问，绕过 SEC-8 的初衷。对策如下：
- 严格的 CSP：禁止内联脚本和 eval；
- Markdown 默认不渲染原始 HTML，并做清洗（streamdown 内置 rehype-harden [S22]）；
- Mermaid 用 strict 安全级别，在沙箱 iframe 里渲染；
- 链接一律在外部浏览器打开，只允许 http/https；
- 禁止页面导航和新开窗口；
- 启用 Trusted Types。

协议层面，批准命令已经要带上请求内容的摘要（architect.md 1.3 #5），这一条保留。还要不要加别的防线，请安全工程师给意见（见第 12 节）。T 和 H 面临同样的威胁；纯原生方案的风险低得多，只有嵌入的 WKWebView 部分需要同样处理。

---

## 8. 对 ADR-02（宿主语言）的意见

从 UI 开发的角度比较三个候选：

| 维度 | TypeScript/Node | Rust | Swift |
|---|---|---|---|
| 与 UI 共享协议类型 | 同一个 TS 包，不需要代码生成 | 需要生成：可以用 ts-rs/specta，也可以从 schemars 导出 JSON Schema 再生成 TS。Codex 的 app-server 就是从 Rust 生成 TS 类型和 JSON Schema 的先例（R01 §十二） | 需要生成 TS |
| 工具链和测试 | 一个 pnpm monorepo，一种测试框架 | Cargo 和 pnpm 两套 | SwiftPM/Xcode 和 pnpm 两套 |
| agent 切换语言的成本 | 无 | 中 | 中 |
| 宿主可执行文件 | 需要 Node 运行时（可以和适配器共用，由 ADR-08 决定），或打包成单个可执行文件 | 单个静态二进制，升级时替换起来最简单 | 单个二进制 |
| 和 iOS 共享 | 给 iOS 生成 Swift 协议类型；加密要在两边各实现一遍，用测试向量互相校验 | 可以通过 UniFFI 把加密和协议客户端交给 iOS 使用 | 可以直接共享，但宿主侧没有官方 ACP SDK（R02 §2） |

**我的意见：**
- UI 侧倾向 TS 宿主（Node 24 LTS）。两个适配器和官方 ACP TS SDK 都是 TS（R02 §2），Node 反正要带。
- 如果架构师和移动端工程师因为加密共享、单个二进制等理由选 Rust，UI 侧可以接受。条件是：以 schema 为唯一来源，生成 TS 类型，并在 CI 里校验生成结果没有过期。
- 不建议用 Swift 写宿主。

**和语言无关、但会影响 UI 的两点，请 ADR-01/08 一起考虑：**

1. **不要用 Electron 的二进制来跑宿主**（即 `ELECTRON_RUN_AS_NODE`）。这个 fuse 应该关掉（第 7 节），宿主要有自己的运行时。
2. **宿主可执行文件最好放在 .app 包外面。** 原因是：Squirrel.Mac 和 Sparkle 升级时会替换整个 .app，而 NFR-8 要求"有回合在运行时推迟重启宿主"，P1 的后台常驻还要求关掉窗口后宿主继续运行。如果宿主在运行期间还要从包里懒加载代码（例如 Node 的模块），包被替换后就可能出错。
   - 我的建议是：App 首次启动或升级后，把宿主连同锁定版本的 Node 和适配器安装到 `~/Library/Application Support/cupper/runtime/<version>/`。旧版本的宿主跑完当前回合，空闲时再切到新版本。
   - 另一个办法是保证宿主在启动时就把所需代码全部加载完。
   - 这两种做法都要在 SP-4 里验证。

---

## 9. 对 ADR-04（宿主协议）的意见

**编码与分帧：**
- M1 用 JSON。分帧用 4 字节长度前缀，比 NDJSON 好，因为将来换成 CBOR 时不用改分帧方式。
- 消息外层可以借用 JSON-RPC 的形状，但订阅和事件流的语义要按 architect.md 2.3 自己定义。

**以 schema 为唯一来源：**
- 一份 schema 生成以下三样：TS 类型加运行时校验（UI 在开发构建里校验每条消息）；M3 用的 Swift Codable；如果宿主用 Rust，再生成 Rust 类型。
- schema 的写法取决于宿主语言：TS 宿主用 TypeBox 或 zod 编写并导出 JSON Schema；Rust 宿主用 schemars 或 ts-rs。
- CI 里校验生成的代码有没有过期。
- 维护一套共享的事件夹具（录制好的事件序列），UI 测试、cupper-cli 测试和将来的 iOS 测试都用它，作为协议一致性测试。

**UI 对协议语义的要求：**

1. 条目按 ID upsert 并带版本号；任何时候都可以请求单个条目或整个会话的快照（UI 重载、新开窗口时要用）。
2. delta 可以合并：宿主按时间窗（例如 30–50 ms）合并同一条目的 delta，UI 再按帧合并一次。
3. 历史向前分页（`history.page`），以条目为单位，返回稳定的 ID。UI 只保留可视窗口附近的条目。
4. 大内容用引用（`blob.get`），包括命令输出、整文件 diff 和图片。条目里要带摘要信息（行数、字节数、退出码、diff 统计），这样 UI 不拉 blob 也能渲染折叠状态。
5. 会话内搜索（⌘F）由宿主提供，返回条目 ID 和匹配位置。
6. 侧边栏使用的索引流采用"快照加增量"。后台会话只订阅状态，不订阅全部事件；切换到某个会话时再订阅它的事件流。
7. 事件内容是语义化的，不带本地化文案（architect.md 1.3 #8）。agent 提供的文字原样透传，并标明来源。
8. 未知的类型和字段可以忽略。UI 遇到未知的条目时，显示"当前版本不支持此内容"的占位。
9. `welcome` 消息带上宿主的当前时间，UI 用它校正本地的实时计时，例如"用时 8m 31s"。iOS 也需要这一条。
10. 每条命令带 `command_id`。需要乐观更新的命令（发送、排队、批准）由宿主回执确认；冲突时返回 `conflict`。
11. 新建会话页在发出第一条消息之前，就能拿到能力描述（模型、推理强度、权限模式、斜杠命令；AG-4），每一项都带稳定的 ID 和 agent 原文的标签。
12. 终端（M2）使用单独的流类型，允许二进制帧或批量 base64，不要逐字节包成 JSON。

---

## 10. SP-6：要不要做，测什么

**我的立场**：ADR-03 可以先按 Electron 提议，不必等 SP-6。但建议在 M1b 打磨界面之前做 SP-6，目的有三个：

- 验证渲染管线的设计（不论选哪个外壳都需要）；
- 拿到实测的数字；
- 如果讨论会在 E 和 T 之间僵持，用数据来定。

SP-6 使用模拟宿主，可以和 SP-4 并行，不阻塞 M1a。工期估计 3–5 个工作日（判断）。

**形式**：
- 做一个 React 原型，配一个模拟宿主。模拟宿主按合成的事件序列回放，速率可调。
- 同一份 bundle 分别装进 Electron 44 和 Tauri 2.12 两个外壳。
- 只有在原生方案仍是候选的情况下，才另做一个 SwiftUI 版的消息列表（Textual 配合 LazyVStack 或 NSTableView）。

**数据集**：
- 用合成的会话，分 1,000 条和 5,000 条两档。
- 内容包括：助手的 Markdown 回复（含 50–300 行的代码块和表格）、单行活动日志、展开的命令输出（最多 5,000 行）、diff 摘要、10 个 Mermaid 图，中英文混排。
- SP-1 需要用户许可才能使用真实会话，所以 SP-6 先用合成数据。

**场景与建议门槛**（门槛供讨论）：

| # | 场景 | 指标 | 建议门槛 |
|---|---|---|---|
| 1 | 冷启动 | 从启动到"会话列表和最近会话的最后一页可见、输入框可以输入" | p95 ≤ 2 s，包含宿主启动时间 |
| 2 | 单会话流式输出 | 100 token/s，持续 5 分钟，会话处于可见状态 | 没有超过 50 ms 的长任务；输入框从按键到上屏 p95 ≤ 50 ms |
| 3 | 多会话并发 | 6 个会话同时流式输出，其中 1 个可见 | 同上，另外记录 CPU 占用 |
| 4 | 长会话滚动 | 1,000 条和 5,000 条，快速拖动和惯性滚动 | 掉帧少于 5%；白屏不超过 100 ms |
| 5 | 历史分页 | 向上滚动，触发分页插入 | 锚点漂移为 0 px |
| 6 | 贴底与回到底部 | 流式输出时用户向上滚动，再点"回到底部" | 行为正确，不跳动 |
| 7 | 大 diff | 3,000 行的单文件分栏显示，加 200 个文件的列表 | 首次渲染 ≤ 300 ms；选择 hunk 时不卡顿 |
| 8 | Mermaid | 10 个图 | 渲染不阻塞滚动 |
| 9 | 内存 | 空闲；打开 5,000 条的会话；流式输出浸泡 1 小时；开两个窗口 | 统计全部进程（包括 WebKit 和 GPU 辅助进程）；内存不持续增长 |
| 10 | 中文输入法 | 用拼音输入时按 Enter 确认候选词 | 不误发送；Chromium 和 WebKit 都要测 |
| 11 | 视觉 | 半透明侧边栏、融合式标题栏、亮暗切换 | 与截图对照；切换主题时不闪白 |
| 12 | 终端（可选，M2 之前做） | xterm.js 承接 50 MB 的突发输出 | 界面保持可以交互 |

**测试机器**：支持范围内最老的 Apple Silicon Mac（例如 M1、8 GB 内存），外加一台当前机型。最低支持的 macOS 版本需要先定下来（见第 12 节）。

---

## 11. 推荐组合

| 层 | 选择 |
|---|---|
| 外壳 | Electron 44.x，跟随最新的稳定版，始终保持在官方支持的 3 个大版本之内 |
| 前端 | React 19 + TypeScript + Vite；Tailwind v4 配合 CSS 变量，亮暗各一套 token；菜单、弹层、对话框用 Radix 一类的无样式组件；⌘K 用 cmdk（P1） |
| 状态 | 宿主投影 store：按条目 ID 归一化存储，通过 `useSyncExternalStore` 细粒度订阅，按帧合并 delta |
| 消息流 | streamdown 2.x（Shiki 4，语言包懒加载）+ TanStack Virtual 3 |
| diff | @pierre/diffs 1.5，用它的注释框架实现行内评论；超大文件的备选是 Monaco DiffEditor |
| 终端（M2） | xterm.js 6（WebGL 渲染）；PTY 由宿主持有 |
| Mermaid（P1） | mermaid 12，懒加载，strict 安全级别，隔离渲染 |
| 本地化 | i18next（或 FormatJS）+ Intl |
| 进程与安全 | 主进程 + utilityProcess 宿主桥 + 沙箱化渲染进程；按第 7 节设置 fuses，配置 CSP 并清洗内容 |
| 打包与更新 | electron-builder 26 + electron-updater 6，通过 GitHub Releases 发布，支持 beta 通道。备选是 Electron Forge 8 + update.electronjs.org。用 Developer ID 签名，用 notarytool 公证 |
| 测试 | Vitest；Playwright 在 Chromium 里跑截图回归，配合事件夹具；Playwright `_electron` 做端到端测试，配合真实宿主和假 agent；可选再用 WebKit 跑一遍渲染组件测试，为 iOS 复用留余地 |
| 外壳抽象 | `ShellAPI` 接口，提供 Electron 实现和浏览器 mock，保留将来切到 Tauri 的余地 |
| ADR-02 | 倾向 TS 宿主（Node 24 LTS）：使用独立的运行时，不借用 Electron 的 RunAsNode；宿主装到 .app 包外的版本化目录（待 ADR-01/08 确认） |
| ADR-04 | JSON 加 4 字节长度前缀分帧；以 schema 为唯一来源，生成 TS 类型和 M3 用的 Swift 类型；CI 校验生成结果是否过期；共享事件夹具 |

---

## 12. 希望其他参会者回应的问题

**请架构师回应：**

1. 多窗口时，每个窗口各开一条到宿主的连接（各自作为一个 principal），还是共用宿主桥的一条连接、在里面复用多路订阅？
2. 宿主能不能按时间窗合并 delta，并且支持按条目请求快照（第 9 节第 1、2 条）？
3. 会话内搜索放在宿主，以及条目里带摘要信息、方便 UI 渲染折叠状态，你是否同意（第 9 节第 4、5 条）？
4. 第 8 节的升级问题：auto-update 替换 .app 包，和"推迟重启宿主"、后台常驻怎么共存？宿主装到包外的版本化目录，你能不能接受？
5. hunk 的暂存和还原，宿主能不能提供稳定的 hunk ID 和内容摘要，作为命令的前置条件？
6. NFR-1 规定 UI 不能缓存会话数据，那么冷启动的 2 秒里，你给宿主启动和首屏快照留多少预算？

**请移动与安全工程师回应：**

1. iOS 是完全用 SwiftUI 原生，还是在 WKWebView 里复用桌面的 Markdown 和 diff 渲染组件？这决定了我们要不要让渲染组件在 CI 上也通过 WebKit 测试。
2. 第 7 节"agent 输出导致渲染进程 XSS，进而自我批准"的威胁，除了 CSP、清洗和内容摘要这些措施，你还要求什么？例如批准命令必须由用户的真实操作触发，或者由主进程弹出原生确认。
3. 桌面通知上能不能带"批准"按钮？从安全角度看能接受吗？
4. 如果宿主用 TS，加密在宿主端用 libsodium 或 Node crypto、在 iOS 端用 CryptoKit 或 swift-sodium，双方用测试向量互相校验。你能接受吗？还是一定要用 Rust 配合 UniFFI 共用同一份实现？

**请编排者转给用户确认：**

1. "像 Codex"和"像原生 Mac 应用"哪个优先？
2. 能不能接受 Electron 版本的安装包在 150–300 MB 这个量级（判断）？
3. 最低支持哪个 macOS 版本？这个决定会影响 SwiftUI API 的可用范围、WKWebView 的引擎版本，以及 SP-6 用什么机器测。

---

## 来源

查证日期都是 2026-10-16；括号里是资料本身的日期。

| 编号 | 内容 | 链接 |
|---|---|---|
| S1 | Electron releases：44.8.0（2026-10-16）、45.0.0-beta.5；DEPS 显示 44.8.0 内置 Chromium 152.0.7977.167、Node v24.22.0；darwin-arm64 压缩包 124 MB | https://github.com/electron/electron/releases |
| S2 | Electron 发布时间表：42 至 46 的稳定版和支持截止日期 | https://releases.electronjs.org/schedule |
| S3 | Electron Timelines：只支持最新的 3 个稳定大版本 | https://www.electronjs.org/docs/latest/tutorial/electron-timelines |
| S4 | Electron 42.0.0 发布说明（2026-05-05）；Notification API 文档 | https://releases.electronjs.org/release/v42.0.0 ；https://www.electronjs.org/docs/latest/api/notification |
| S5 | BaseWindow 文档：titleBarStyle、trafficLightPosition、vibrancy | https://www.electronjs.org/docs/latest/api/base-window |
| S6 | Electron 更新文档：update.electronjs.org 的条件 | https://www.electronjs.org/docs/latest/tutorial/updates |
| S7 | Electron Fuses | https://www.electronjs.org/docs/latest/tutorial/fuses |
| S8 | utilityProcess | https://www.electronjs.org/docs/latest/api/utility-process |
| S9 | AppleInsider（2025-10-10）：Electron 在 macOS 26 上卡顿的修复版本 | https://appleinsider.com/articles/25/10/10/update-your-slack-discord-clients-the-electron-tahoe-gpu-slowdown-bug-is-fixed |
| S10 | Electron 侧边栏材质示例；electron-liquid-glass（第三方，使用私有 API，信息来自搜索摘要） | https://github.com/davidcann/electron-tinted-with-sidebar ；https://github.com/meridius-labs/electron-liquid-glass |
| S11 | Playwright 的 Electron 支持（experimental，v21 及以上） | https://playwright.dev/docs/api/class-electron |
| S12 | Tauri releases：2.12.4、3.0.0-alpha.0～alpha.5；仓库分支 feat/cef、v3 | https://github.com/tauri-apps/tauri/releases |
| S13 | Tauri 配置参考：Overlay 的限制、trafficLightPosition、windowEffects（含 liquidGlass）、macOSPrivateApi | https://v2.tauri.app/reference/config/ |
| S14 | Tauri WebDriver 文档 | https://v2.tauri.app/develop/tests/webdriver/ |
| S15 | Tauri 通知插件文档；2.6.0 更新日志（2026-10-10） | https://v2.tauri.app/plugin/notification/ ；https://v2.tauri.app/release/notification/v2.6.0/ |
| S16 | Tauri 更新插件文档；plugins-workspace 中 updater 2.14.0（2026-10-14） | https://v2.tauri.app/plugin/updater/ |
| S17 | Tauri macOS 签名与公证 | https://v2.tauri.app/distribute/sign/macos/ |
| S18 | Better Stack：Tauri、Electron、Deno、Electrobun 实测（2026-07-20 更新，未注明机型和版本） | https://betterstack.com/community/guides/scaling-nodejs/tauri-vs-electron-vs-deno-vs-electrobun/ |
| S19 | Daring Fireball（2026-07-09）：新版约 1.5 GB，原生版 159 MB；Michael Tsai 汇总（2026-07-10） | https://daringfireball.net/linked/2026/07/09/todays-the-day-openai-fucked-up-the-chatgpt-mac-app ；https://mjtsai.com/blog/?p=52579 |
| S20 | Conductor 用 Tauri 开发（来自创始人的发布帖，经搜索摘要转述，未逐字核对） | https://threadreaderapp.com/thread/1945870105109246401.html |
| S21 | CodexMonitor README：Tauri + React 19，基于 app-server，iOS 支持为 WIP | https://github.com/Dimillian/CodexMonitor |
| S22 | streamdown README、2.7.0（2026-09-30）、Apache-2.0 | https://github.com/vercel/streamdown |
| S23 | @pierre/diffs README、1.5.2（2026-10-05）、Apache-2.0 | https://github.com/pierrecomputer/pierre |
| S24 | xterm.js 6.0.0（2025-12-22） | https://github.com/xtermjs/xterm.js/releases |
| S25 | mermaid 12.2.0（2026-10-13） | https://github.com/mermaid-js/mermaid/releases |
| S26 | Shiki 4.5.0（2026-10-01） | https://github.com/shikijs/shiki/releases |
| S27 | TanStack Virtual；react-virtuoso（MIT）；Virtuoso Message List 为商业许可 | https://github.com/TanStack/virtual ；https://github.com/petyosi/react-virtuoso ；https://virtuoso.dev/message-list/ |
| S28 | MarkdownUI（维护模式）；Textual 0.5.0（2026-06-15） | https://github.com/gonzalezreal/swift-markdown-ui ；https://github.com/gonzalezreal/textual |
| S29 | SwiftTerm v1.99.0（2026-10-08） | https://github.com/migueldeicaza/SwiftTerm |
| S30 | Sparkle 2.10.0（2026-09-13） | https://github.com/sparkle-project/Sparkle/releases |
| S31 | WWDC25 Session 231：Meet WebKit for SwiftUI | https://developer.apple.com/videos/play/wwdc2025/231/ |
| S32 | Apple 开发者论坛：LazyVStack 变高行的抖动，以及 iOS 26/27 上的锚点问题 | https://developer.apple.com/forums/thread/685461 ；https://developer.apple.com/forums/thread/848608 |
| S33 | WebKit bug 165004（输入法 Enter 的 isComposing）；MDN keydown 文档 | https://bugs.webkit.org/show_bug.cgi?id=165004 ；https://developer.mozilla.org/en-US/docs/Web/API/Element/keydown_event |
| S34 | npm 一周下载量（2026-10-06 至 10-12） | https://api.npmjs.org/downloads/point/last-week/electron （其他包同格式） |
| S35 | Node.js 24.22.0 发布文件：darwin-arm64 的 tar.xz 为 27,503,188 字节，tar.gz 为 53,049,576 字节 | https://nodejs.org/dist/v24.22.0/ |
| S36 | Electron Forge 8.0.1（2026-09-29）；electron-builder 26.17.0、electron-updater 6.8.10（2026-09-26） | https://github.com/electron/forge/releases ；https://github.com/electron-userland/electron-builder/releases |
| 内部 | R01 §八、§十二（Codex 的技术栈，第三方逆向，推测）；R02 §2、§8；architect.md 1.3、1.4、2.1、2.3、2.5、2.6；PRD v1.0 | 仓库 `docs/` |
