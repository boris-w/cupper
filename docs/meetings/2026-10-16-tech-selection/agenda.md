# 技术选型讨论会：议程

- 日期：2026-10-16
- 类型：讨论会（阶段 ②）
- 主持：编排者
- 参与者（子 agent，各自独立发言）：
  - 架构师：宿主与协议
  - 桌面客户端工程师
  - 移动与安全工程师
- 关联：ADR-01 至 ADR-05；PRD v1.0（tag `gate/prd`）

## 目的

为下面五个决定形成提议：

- 会话宿主的进程形态（ADR-01）
- 会话宿主的实现语言（ADR-02）
- 桌面 UI 技术（ADR-03）
- 宿主与客户端之间的协议（ADR-04）
- 本地传输与本地客户端认证（ADR-05）

这些提议会在 `gate/design` 时交用户确认。

## 输入材料

- `docs/01-prd.md` v1.0
- `docs/meetings/2026-10-16-prd-arch-alignment/architect.md`：尤其是 1.3 不可逆决定、2.3 接口、2.6 候选方案、5.1 ADR 清单
- `docs/tasks/R02-acp-adapters/report.md`：SDK 成熟度、适配器的运行要求
- `docs/tasks/R01-codex-ui-teardown/report.md` 第十二节：Codex 自身的技术栈
- `docs/meetings/2026-10-16-prd-interview-2/codex-screenshot-notes.md`：视觉目标

## 评判标准

每位参会者用以下标准给候选方案打分，并说明理由：

1. Codex UI 的复刻保真度和视觉打磨的成本
2. M3 接入 iOS 时代码能共享多少（协议类型、加密、渲染）
3. ACP 和其他关键依赖的生态成熟度
4. 由 AI 编码 agent 来写这部分代码的效率和出错率
5. 性能和资源占用（NFR-6、NFR-11）
6. macOS 上的签名、公证、自动更新、分发
7. 开源贡献者是否容易上手
8. 长期维护成本

## 流程

1. 第一轮：三位参会者独立给出立场和理由，原文存为 `<角色>.md`。
2. 如有分歧，编排者把各方的关键论点转给对方回应（第二轮）。
3. 编排者整理成 `minutes.md`，并起草 ADR-01 至 ADR-05（状态为"提议"）。
