# ADR-03 桌面 UI 技术

- 状态：已采纳（2026-10-16，`gate/design`）；编排者结论，依据技术选型讨论会、R04、R05
- 日期：2026-10-16
- 来源：`docs/meetings/2026-10-16-tech-selection/`（`minutes.md`，尤其是 C5；`desktop-engineer.md`；`desktop-engineer-round2.md`；`architect.md` ADR-03；`mobile-security-engineer-round2.md` 问题 2、3）

## 背景

PRD 要求复刻 Codex 视图（UI-6），包括：
- 流式 Markdown（MS-1）
- 长会话流畅滚动（NFR-6）
- 支持按 hunk 操作和行内评论的 diff（GT-1～3）
- Mermaid（MS-7）
- 集成终端（UI-1，M2）
- 中英文本地化，亮暗主题

桌面 UI 只是会话宿主的一个客户端，不碰 ACP、Git 和 PTY（NFR-1），所以 UI 技术选型不影响 M3 是否返工，只影响 M1、M2 的成本。代码主要由 AI 编码 agent 编写。

## 可选方案

1. Electron + React
2. Tauri 2 + React
3. SwiftUI/AppKit 原生
4. 原生外壳 + WKWebView 混合

方案 3 缺少 Mermaid、diff 组件和成熟的流式 Markdown，这些最后还是要嵌 WebView，等于变成方案 4。方案 4 要维护两套 UI 技术栈，成本最高。

## 决策

**采用 Electron + React 19 + TypeScript。Tauri 2 为备选。**

选择理由：
- Codex 本身是按 Electron 编程模型写的 React 应用（macOS 版现在跑在 OpenAI 自研的 Chromium 运行时 OWL 上，见 R04），用同一套技术复刻，保真度最高；
- Web 生态有成熟的组件；
- 测试和生产用的是同一个引擎；
- AI agent 可以用 Playwright 和 CDP 自己验证界面效果。

**组件选型（初版）**

| 用途 | 组件 |
|---|---|
| 构建 | Vite |
| 样式 | Tailwind v4 配合 CSS 变量 |
| 无样式组件 | Radix 一类 |
| 流式 Markdown | streamdown（Shiki 懒加载） |
| 虚拟滚动 | TanStack Virtual |
| diff | @pierre/diffs |
| Mermaid（P1） | mermaid，用 strict 安全级别，放在沙箱 iframe 里渲染 |
| 终端（M2） | xterm.js |
| 本地化 | i18next 或 FormatJS，加 Intl |

**进程结构**
- **主进程**持有到宿主的全部连接：一条自用的 control 连接，加上每个窗口一条数据连接（绑定到窗口的文档实例）；每个窗口经自己的 MessagePort 收发。套接字、令牌和签名校验都不进渲染进程。（设计评审修订，原写法为"唯一连接，所有窗口共用"：主进程不解析入方向的 JSON，就无法把事件分给多个窗口，见 Q1。）
- 入方向按字节块转发，由渲染进程切帧；出方向由主进程解析、按白名单放行并重新序列化。渲染进程收到即应用、应用后确认；跟不上时主进程停止读取，交给宿主重新同步。（设计评审修订）
- **渲染进程**开启 sandbox 和 contextIsolation。preload 只暴露有类型、有白名单的 API。
- 外壳相关的能力都通过一个 `ShellAPI` 接口调用，同时提供一个浏览器 mock 实现，为将来换成 Tauri 留余地。
- 如果 M1a 的 windowBridge 测量闸门（负载按一个窗口的订阅上限定义，见详细设计 5.7.1）测得主进程事件循环延迟的 p99 达到 5 ms，就在界面任务之前改用"utilityProcess 宿主桥 + 委托票据"。（设计评审修订：测量从 SP-6 前移到 M1a）

**安全加固（必须做到）**
- **Fuses**（发布构建）：
  - 关闭：`runAsNode`、`nodeOptions`、`nodeCliInspect`；
  - 打开：`embeddedAsarIntegrityValidation`、`onlyLoadAppFromAsar`；
  - 用 CI 校验构建产物；
  - 启动参数里出现调试参数时直接退出。
- **按"渲染不可信内容"处理 agent 输出**：
  - 严格的 CSP，禁止内联脚本和 eval；
  - 不渲染原始 HTML，做内容清洗；
  - 链接在外部浏览器打开，并且只允许 http 和 https；
  - 禁止页面导航和新开窗口；
  - 主文档启用 Trusted Types，禁止 `default` 策略；审查面板的 diff 视图放在独立源的 iframe 里（设计评审修订，见文末）。
- **防止"渲染进程替自己批准"**：
  - 下列操作只能走主进程弹出的原生确认框，确认框的内容由主进程直接从宿主获取：切换到完全访问或跳过审批、选择 `allow_always`、批准 CLI 提权、M3 中的设备配对和撤销。
  - 批准和回答提问，需要主进程在 1 秒内通过 `before-input-event` 观察到真实的用户输入，同时检查 `navigator.userActivation`。
  - 审批卡片出现后，有约 500 ms 的输入保护；Enter 只在焦点位于卡片上时生效。
  - M1 的系统通知上不放"批准"按钮。
- **其他入口**：
  - 深链接按白名单路由，只做导航，参数只接受 ID；
  - 不提供 AppleScript 字典，不注册 Services 菜单；
  - 拖放只往输入框里插入内容，不会直接发送。
- **构建种类**：发布构建、e2e 构建（放开调试，供 Playwright 使用）、开发构建（名称为 cupper Dev）。三者的 bundle identifier 各不相同，发布版的宿主只接受发布版的 UI。

**打包与更新**：electron-builder 配合 electron-updater，通过 GitHub Releases 分发；用 Developer ID 签名，用 notarytool 公证；安装时机由我们控制，见 ADR-01。

**测试**：
- Vitest 做单元测试；
- Playwright 在 Chromium 里跑截图回归，配合事件夹具；
- Playwright `_electron` 在 e2e 构建上跑端到端测试，连接真实宿主和假 agent。

## 后果

- **好处**：复刻保真度最高；组件生态成熟；AI 能自己验证界面。
- **代价**：
  - 包体和内存比 Tauri 大；
  - 每 8 周左右有一个 Electron 大版本，需要跟进（官方只支持最新的 3 个）；
  - 加固工作量不小。
- **SP-6**：在 M1b 打磨界面之前做。指标包括：流式输出、1000 和 5000 条消息的滚动、冷启动、中文输入法、内存，以及主进程事件循环延迟。
- **改变条件**：
  - SP-6 测得 Electron 冷启动或内存不达标，改用 Tauri；
  - 用户更看重原生 Mac 的观感，或者对包体有硬性要求。

## 编排者结论（2026-10-16）

用户在技术选型讨论会之后，要求先看同类产品用什么框架，并请编排者直接给出结论（`docs/meetings/2026-10-16-tech-selection-feedback/` 中的 D7）。R04 调研了约 40 个产品和 20 个框架，结论如下。

**维持 Electron + React 19 + TypeScript。** 界面核心（`ui-core`）不依赖外壳，Tauri 3 的 CEF 版本作为长期观察的后路。

理由：
1. **和复刻对象同构。** Codex 的界面代码就是按 Electron 编程模型写的 React，用到的组件也一样：Pierre diffs、Shiki、xterm、mermaid、KaTeX。编排者已在本机只读核实过它的运行时。
2. **符合用户的工程原则"不写补丁代码"。** 用系统自带的 WebKit（Tauri 2、原生外壳加 WebView 的方案），要为输入法回车、后台节流、闪烁等问题专门写绕过代码。opencode、Happy 已经因为这些问题从 Tauri 换回了 Electron；Raycast 改用 WKWebView 后，内存反而升高了。
3. **主流选择。** 编码 agent 客户端里，Claude、Cursor、VS Code、Goose、opencode 都用 Electron；ACP 客户端中，按 star 数排名前 6 的也都是 Electron。
4. **AI 编码效率最高，而且 AI 能自己验证结果。** React 和 TS 的训练语料最多；测试和生产用的是同一个 Chromium，可以用 Playwright 和 CDP 自动截图、检查。

代价和应对：
- **安装包较大**：运行时约 250 MB，加上 Node 和适配器，整包有几百 MB。用户本来就希望"像 Codex"，接受这个代价。
- **内存**：公开报告的高内存问题，根源大多在应用代码里（React effect 死循环、长会话一次性全部挂载），不在外壳。应对措施：lint 规则、性能预算、长会话窗口化，并由 SP-6 做长时间运行测试。
- **Electron 大约每 8 周出一个大版本**：跟着官方支持的最新 3 个大版本走，每次升级都经过回归测试。
- **安全加固**：按本 ADR 前文列出的清单执行。另外两种做法明确禁止：在发布构建里打开 CDP 调试端口；从远端加载渲染层的页面。

不采用的方案：
- **Tauri 2**：要写 WKWebView 的补丁代码，而且已有产品从它迁走。
- **Tauri 3 + CEF**：还在 alpha 阶段，长期观察。
- **原生外壳加 WKWebView（Raycast 2 的模式）**：要维护两套界面技术，AI 写起来效率低。
- **GPUI 全 Rust 界面**：界面工作量估计是 Electron 方案的 1.5 到 2.5 倍；API 不稳定，AI 可参考的资料少；Zed 的终端、Markdown 等组件是 GPL，不能直接用。

补充：
- 终端组件以 xterm.js 为主，ghostty-web 作为备选。
- 打包工具定为 **electron-builder 26.x + electron-updater 6.8.x**（详细设计 5.10.4）：`autoInstallOnAppQuit=false` 时，下载好的更新一直留到调用 `quitAndInstall()` 才交给 Squirrel，满足 ADR-01 的"自己控制安装时机"（已核对源码）；Forge 默认的更新路径会在下次启动时自动应用，无法暂缓。

## R05 补充证据（2026-10-16）

R05 调研了 33 个热门 App，包括 Manus、豆包、Kimi、元宝、飞书、微信、Teams、Figma、Slack、Discord、Notion 等。结论支持 Electron + React，没有反例：

- 32 个已知框架的 App 中，16 个用 Electron，另有 9 个也用 Web 技术渲染主界面（自研 Chromium 运行时、CEF、原生外壳加 WebView2、Tauri），合计 78%。
- **大厂"超出 Electron"之后，换的是自己掌握的 Chromium 运行时，界面仍然是 Web**：
  - 飞书改用自研的 Lark Framework，bundle id 还是 `com.electron.lark`；
  - 豆包用自研的 Chromium 运行时；
  - Teams 改用 WebView2，在 Mac 上自带了一整份 Edge（853 MB）；
  - Codex 改用 OWL。

  32 个 App 里，只有元宝把主界面放在 WKWebView 里。
- **AI 新产品**：创业公司和国内厂商默认用 Electron，例如 Manus、Kimi、千问、智谱、DeepSeek Harness、Granola；三家国内大厂的 AI IDE 都是 VS Code fork 加 Electron。原生派只有美国平台大厂（Gemini、Perplexity、Copilot）。
- **留在 Electron 的老牌产品都在框架内优化**，没有一个离开。Slack、Discord、Figma、Notion、Obsidian 都跟进到了 Electron 42–44。

另外补充两项决策：
- **Electron 的最低版本**：要避开 macOS 26 上已知的卡顿问题。Qoder 为此从 Electron 34 升到了 37。具体版本在详细设计时核实，并始终跟随官方支持的版本。
- **分发渠道**：只做官网直接下载，不上 Mac App Store。cupper 需要操作本地文件和进程，与 App Store 的沙箱要求冲突；同类 agent 产品（Perplexity 新版、Gemini、Manus、Granola）也都不上商店。

## 设计评审修订（2026-10-16）

来源：设计评审会（`docs/meetings/2026-10-16-design-review/minutes.md`）、用户决定（`docs/meetings/2026-10-16-gate-design-decisions/minutes.md`）、详细设计 v0.7 的 5.13.2。本节是细化，不推翻上文；上文中被替换的写法已就地改正并注明。

- **Electron 起始版本 44.8.0**，始终在官方支持的最新三个大版本之内（5.10.6）。
- **构建**：Vite 构建主进程、单文件 CommonJS 的 preload（沙箱 preload 不支持 ESM）、主文档和审查文档两个渲染入口；运行时依赖全部打进 bundle。
- **数据连接的生命周期**：绑定到文档实例，导航、渲染进程崩溃、窗口关闭时关闭；重连先 control 后 window；window 连接的 `initialize` 由主进程代发后才交出端口（5.7.1）。
- **Trusted Types 的落法**（纪要 D6）：主文档只有具名策略 `cupper-worker`，放行 `style-src 'unsafe-inline'` 以容纳组件库注入的样式，但 agent 给出的字符串不能进入 `<style>`、`insertRule` 或 CSS 自定义属性（ESLint 约束）；审查面板的 diff 视图放进 `app://cupper-review` 这个独立源的 iframe，原样使用 @pierre/diffs，有注入金样例、跨源隔离测试，`git/revert` 等破坏性操作只能由主文档的控件触发。验证不通过（Q29）时改用 `DiffHunksRenderer` 加自己的渲染。
- **真实输入的细则**（纪要 D7）：键盘用 `before-input-event` 识别，只认 Enter、Space、Escape、⌘Enter，`isComposing`、自动重复和打字中的按键都不算；任何出方向命令都会用掉尚未消耗的输入；只记录主框架上的输入；窗口激活后 1 秒冷却；Escape 只能用于拒绝类选项；`requestPrivileged` 也要求真实输入，被取消后冷却逐次加长；同一个 `command_id` 只在未收到任何响应时免输入重发。宿主侧对 window 角色要求 `input` 证据，并保留 400 ms 最短回答间隔（5.7.8、5.2.5）。
- **三种构建**：发布、开发、e2e 各有自己的 bundle id、数据目录和 URL scheme；e2e 构建的宿主开 `dev-auth` 和 `test-harness`，ad-hoc 签名，关闭 hardened runtime 和 cookie 加密；依赖 hardened runtime 和 codesign 的检查只在发布流水线的签名后冒烟里跑（5.7.9、5.10.7）。
