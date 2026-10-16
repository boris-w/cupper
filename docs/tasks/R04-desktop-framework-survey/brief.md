# R04 桌面客户端框架调研：同类产品用什么、还有哪些选择

- 派发日期：2026-10-16
- 执行者：子 agent（技术调研）
- 分支：`research/R04-desktop-framework-survey`
- 依赖：无

## 背景

技术选型讨论会提议桌面 UI 采用 Electron + React（`docs/decisions/ADR-03-desktop-ui.md`，Tauri 2 为备选）。用户希望先拓宽选择范围，看看 Codex 等同类产品用的是什么框架，再一起讨论（见 `docs/meetings/2026-10-16-tech-selection-feedback/minutes.md`）。

已经确定或提议的约束：
- 只做 macOS，最低支持 macOS 14；
- 会话宿主是独立的 Rust 进程（ADR-01、ADR-02 提议），桌面 UI 只是它的一个客户端，不碰 ACP、Git 和 PTY；
- 要复刻 Codex 视图（PRD UI-6；截图笔记在 `docs/meetings/2026-10-16-prd-interview-2/codex-screenshot-notes.md`）；
- 代码主要由 AI 编码 agent 来写；
- M3 要做 iOS App。

上一轮讨论只比较了 Electron、Tauri 2、SwiftUI 原生、原生加 WebView 混合这四种。

## 目标

给出一张有依据的"同类产品框架地图"和一张"候选框架全景表"，供用户和编排者讨论 ADR-03。

## 范围

- 做：
  1. **同类产品用什么**。每个产品都要找到证据：官方说明、开源仓库、安装包结构（比如是否带 `Electron Framework.framework`）、可信的拆解文章；拿不到证据的标"推测"。产品至少包括以下这些：
     - 编码 agent 客户端：ChatGPT 桌面 App 的 Codex 视图、Claude 桌面 App（含 Claude Code 桌面功能）、Cursor、Windsurf、VS Code、Zed、JetBrains Air、Warp、GitHub Desktop 或 Copilot 相关客户端；
     - ACP 和多 agent 客户端：Conductor、CodexMonitor、Harnss、Codeg、AionUi、Goose 桌面版、opencode 桌面版、Happy 桌面版，以及 `R02` 第 8 节客户端清单里的其他带 GUI 的产品；
     - 体验出色、可作参照的开发者桌面工具：Linear、Raycast、Ghostty、Arc 或 Dia、Superhuman、Notion Mail 等，挑选有代表性的几个即可。

     每个产品要列出：框架和渲染方式、安装包大小、常驻内存的公开数字（如果有）、有没有官方说过为什么这么选、有没有迁移过框架（例如 ChatGPT 从原生改成 Electron）、用户的反馈。
  2. **候选框架全景**。除了已比较过的四种，还要覆盖：
     - Rust 原生 UI：GPUI（Zed）、Dioxus（桌面版以及 Blitz 渲染器）、Slint、iced、egui、Makepad、Freya；
     - 新一代 Web 外壳：Electrobun、Wails 3、Tauri 3（包括 CEF 后端的进展）、Neutralinojs；
     - 其他：Flutter、Compose Multiplatform、React Native macOS、Avalonia。

     每个框架评估：成熟度（版本、维护情况、有没有生产用户）、macOS 上的表现（原生观感、半透明材质、输入法、无障碍、多窗口），以及对我们需求的组件支持：流式 Markdown、代码高亮、diff 视图（hunk 操作和行内评论）、长列表虚拟化、终端、Mermaid。还要评估：包体和内存、签名、公证与自动更新、AI 编码 agent 写这种框架的效率、和 iOS 共享代码的可能性。
  3. **和"宿主用 Rust"的协同**：哪些方案能直接复用宿主的 Rust 类型和代码（例如 Tauri、GPUI、Dioxus），这一点值多少。
  4. **给出 3 到 4 个推荐组合**。每个组合写清楚适用的前提，例如"最看重和 Codex 一样"、"最看重轻量"、"最看重原生观感"、"最看重和宿主同语言"。
- 不做：不写产品代码，不做最终决定。

## 验收标准

- [ ] 同类产品框架表：产品 / 框架 / 渲染方式 / 包体与内存 / 证据（URL 或拆解方法）/ 已核实或推测。
- [ ] 候选框架全景表：按上面列出的维度逐项填写，每格都有依据。
- [ ] 一节"同类产品选型的规律与教训"：例如哪些产品从原生换成了 Electron、为什么，Zed 为什么自研 GPUI，等等。
- [ ] 3 到 4 个推荐组合，以及每个组合的前提和代价。
- [ ] 每条事实附来源和日期，区分已核实和推测。

## 注意

- 遵守 `CLAUDE.md` 子 agent 守则，尤其是第 7 条：任何外发请求都不能带用户的邮箱、姓名、账号等个人信息。
- 如果要查看本机已安装 App 的包结构（例如 `/Applications` 下的 App），只能读取 bundle 的目录结构和 Info.plist 这类元数据，不要运行这些 App，也不要读取任何用户数据目录。
- 不要下载和运行第三方安装包。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。资料摘录（例如 `sources.md`）由执行者在任务分支上提交。
