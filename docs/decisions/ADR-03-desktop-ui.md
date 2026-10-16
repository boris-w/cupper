# ADR-03 桌面 UI 技术

- 状态：提议（编排者结论，2026-10-16；依据技术选型讨论会和 R04，R05 的补充证据待追加；最终在 gate/design 确认）
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
- **主进程**是到宿主的唯一连接，所有窗口共用。每个窗口分到一个自己的 MessagePort。
- 主进程只按长度前缀切帧，不解析 JSON；大约每 16 ms 批量转发一次；渲染进程跟不上时停止读取，交给宿主重新同步。
- **渲染进程**开启 sandbox 和 contextIsolation。preload 只暴露有类型、有白名单的 API。
- 外壳相关的能力都通过一个 `ShellAPI` 接口调用，同时提供一个浏览器 mock 实现，为将来换成 Tauri 留余地。
- 如果 SP-6 测得主进程事件循环延迟的 p99 超过 5 ms，就改用"utilityProcess 宿主桥 + 委托票据"。

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
  - 启用 Trusted Types。
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
- 打包工具在 electron-builder 和 Electron Forge 之间选，在详细设计阶段确定。Codex、Claude 用的是 Forge，前提是能满足 ADR-01 对"自己控制安装时机"的要求。
