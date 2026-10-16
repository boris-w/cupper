# ADR-03 桌面 UI 技术

- 状态：讨论中（2026-10-16 用户要求先调研同类产品的框架选择，见 R04 和 `docs/meetings/2026-10-16-tech-selection-feedback/`；下文为技术选型讨论会的提议，暂不生效）
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
- Codex 本身就是 Electron + React，复刻保真度最高；
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
