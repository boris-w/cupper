# T240 会话归档、重命名与删除项目

> **v1.3.87 再拆分（2026-11-04，实现做到 +701 时估算会越过硬停止线）**：下面第 2 项（`session/restore`）、第 5 项里只读会话的拒绝和 `session_read_only`，拆到 T241（会话恢复）。本任务的用例改名为 IT-session-archive；停止线、硬停止线不变（600/700）。

- 派发日期：2026-11-04
- 执行者：子 agent（Rust 工程师，宿主 Git 服务与会话编排）
- 分支：`feat/T240-session-archive`
- 依赖：T125（`worktree/check`，已合并）。T151、T153（设置页、会话管理界面）依赖本任务

## 背景

T125 开工前估算后拆分（计划 v1.3.87），`worktree/check` 已合并（PR #73），会话生命周期的其余部分拆到本任务。协议方法在 M1a 已冻结（5.3.4）：`session/archive`、`session/restore`、`session/rename`、`session/list_archived`、`project/remove`。T125 开工前，编排者已经定了六个问题，记在 T125 的 notes 里，这里复述。

## 范围

**做**（计划 T240 一行；设计 5.2.6"归档""恢复"、5.2.10"worktree"、5.5.9、ADR-10、PS-3）：
1. **`session/archive`**：
   - 有运行中的回合（或正在准备工作区）时回 `conflict/turn_running`；
   - 关闭 agent 会话；
   - 按参数保留或删除 worktree：
     - 删除前调用 `cleanup::check`，有 blocker 时结果为 `blocked` 并带上 blockers；
     - 先 `worktree unlock`，再 `git worktree remove`，不加 `--force`；
     - 只有参数是 `remove_case_only_dirty`、`case_only_dirty` 为真且没有别的 blocker 时，才加 `--force`；
     - 默认保留分支；
   - worktree 已经不在：没登记的结果为 `removed`；已登记但目录不在的结果为 `kept`，记录留给用户；
   - 会话从索引流移除。
2. **`session/restore`**：
   - 分支还在就重建 worktree（复用 T236 的接管逻辑），回 `{mode: full}`；
   - 分支不在就把会话标为只读（PS-3），回 `read_only`；
   - `workspace.failed{not_found}` 的会话（已登记、目录不在）：锁是 cupper 自己的或者没有锁，并且分支没有在**别的** worktree 里检出（排除这条记录本身，复用 `listed`）时，先 `unlock`，再 `worktree add --force <路径> <分支>`；否则转为只读。`--force` 也会越过"分支已在别处检出"的检查，所以前面那项检查必须自己做，并有用例。
3. **`session/rename`**、**`session/list_archived`**（结构化游标 `{archived_at, session_id}`）。
4. **`project/remove`**：项目下的会话一并归档，worktree 一律保留；有运行中的回合或正在准备工作区时回 `conflict/turn_running`。
5. **只读会话和已归档会话的写调用**（send、`queue_resume`、`set_config` 等）回 `unavailable`，带上附加的 reason：`session_read_only`、`session_archived`。`session/get` 照常回答。
6. 写操作照 T121 的路径：拿 `RepoLock`，先写 `pending_external`；事件经会话 actor 写；每条流只有一个写者。
7. **用例 IT-session-archive-restore**（已登记）：
   - 归档时保留、删除、因 blocker 被挡；
   - `remove_case_only_dirty`；
   - worktree 已不在的两种情形；
   - 恢复为 full、read_only；
   - not_found 的重建与转只读；
   - 只读、已归档会话的写调用被拒；
   - 重命名、列出已归档、删除项目。
8. 协议照 ADR-04；契约已冻结，只能附加，`schema-compat` 0 breaking。

**不做**：界面（T151、T153）。

## 验收标准

- [ ] IT-session-archive-restore 通过，并在 runner 上连跑 10 次（由 `main` 触发 test-repeat）。
- [ ] 测试里没有任何 worktree 建在临时目录之外；删除只删测试自己建的 worktree。
- [ ] `schema-compat` 0 breaking。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：600；硬停止线：700

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.2.6（归档、恢复）、5.2.10（worktree、并发、`worktree/check`）、5.3.4（`session/*`、`project/remove`）、5.5.9、ADR-10、PS-3。
- 卷宗：T125（`cleanup::check`、notes 里的六个决定）、T124、T236（`Worktrees`、`listed`、接管逻辑）、T121（写路径）。
- 交接事项：`docs/tasks/handoff-notes.md` 里写给 T240、T125 的条目。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 测试只用临时目录里的仓库。
- git 子进程一律经 `cupper-sys::spawn`；时间经 `Clock`；每条事件流只有一个写者。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
