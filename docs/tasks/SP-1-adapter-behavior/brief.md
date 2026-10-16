# SP-1 适配器行为实测

- 派发日期：2026-10-16
- 执行者：子 agent（协议工程师）
- 分支：`spike/SP-1-adapter-behavior`
- 依赖：无

## 背景

R02 的能力矩阵里有一批标为 🟡（部分支持）的项目，以及一些已知缺陷，都还没有实际跑过验证（见 `docs/tasks/R02-acp-adapters/report.md` 第 3.3、4.3、6、7 节）。对齐会和技术选型讨论会把这些列为 SP-1，作为以下 ADR 的依据：
- ADR-06：事件模型
- ADR-07：Agent 驱动路径
- ADR-09：适配器进程拓扑
- 以及 PRD 中的 AP-2、AP-5、CP-6、CP-7、MS-3、PS-5、PS-8

具体清单见 `docs/meetings/2026-10-16-prd-arch-alignment/architect.md` 第 5.2 节 SP-1。用户已许可使用本机已登录的 Claude Code 和 Codex 运行真实会话。

## 目标

用一个最小的 ACP 客户端，对两个适配器跑固定剧本，录下全部 JSON-RPC 帧，把能力矩阵里的 🟡 项目落实为"支持"或"不支持"，并写清楚具体行为。

## 范围

- **适配器**：
  - `@agentclientprotocol/claude-agent-acp`：用 `CLAUDE_CODE_EXECUTABLE` 指向本机的 `claude`；
  - `@agentclientprotocol/codex-acp`：先用它自带的 codex 跑，再用 `CODEX_PATH` 指向本机的 `codex` 各跑一遍。

  两个适配器都锁定当天的最新版本，在报告里写明版本号。
- **客户端**：可以用 TypeScript 写（`@agentclientprotocol/sdk`），也可以用 Rust 写（`agent-client-protocol`）。选哪个都行，但要说明理由。客户端声明它支持的能力，包括 elicitation、`plan_update`、`_meta.terminal_output`、boolean 配置项等，具体以 R02 为准。
- **剧本**：每一项都要在两个适配器上各跑一遍。
  1. `initialize` 返回的能力；新建会话之后，配置项（模型、模式、推理强度）和斜杠命令是什么时候下发的；预先建立会话、但不发 prompt，会不会在 agent 那边留下一个空会话（例如出现在 CLI 的历史里）。
  2. 一个普通回合：消息和思考的流式输出、工具调用、修改文件（diff 的形状）、计划、`usage_update`。
  3. 执行命令：输出和退出码能不能拿到，以什么形式拿到。
  4. 审批：分别选择"允许一次"和"拒绝一次"，看拒绝之后回合是继续还是结束（Codex #556）。
  5. 结构化提问：AskUserQuestion、request_user_input、计划确认。
  6. 回合进行中再发一次 `session/prompt`（排队），以及用 `_session/steering` 插话，记录成功和失败的表现（Claude #1267）。
  7. 取消：`session/cancel` 的时序，`stopReason` 什么时候返回。
  8. 会话列表：`session/list` 能不能看到 CLI 里原有的会话（只记数量和字段结构，不记录会话内容）。
  9. 恢复：用 `session/load` 回放（看完整性和有没有时间戳）；杀掉适配器再重启，用 `session/resume` 继续对话。
  10. 错误：模拟"未登录"（用临时的配置目录，例如给 Claude 设一个空的 `CLAUDE_CONFIG_DIR`，**绝不要登出用户的真实账号**），记录错误的形态。如果顺带遇到额度相关的错误，也一并记录。
  11. 图片输入：发一张很小的测试图片。
  12. 资源：每个适配器加 CLI 的进程树，空闲时和运行时的内存。
- 不做：不写产品代码；不做 Codex app-server 的直连对照（那是 SP-3）。

## 安全约束（必须遵守）

- 所有会话的工作目录都放在 worktree 之外新建的临时测试仓库里，**不要在任何用户已有的仓库或目录里运行 agent**。
- 只用默认的权限模式，或者更严格的模式（需要审批、只读、工作区）。**不要在"完全访问""跳过审批"或 bypass 模式下发送任何 prompt。** 枚举可用的模式没问题，但不要在这些模式下执行任务。
- prompt 必须无害、范围小，例如"在当前目录新建一个 hello.txt""运行 ls""把这个函数改个名字"。
- 不要选择"总是允许"，因为它会持久写入 agent 的权限规则；也不要修改 `~/.claude`、`~/.codex` 下的任何配置。
- 尽量节省额度：能选模型和推理强度时，选最便宜的组合。总回合数控制在需要的最小范围内，在报告里给出大致的调用次数。
- 测试会在 CLI 的历史里留下会话记录。不要删除用户的任何数据，在报告里列出新建了哪些会话（ID 和临时目录），由用户决定是否清理。
- 遵守 `CLAUDE.md` 子 agent 守则第 7 条：外发请求里不能出现任何个人信息。

## 物料与脱敏

- 脚本放在 `spikes/SP-1-adapter-behavior/`，带 README。
- 录制的帧放在 `spikes/SP-1-adapter-behavior/recordings/`，每个场景一个 JSONL 文件，以后可以直接给假 agent 和回归测试用。
- 提交之前必须脱敏，把以下内容替换成占位符：本机路径（统一写成 `$HOME`、`<tmp>`）、用户名、主机名、邮箱、账号或组织的标识、会话 token，以及任何看起来像密钥的字符串。脱敏脚本也一起提交。如果有拿不准的内容，宁可删掉那一段，并在报告里说明。

## 验收标准

- [ ] 一张结果矩阵：12 个场景 × 2 个适配器，写明实际行为，并引用对应的录制文件。
- [ ] 逐条说明 R02 矩阵里的 🟡 项目，以及 #556、#600、#1267、#1297 等已知问题的实测结论。
- [ ] 给 ADR-06、ADR-07、ADR-09 的建议。
- [ ] 录制文件已脱敏，并附脱敏脚本。
- [ ] 列出这次实测在本机留下的会话记录和临时目录。

## 汇报要求

按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。其他物料提交在任务分支上。
