# T239 按文件在外部应用中打开

- 派发日期：2026-11-04
- 执行者：子 agent（Rust 工程师，宿主；带一处界面改动）
- 分支：`feat/T239-open-file`
- 依赖：T232（发现者：文件树右键菜单的"在外部应用中打开"）、T126（`workspace/open`），都已合并

## 背景

T126 做的 `workspace/open {session_id, target: terminal|editor|finder}` 只能打开整个工作区（UI-0 原本就是"打开当前会话的工作区"）。T232 的文件树右键菜单有"在外部应用中打开"，现在只能打开整个工作区，Codex 打开的是那一个文件。

## 范围

**做**：
1. **协议**：`workspace/open` 附加可选的 `path`，内容是工作区内的相对路径（`WirePath`）；契约已冻结，只能附加，`schema-compat` 0 breaking。
2. **宿主**：
   - 带 `path` 时，核对它不越出工作区：拒绝绝对路径和 `..`；解析符号链接后（realpath）仍须在工作区之内；文件须存在。不合格回 `invalid_params`。
   - 执行 `/usr/bin/open -a <应用> <工作区>/<path>`，沿用 T126 的规则：应用只取内置清单、经 `cupper-sys::spawn`、不经 shell、环境为空、超时 `app_open`。
   - `target: terminal` 带 `path` 时，打开文件所在的目录；`finder` 时在 Finder 里选中这个文件（`open -R`）。具体怎样最接近 Codex，你核对后提出。
3. **界面**：审查面板文件树右键菜单的"在外部应用中打开"改为带 `path`，失败时照旧按语义码提示。
4. **用例 IT-workspace-open-path**（已登记）：
   - 合法路径；
   - 越出工作区的三种情形：绝对路径、`..`、指向工作区外的符号链接；
   - 不存在的文件；
   - 三种 target。
   用 test-harness 记录要执行的命令，**不真的打开任何应用**。

**不做**：顶栏"打开"按钮（T153）。

## 验收标准

- [ ] IT-workspace-open-path 通过，并在 runner 上连跑 10 次（由 `main` 触发 test-repeat）。
- [ ] 测试不在本机真的拉起任何应用，也不抢焦点。
- [ ] `schema-compat` 0 breaking。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：200；硬停止线：250

## 参考资料

- 设计：5.2.11（`workspace/open`、文件服务）、5.3.4、UI-0、ADR-04。
- 卷宗：T126（`workspace/open` 的规则与用例）、T232（文件树右键菜单）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 子进程一律经 `cupper-sys::spawn`。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
