# T274 兼容套件支持 Codex

- 派发日期：2026-11-23
- 执行者：子 agent（CI 与工具；T113 的执行者）
- 分支：`test/T274-compat-codex`
- 依赖：T112（已合并）。由 T113 拆出；T113 依赖本任务

## 背景

T113 开工前估算（`docs/tasks/T113-codex-compat/notes.md`）发现兼容套件只支持 Claude：运行器、`cargo xtask compat`、剧本和 `gold_b.rs` 都写死了 `claude`。真实运行之前要先让套件支持 Codex。本任务只写代码，用假 agent 跑通，不用额度；真实运行留在 T113。

## 范围

**做**（估算的块 1、块 2）：

1. `cargo xtask compat` 加 `--agent`；`--out` 认相对路径；配置比对的对象只有 Claude 那边原有的；Codex 那边不取 `~/.codex` 的摘要（见"不做"）。
2. 运行器让 agent 贯穿建会话、等运行时装好、杀 CLI（按 agent 找进程名）、报告头和自我约束；用假 agent 真跑一次 T203 改过的拉起方式。
3. 约 16 个剧本各加 Codex 版：Q8、Q25、Q34、未信任项目拒绝启动、Codex 版 close-running；prompt 点名要用的工具（T102：真实配置下模型会绕开 apply_patch 改用用户的 MCP 工具）；思考剧本出能触发思考的题（T202）。
4. `gold_b.rs` 按录制头选 agent，加 Codex 的取消场景（为 GOLD-B-codex-s07 做准备）。
5. 单元测试。

**不做**：
- 真实运行和录制（T113）；
- sanitize 和 rec-shape-diff（T275）；
- 对 `~/.codex` 取摘要或读取任何内容（作者备份后自己用 `cmp` 比对）。

## 验收标准

- [ ] 用假 agent，`cargo xtask compat --agent codex` 的各剧本都能跑通（假 agent 能模拟的部分）；Claude 那边的行为不变。
- [ ] 单元测试覆盖 agent 参数的贯穿和 `gold_b.rs` 的选择。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 估算约 300（块 1 约 260、块 2 约 40）。停止线：360；硬停止线：420。每完成一块跑一次 `cargo xtask size`。

## 注意

- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令；要推送时告诉 `main`。
- 不运行真实的 Claude 或 Codex CLI。

## 工作方式

worktree 是 `.claude/worktrees/T052`，编排者已切到新分支 `test/T274-compat-codex`，从 main 开出（含本任务书和 T113 的估算）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
