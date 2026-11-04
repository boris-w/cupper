# T123 Git 服务 V：提交与推送（用户档）与 Q23

- 派发日期：2026-11-04
- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`feat/T123-git-commit-push`
- 依赖：T222（`git/status`、`git/branches`）。T130（审查面板的"Git 操作"菜单与提交弹窗）依赖本任务

## 背景

Git 服务的读的一侧（T120、T221、T222）和仓库档的写操作（T121）都已合并。T123 做用户档：用户在界面上点"提交""推送"时，宿主以与终端一致的方式执行 git，保留用户的 hooks、签名和凭据助手（5.2.10 的三档表）。协议方法 `git/commit`、`git/push` 在 M1a 已冻结。

## 范围

**做**（计划 T123 一行，5.2.10 的用户档与"提交与推送"，Q23）：
1. `git/commit`：`commit -F -`，从 stdin 读提交信息，运行用户的 hooks；"包含未暂存的更改"按协议的参数处理。
2. `git/push`：默认推当前分支，没有上游时 `--set-upstream`。
3. 错误映射：失败时把 stderr 末尾（脱敏后）放进错误的 `data`；hooks 拒绝、没有可提交的内容、推送被拒（non-fast-forward）、认证失败各有可区分的 reason。
4. 指纹变化的确认参数：本会话期间仓库配置变过时（`repo_config_changed`），确认文案要带上这一条，按 5.2.10 和协议的参数做。
5. Q23 的验证与恢复指引：SSH agent（继承 `SSH_AUTH_SOCK`）、GPG 签名（没有 TTY 时终端 pinentry 会失败，给出改用图形版 pinentry 的指引）、凭据助手。能在测试里验证的（本地裸仓库、假的 hooks、不需要真实凭据的部分）写进用例；需要作者自己的 SSH、GPG 配置的部分写成说明，由作者可选地手动验证（计划里这一项是"可选"）。
6. 写操作照 T121 的做法：`RepoLock`、`pending_external`、经会话 actor 写 notice。
7. 用例 **IT-git-commit-push**（已登记）：本地裸仓库；提交成功、hooks 拒绝、没有可提交的内容、推送与 `--set-upstream`、推送被拒；stderr 尾部脱敏。
8. 协议照 ADR-04；契约已冻结，只能附加，`schema-compat` 0 breaking。

**不做**：界面（T130）；生成提交信息、创建 PR（GT-6，P1）。

## 验收标准

- [ ] IT-git-commit-push 通过。
- [ ] Q23 的结论写进汇报：哪些自动验证了，哪些写成作者手动验证的说明。
- [ ] `schema-compat` 0 breaking。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：500；硬停止线：600

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项，估完报给 `main`。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.2.10（三档表的用户档、"提交与推送"、调用约定、指纹）、5.3.4（`git/commit`、`git/push`）、13 的 Q23、ADR-10。
- 卷宗：T121（写路径、`pending_external`、notice 通路）、T222、T098（仓库档规则）、T226（错误的脱敏）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 测试只用临时目录里的仓库和本地裸仓库，不推送到任何真实的远端，不读本机的 SSH 密钥、GPG 钥匙串或凭据。
- git 子进程一律经 `cupper-sys::spawn`；时间经 `Clock`；每条事件流只有一个写者。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
