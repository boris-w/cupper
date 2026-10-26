# T192 兼容套件 IV：剧本补全

- 派发日期：2026-10-26
- 执行者：子 agent（Rust 工程师）。T064、T187 的执行者接着做
- 分支：`feat/T192-compat-scenarios`
- 依赖：T187（已合并）。T066 依赖本任务

## 背景

兼容套件的剧本全集在设计 5.9.5。T064 做了"基本"组 10 个，T187 做了真实模式、自我约束和 Steer（记不适用），计划 v1.3.60 把其余 10 个剧本拆给本任务。剧本编号已在 T187 的 `notes.md` 里定下，T194 照它给 COMPAT 编号登记对应，不要再改。

## 范围

**做**（计划 T192 一行；设计 5.9.5）：
1. **其余 10 个剧本**，每个都有假 agent 脚本和真实提示词，并加上它需要的步骤和执行者能力：
   - `crash-resume`：CLI 或 app-server 被杀后恢复；
   - `host-restart`：宿主重启后 resume；
   - `image`：先上传 blob，再发带图的 prompt。协议里这条路径还没通的话，记"不适用"并写明缺什么（T064 第五点确认）；
   - `concurrent-two`（Q8）：两个会话在同一个适配器上并发 prompt。通过条件见 5.9.5：每个会话 3 个 prompt、5 分钟内全部返回、重复 3 轮。串话用执行者生成的标记检查（T187 第四点确认）；
   - `six-sessions`（COMPAT-claude-six-sessions）：6 个会话在同一个适配器上并发；
   - `close-running`（COMPAT-claude-close-running，K21）：关闭之后，命令及其子进程都已退出；
   - `resume-procs`（COMPAT-claude-resume-procs，Q43）：多次 resume 同一个会话后的进程数；
   - `untrusted-project`（COMPAT-untrusted-project-claude）：项目的 hooks 不运行、`.mcp.json` 里的服务器不启动、项目的 `env` 和 `apiKeyHelper` 不生效；
   - `hooks-reload`（Q28）：回合中改写 `.claude/settings.json` 的 hooks，看当前回合内是否生效。观察型，结果记 `observed`；
   - `config-scope`（Q34）：在上级目录和 git 根目录之外各放一份带 hook 的配置，看哪些生效。观察型，结果记 `observed`。
2. **`--fake` 能跑的都跑**：假 agent 模拟不了 CLI 读配置行为的剧本，在 `--fake` 下记 `real_only` 并写明原因。
3. 每个剧本仍受 T187 的自我约束和进程检查约束，不为新剧本放宽，除非剧本声明了 `loosens` 并写明理由。

**不做**：用真实 CLI 跑（T066）；`rec-shape-diff`、`trace-ids --compat`（T194）。

## 验收标准

- [ ] `cargo xtask compat --fake` 跑全部 21 个剧本，结果都是通过、不适用或 `real_only`，没有失败。
- [ ] `cargo xtask compat --real --dry-run` 列出全部 21 个剧本的计划。
- [ ] 三份 B 层录制与金样例不变；如果变了，在汇报里逐条说明。
- [ ] `cargo xtask ci compat-framework` 通过。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T192-compat-scenarios/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T192 的行）

- T187：剧本编号；`Scenario` 类型；`loosens`；进程检查（`lsof -F`）；真实宿主的启动方式。
- T063：多次 resume 后宿主侧不泄漏进程的做法（假 agent）。
- T003：Q8 的通过条件与 2 个会话的测法。
- T116、T029：录制导出、blob 分片。

## 注意

- **本任务里不运行真实的 Claude Code、Codex CLI**，只用 `--fake` 和 `--dry-run`。
- 不读、不改用户的 `~/.claude`、`~/.codex`。
- 入库的录制与报告不含个人信息。

## 规模

- 停止线：620；硬停止线：700

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每写完一个模块就跑一次 `cargo xtask size`，不要等全部写完；推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T043 worktree 里从最新的 main 开新分支：`git checkout -b feat/T192-compat-scenarios main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
