# R02 ACP 协议与 Claude Code / Codex 适配器调研

- 派发日期：2026-10-16
- 执行者：子 agent（技术调研）
- 分支：`research/R02-acp-adapters`
- 依赖：无

## 背景

桌面客户端计划通过 ACP（Agent Client Protocol）接入 agent，初期只支持 Claude Code 和 Codex。写 PRD 之前需要弄清楚：ACP 和两个适配器现在能提供什么能力，复刻 Codex UI 时哪些功能有协议支撑，哪些需要自己补。见 `docs/meetings/2026-10-16-prd-interview-1/minutes.md`。

## 目标

产出 ACP 协议能力清单，以及 Claude Code、Codex 两个适配器的能力对照矩阵。

## 范围

- 做：
  - ACP 规范现状：版本与稳定性、传输方式、生命周期、全部方法和通知（会话创建/加载/恢复/列表、prompt、session update 的各种类型、权限请求、fs 和 terminal 等客户端能力、模式和模型切换、斜杠命令、MCP 配置、认证、扩展机制）。
  - 官方 SDK（TypeScript、Rust、Python 等）的成熟度和许可证。
  - Claude Code 适配器（例如 claude-code-acp 或其后继项目）：安装方式、维护方、覆盖了哪些能力、已知限制、认证方式（订阅 / API key）。
  - Codex 适配器（例如 codex-acp）：同上。
  - Codex 自身的 app-server 协议：Codex 官方客户端用的是它。与 ACP 对比，哪些能力 ACP 没有。
  - 已有的 ACP 客户端（Zed、JetBrains、其他开源客户端），尤其是带完整 GUI 的，作为参考。
- 不做：不写代码，不做最终技术选型。

## 验收标准

- [ ] ACP 能力清单：方法或通知 / 作用 / 稳定性（stable、unstable、draft）。
- [ ] 能力对照矩阵：能力 × （ACP 规范、Claude Code 适配器、Codex 适配器），标注 支持 / 部分支持 / 不支持，并写明依据。
- [ ] 重点说明与"Codex 风格 UI"相关的缺口：会话历史列表与恢复、diff 和文件变更展示、计划/待办、审批流程、worktree、模型和推理强度切换、图片输入、中断和排队。
- [ ] 每条事实附来源（规范文档、仓库代码、issue、发布说明），标注日期，区分"已核实"和"推测"。
- [ ] 「对架构的启示」一节：从"桌面端先行、移动端平滑接入"的角度，说明这些发现对整体架构意味着什么（作为 PRD 产品-架构对齐会的输入）。

## 参考资料

- agentclientprotocol.com 和对应的 GitHub 组织
- zed-industries 下的相关仓库
- openai/codex（app-server、exec 等）
- Claude Agent SDK 文档

## 汇报要求

完成后在本目录写 `report.md`（模板见 `docs/templates/task-report.md`）。原始资料摘录可以放在本目录的其他文件里。
