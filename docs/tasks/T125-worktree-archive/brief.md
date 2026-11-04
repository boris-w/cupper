# T125 worktree 检查

> **v1.3.87 拆分（2026-11-04，开工前估算约 805 行）**：本任务只做下面第 1 项（`worktree/check`）和 IT-worktree-check-rules；第 2–5 项与 IT-session-archive-restore 拆到 T240（会话归档、恢复、重命名与删除项目，任务书另写）。停止线改为 350，硬停止线 420。

- 派发日期：2026-11-04
- 执行者：子 agent（Rust 工程师，宿主 Git 服务与会话编排）
- 分支：`feat/T125-worktree-archive`
- 依赖：T222（`git/status`、`in_progress`）、T124、T236（worktree 创建与恢复），都已合并。T151、T153（设置页、会话管理界面）依赖本任务

## 背景

新建 worktree（T124）和崩溃后的接管（T236）已经合并。T125 做会话生命周期的后半段，协议方法在 M1a 已冻结（5.3.4）：
- 归档前的清理检查 `worktree/check`；
- `session/archive`、`session/restore`、`session/rename`、`session/list_archived`；
- `project/remove`。

## 范围

**做**（计划 T125 一行；设计 5.2.6"归档""恢复"、5.2.10"worktree"、5.5.9、ADR-10）：
1. **`worktree/check`**：只读，按 ADR-10 的 7 条规则返回 `{removable, blockers: [WorktreeBlocker], warnings: {ignored_files, case_only_dirty}}`：
   - 第 1、2 条（修改、暂存、未跟踪）由快照档计算；
   - 第 3 条检查 git 目录里的进行中标记（复用 `git::status::in_progress`，T222 留的交接）；
   - 第 4 条检查锁文件（只有 cupper 自己的锁时不算外来锁；复用 T236 的 `listed`）；
   - 第 5、6 条用 `rev-list`；
   - 第 7 条检查子模块的 git 目录。
   - 被忽略文件的数量、只因大小写冲突显示为"脏"，只提示，不阻止。
2. **`session/archive`**：
   - 关闭 agent 会话；
   - 按参数保留或删除 worktree：`git worktree remove` 不加 `--force`，只有 `remove_case_only_dirty` 且确认时才加；默认保留分支；
   - 有运行中的回合时回 `conflict`；
   - 会话从索引流移除。
3. **`session/restore`**：
   - 分支还在就重建 worktree（复用 T236 的接管逻辑），回 `{mode: full}`；
   - 分支不在就把会话标为只读（PS-3），回 `read_only`；
   - `workspace.failed{not_found}` 的会话怎样恢复，在这里一并定（T236 的交接）。
4. **`session/rename`、`session/list_archived`**（结构化游标 `{archived_at, session_id}`）、**`project/remove`**（项目下的会话一并归档；有运行中的回合时回 `conflict`）。
5. 写操作照 T121 的路径：拿 `RepoLock`，先写 `pending_external`；事件经会话 actor 写；每条流只有一个写者。
6. **用例**（都已登记）：
   - IT-worktree-check-rules：7 条规则各一个情形，加上两种只提示的情形；
   - IT-session-archive-restore：归档时保留或删除 worktree、恢复为 full 或 read_only、重命名、列出已归档、删除项目。
7. 协议照 ADR-04；契约已冻结，只能附加，`schema-compat` 0 breaking。

**不做**：界面（T151 设置页"已归档会话"、T153 会话管理界面）。

## 开工前先估算（重要）

规模是 L。**开工前**先在 notes.md 里按模块估行数（7 条规则、archive、restore、rename 与 list、project/remove、用例），用 SendMessage 报给 `main`，等回复再写代码。超过停止线的话，提一个拆分方案（例如 `worktree/check` 单独一份）。

## 验收标准

- [ ] 两个用例通过，并在 runner 上连跑 10 次（由 `main` 触发 test-repeat）。
- [ ] 测试里没有任何 worktree 建在临时目录之外。
- [ ] `schema-compat` 0 breaking。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：700；硬停止线：850（开工前的估算报给 `main` 之后再定，可能拆分）

## 参考资料

- 设计：5.2.6（归档、恢复）、5.2.10（worktree、并发）、5.3.4（`session/*`、`worktree/check`、`project/remove`）、5.5.9、ADR-10。
- 卷宗：T124、T236（`Worktrees`、`listed`、接管逻辑）、T222（`in_progress`）、T121（写路径）。
- 交接事项：`docs/tasks/handoff-notes.md` 里写给 T125 的条目。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 测试只用临时目录里的仓库；删除只删测试自己建的 worktree。
- git 子进程一律经 `cupper-sys::spawn`；时间经 `Clock`；每条事件流只有一个写者。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
