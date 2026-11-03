# T222 Git 服务 IIIc：status、branches 与 cupper diff

- 派发日期：（派发时填）
- 执行者：子 agent（Rust 工程师，宿主 Git 服务与 CLI）
- 分支：`feat/T222-git-status`
- 依赖：T120（`git/diff` 的四种范围与 `DiffCache`）。T123（提交与推送）、T125（worktree 检查与归档）依赖本任务

## 背景

T120 开工后越过硬停止线，拆成三份（计划 v1.3.86，卷宗 `docs/tasks/T120-git-diff-status/report-r1.md`）。T222 是第三份：仓库状态、分支列表，以及命令行的 `cupper diff`。代码在 T120 的提交 38f85e0 里已经写过一版（`git/status.rs` 等），T120 合并时移出了，本任务从那里取回、补齐用例。

## 范围

**做**（计划 T222 一行，5.2.10 的 `git/status`，5.3.4，5.2.14）：
1. `git/status`：HEAD（`branch --show-current` 加 `for-each-ref` 或 `rev-parse --verify -q HEAD`，游离 HEAD 和还没有提交的仓库都要对；新增的 `RepoOp::BranchCurrent` 由 T120 合并时补进设计的仓库档清单）、进行中的操作（标记文件）、文件状态（临时 index 上 refresh 后用 `diff-index --cached -M`、`diff-files`、`ls-files -o`，不在快照档里直接跑 `git status`）。
2. `git/branches`。
3. `cupper diff`（CLI，5.2.14）。
4. 用例 **IT-git-status**（已登记，挂在 T222 下）：HEAD 的几种状态（分支、游离、无提交）、进行中的操作、文件状态、branches、`cupper diff` 的输出。
5. 协议照 ADR-04；契约已冻结，只能附加，`schema-compat` 0 breaking。

**不做**：写操作（T121）；提交与推送（T123）；界面。

## 验收标准

- [ ] IT-git-status 通过。
- [ ] `schema-compat` 0 breaking。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：450；硬停止线：550

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.2.10（`git/status`、仓库档清单）、5.3.4、5.2.14（CLI）、ADR-10、T098 的规则 5（仓库档把非零退出判成 `unsafe_repo_config`）。
- 计划：T222 一行，第 3 节 T120 的注。
- 卷宗：T120（report-r1.md、report.md、review.md）、T046（cupper-cli II）、T098。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）；不在用户机器上跑性能或高负载测试。
- git 子进程一律经 `cupper-sys::spawn`；时间经 `Clock`。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
