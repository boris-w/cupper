# T111 Codex 接入：发现、运行时与回收

- 派发日期：（派发时填）
- 执行者：子 agent（Rust 工程师，驱动层与运行时）
- 分支：`feat/T111-codex-runtime`
- 依赖：T099、T110（Codex 的提取层，已合并）

## 背景

M1b 接入第二个 agent：Codex。T110 已经做了 Codex 的提取层、能力和选项（`CodexSession`、`codex::agent_error`、`codex::capabilities`），交接事项里写了运行时要怎么接。T111 把它接进宿主的 AgentRuntime：找到 Codex CLI、准备 codex-acp、起适配器、回收。

## 范围

**做**（计划 T111 一行和第 3 节的注）：
1. codex-acp 从 App 包内复制到运行时目录（照 Claude 的做法，按内嵌清单核对哈希）。
2. `CODEX_PATH` 解析（Q19 的 Codex 布局、K15）；`CODEX_HOME` 校验。
3. 未信任的项目返回 `project_untrusted`。
4. `RunState` 接入 Codex 的自发回合。
5. 回收策略；`agents/restart`。
6. 运行时现在在 `Wire::Prompted`、`Wire::Opened`、`Wire::Configured` 里写死了 `claude::agent_error`，按 agent 换成对应的分类器。**T218 的规则**（`AdapterExited` 一类的 prompt 失败不转给会话，回合由实例结束时的 `AdapterExited` 结束）要对 Codex 同样生效，用例守住（交接事项 T218）。
7. 用例：UNIT-cli-resolver-codex、IT-K20-reclaim、IT-codex-untrusted（已登记）。需要新编号先告诉 `main`。

**不做**：真实 Codex CLI 的兼容运行（T113，维护者手动，消耗额度）；新建会话页的 agent 选择（T144）。

## 验收标准

- [ ] 三个用例通过；Codex 的会话能经假 agent（或 codex 形态的替身）走完创建、发送、回合结束、回收。
- [ ] T218 的规则对 Codex 生效，有用例。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：900；硬停止线：1100

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。共用代码的搬动单独成一个提交，按净增计（T110 的补充裁定二）。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.6.6–5.6.8（运行时、回收、清理）、5.2.6（崩溃表，T218 补的那句）、5.2.12、ADR-07、ADR-08、ADR-09。
- 计划：T111 一行，第 3 节的注，8.1 的 P-14（真实 Codex 改写 `~/.codex/config.toml`）。
- 卷宗：T110（交接给 T111 的接口清单）、T218（`AdapterExited` 的规则）、T102、T114（codex 0.166 的漂移、codex-acp 2.2.2）、T209。
- 交接事项（`docs/tasks/handoff-notes.md`）里所有写给 T111 的条目。

## 注意

- **不运行真实的 Codex CLI 或 Claude Code**；不读也不改 `~/.codex`、`~/.claude`。测试只用假 agent 或替身。
- 本机只单份跑（守则第 10 条）；不在用户机器上跑性能或高负载测试。
- 子进程一律经 `cupper-sys::spawn`；时间经 `Clock`；每条事件流只有一个写者。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
