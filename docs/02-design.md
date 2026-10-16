# cupper 技术设计

- 版本：v0.4（ADR-01 至 ADR-05 已提议）
- 日期：2026-10-16
- 状态：进行中
- 依据：PRD v1.0（tag `gate/prd`）；产品-架构对齐会 `docs/meetings/2026-10-16-prd-arch-alignment/`

本文件在阶段 ② 中逐步填充：先通过 ADR 和原型实测确定关键选型，再写详细设计，最后经设计评审和用户确认后打 `gate/design`。

## 1. 架构概览

起点是对齐会上架构师给出的骨架（`architect.md` 议题 2）：

- 会话宿主是一个与界面分离的进程，持有会话、事件日志、审批与提问、Git 和 worktree。
- 桌面界面和命令行工具（cupper-cli）都是宿主的客户端；M3 起 iOS 通过远程桥接入。
- 宿主通过 ACP 驱动 agent 适配器。

最终形态以下面的 ADR 为准，确定后在这里补全组件图。

## 平台约束

- 桌面端只支持 macOS 14 及以上，即最近三年的版本（14、15、26）。用户于 2026-10-16 确认（U-T1）。
- **不写兼容代码**：不为兼容旧系统写任何兼容分支或补丁代码。需要新版本系统才有的 API 时，直接提高最低版本（2026-10-16 用户确认）。

## 2. ADR 索引

状态取值：待定 → 讨论中 → 提议 → 已采纳（用户在 `gate/design` 确认）。

| 编号 | 主题 | 状态 | 依赖 | 文件 |
|---|---|---|---|---|
| ADR-01 | 会话宿主的进程形态与生命周期 | 提议（技术选型讨论会，2026-10-16） | 技术选型讨论会 | `docs/decisions/ADR-01-host-process.md` |
| ADR-02 | 会话宿主的实现语言 | 提议（技术选型讨论会，2026-10-16） | 技术选型讨论会 | `docs/decisions/ADR-02-host-language.md` |
| ADR-03 | 桌面 UI 技术 | 提议（编排者结论：Electron + React；R05 补充证据待追加） | R04；用户讨论；必要时 SP-6 | `docs/decisions/ADR-03-desktop-ui.md` |
| ADR-04 | 宿主与客户端之间的协议 | 提议（技术选型讨论会，2026-10-16） | 技术选型讨论会 | `docs/decisions/ADR-04-host-protocol.md` |
| ADR-05 | 本地传输与本地客户端认证 | 提议（技术选型讨论会，2026-10-16） | 技术选型讨论会；SP-4 | `docs/decisions/ADR-05-local-transport-auth.md` |
| ADR-06 | 事件模型与存储 | 待定 | SP-1、SP-4 | — |
| ADR-07 | Agent 驱动路径 | 待定 | SP-1；必要时 SP-3 | — |
| ADR-08 | Agent 程序来源、适配器分发与版本锁定 | 待定 | SP-2 | — |
| ADR-09 | 适配器进程拓扑 | 待定 | SP-1 | — |
| ADR-10 | Git 与 worktree | 待定 | SP-5 | — |
| ADR-11 | 开源许可证与第三方依赖的再分发 | 待定 | SP-2 | — |
| ADR-12 | 配对与 E2EE 通道 | M3 前 | SP-8 | — |
| ADR-13 | 中继设计 | M3 前 | — | — |
| ADR-14 | 推送方案 | M3 前 | SP-7 | — |
| ADR-15 | iOS 技术与跨端代码共享 | M3 前 | SP-8 | — |

## 3. 原型实测跟踪

| 编号 | 内容 | 时间 | 状态 | 任务目录 |
|---|---|---|---|---|
| SP-1 | 适配器行为实测 | M1 前 | 进行中 | `docs/tasks/SP-1-adapter-behavior/` |
| SP-2 | 本机 CLI 与适配器的版本耦合、分发 | M1 前 | 进行中 | `docs/tasks/SP-2-adapter-distribution/` |
| SP-3 | 直连 Codex app-server 的对照 | 视情况 | 待定 | — |
| SP-4a | 宿主骨架走通（Rust） | M1 前 | 进行中 | `docs/tasks/SP-4a-host-skeleton/` |
| SP-4b | macOS 本地安全实测 | M1 前 | 已完成（ADR-05 方案成立，已据此修订 ADR-01、ADR-05） | `docs/tasks/SP-4b-local-security/` |
| SP-5 | Git 快照与审查操作 | M1 前 | 进行中 | `docs/tasks/SP-5-git-snapshot/` |
| SP-6 | UI 渲染压力测试 | 视情况 | 待定 | — |
| SP-7 | iOS 自建与推送链路 | M3 前 | — | — |
| SP-8 | E2EE 通道互通 | M3 前 | — | — |
| SP-9 | 睡眠与网络切换 | M3 前 | — | — |
| SP-10 | LaunchAgent 注册、TCC 授权归属、通知身份（会改动系统设置，需要用户在场） | M2 前 | — | — |

## 4. 工作顺序

1. **第一波（并行）**
   - 技术选型讨论会：讨论 ADR-01 至 ADR-05。参会者是三个不同角色的子 agent：架构师、桌面客户端工程师、移动与安全工程师。
   - SP-5。
   - SP-1、SP-2：得到用户许可后开始。
2. **第二波**
   - 根据实测结果起草 ADR-06 至 ADR-11。
   - 用选定的语言做 SP-4 宿主骨架。
3. **第三波**
   - 写详细设计（第 5 节）。
   - 开设计评审会。
   - 用户确认后打 `gate/design`。

## 5. 详细设计

待第二波完成后编写，计划包括：组件设计、宿主协议 schema 草案、领域与数据模型、Agent 驱动层、Git 与 worktree 服务、桌面 UI 结构、测试策略（假 agent 和录制回放）、构建与分发。
