# ADR-07 Agent 驱动路径

- 状态：提议
- 日期：2026-10-16
- 来源：SP-1 实测（`docs/tasks/SP-1-adapter-behavior/report.md`），R02

## 背景

宿主要通过一个驱动层同时接 Claude Code 和 Codex（ADR-02）。可选的路径有：

- **Claude**：用 ACP 适配器 `claude-agent-acp`，或者直接调用 Claude Agent SDK。
- **Codex**：用 ACP 适配器 `codex-acp`，或者直接连 Codex 的 app-server。直连能拿到队列、回合级 diff、多连接这些 ACP 没有的能力（R02 §5）。

## 决策

**两边首期都走 ACP 适配器。驱动层留出接口，以后可以换实现。**

1. **驱动接口用 cupper 自己的类型。** ACP 的类型不出驱动层（对齐会 1.3 第 9 条），以后加一个"直连 app-server"的实现，不影响上层。
2. **Claude 用 `claude-agent-acp`。** 审批、结构化提问、计划确认、取消、`session/resume` 实测都能用。它的短板由宿主补上：
   - 历史自己存；
   - CLI 崩溃后（适配器回复 "Session not found"）自动 resume；
   - 排队自己做。
3. **Codex 首期用 `codex-acp`。** 它的问题在宿主层规避：
   - **排队只能由宿主实现**：绝不并发发送 `session/prompt`，否则第一个 prompt 永远不返回；
   - **超时兜底**（CP-7）；
   - **取消之后**，丢弃晚到的工具输出（Codex 取消后命令不会停）；如果审批请求还挂着，宿主自己把它标为失效；
   - **cwd 一律传 realpath**。
4. **steering 是可选增强**：只在回合运行中发送，按返回的 `outcome` 处理 `startedNewTurn` 的情况。Claude 的 steering 有副作用（工具调用被丢弃、命令被移到后台），所以默认行为是排队（CP-6）。
5. **已知的体验问题：Codex #556。** 拒绝审批时，Codex 只有 `cancel` 这一个选项，整个回合随之结束（实测稳定复现）。界面上要写明"拒绝将结束本回合"（AP-2）。
6. **提前做 SP-3（直连 app-server）的条件**，满足任意一条即可：
   - "拒绝后让 agent 换个做法"被定为 P0；
   - codex-acp 在版本升级后出现无法规避的退化。

## 后果

- **好处**：两个 agent 走同一条 ACP 路径，实现成本最低；录制的帧可以直接用于回归测试。
- **代价**：
  - 依赖适配器的发版节奏，需要锁定版本（ADR-08）；
  - Codex 的 #556 在首期不能消除；
  - 驱动层要针对不同适配器写一层提取逻辑（退出码、diff、运行状态、错误）。
