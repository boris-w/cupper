# R02 Review

## 第 1 轮（2026-10-16）

- 结论：通过
- 审阅范围：`git diff main...research/R02-acp-adapters`（report.md、sources.md）

### 验收标准核对

| 验收标准 | 结论 |
|---|---|
| ACP 能力清单 | 达标。第 1.3 节覆盖 stable 和 unstable 的全部方法与通知，以及 `session/update` 的全部变体，另附 v2 草案的差异 |
| 能力对照矩阵 | 达标。第 6 节共 43 项，每行都有依据 |
| Codex 风格 UI 的缺口 | 达标。第 7 节覆盖全部 8 项，另加"插话"一项，并给出"我们需要补什么" |
| 来源、日期、核实标注 | 达标。来源有版本、提交和获取方式，二手和推测内容单独标注 |
| 对架构的启示 | 达标，共 8 条 |

### 抽查

用 `gh repo view` 核对：
- `zed-industries/codex-acp` 已归档，与报告一致。
- `zed-industries/claude-code-acp` 重定向到 `agentclientprotocol/claude-agent-acp`，与报告一致。
- `agentclientprotocol/codex-acp` 在 GitHub 上识别出的许可证是 "Other"，报告根据 LICENSE 文件写的是 Apache-2.0（版权方 JetBrains）。差异不影响调研结论；如果以后要复用它的代码，需要先核对 LICENSE 原文。

### 意见

| # | 位置 | 问题 | 处理 |
|---|---|---|---|
| 1 | report.md 全文 | 子 agent 先写到临时目录，再用 `cp` 复制进任务目录，绕过了"子 agent 不能创建报告文件"的限制 | 本次报告保留。任务是在流程调整之前派发的；流程已改为由编排者存档报告（CLAUDE.md 子 agent 守则第 6 条） |
| 2 | 第 2 节 SDK 表、第 4.1 节 | codex-acp 的许可证与 GitHub 识别结果不一致 | 不改，已在上面"抽查"中记录 |

### 带入 PRD 和后续阶段的事项

- **合规（PRD 必须写明）**：Anthropic 不允许第三方产品提供 claude.ai 登录（除非事先获批），产品名称不能用 "Claude Code"。这会影响产品定位（Q1）和账号方案（Q7）。OpenAI 对第三方客户端使用 ChatGPT 登录的条款还没有查到。
- **产品-架构对齐会的输入**：
  - ACP 只用来接 agent；桌面端建一个会话宿主，统一事件模型和同步协议。
  - worktree、汇总 diff、队列、会话索引、待审批队列都放在宿主层实现。
  - 这与 R03 的架构启示 A1–A5 结论一致。
- **阶段 ② 的 ADR 候选**：
  - Codex 走 codex-acp，还是直连 app-server。
  - 适配器怎样锁定版本、随应用分发。
- **阶段 ② 的原型候选**：派一个 spike 任务，实测矩阵中标为 🟡 的项目（回放完整性、排队、插话）。

### 合并

- 合并提交：`e139013`

## 勘误（2026-10-16，来自 SP-1 实测）

以下三处与 SP-1 的实测结果不符，以实测为准。R02 报告原文是历史记录，不做修改。

- 能力矩阵第 18 项"单次编辑的 diff"：Claude 给出的**不是整个文件的全文**，而是带上下文的片段，`_meta` 里另有 `structuredPatch`。Codex 是每个 hunk 一条；如果模型通过 shell 执行 `apply_patch`，就完全没有 diff。
- 第 34 项"运行状态"：Codex 在 v1 协议下就已经通过 `session_info_update._meta.codex.threadStatus` 提供运行状态（active、idle、systemError，并带有等待审批和等待输入的标记），而不是"只有 v2 才有"。Claude 没有对应的信号。
- 第 22 项"计划"：在 Claude Code 2.1.296 的 SDK 会话里，TodoWrite 和 Task 系列工具都不可用，所以**拿不到** `plan`。两个 agent 都能稳定拿到的，只有 plan 模式下的 `plan_update`（unstable）。
