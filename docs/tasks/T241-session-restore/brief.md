# T241 会话恢复

- 派发日期：2026-11-04
- 执行者：子 agent（Rust 工程师，宿主 Git 服务与会话编排）
- 分支：`feat/T241-session-restore`（执行者本机已有，存着 T240 拆分前一起写的 restore 代码，cc55be2c；派发后 merge main）
- 依赖：T240（会话归档，已合并）。T151、T153 依赖本任务

## 背景

T240 做到一半时估算会越过硬停止线，restore 拆到本任务（计划 v1.3.87）。restore 的代码在 T240 拆分前已经写好，存在执行者本机的 `feat/T241-session-restore` 分支上，没有推送。T240 的评审另外留下三处缺口，也一并在这里补。

## 范围

**做**（计划 T241 一行；设计 5.2.6"恢复"、5.2.10、PS-3）：
1. **`session/restore`**：
   - 分支还在：重建 worktree（复用 T236 的接管逻辑），回 `{mode: full}`；
   - 分支不在：把会话标为只读（PS-3），回 `read_only`；
   - `workspace.failed{not_found}` 的会话（已登记、目录不在）：锁是 cupper 自己的或者没有锁，并且分支没有在**别的** worktree 里检出（判断时排除这条记录本身，复用 `listed`）时，先 `unlock`，再 `worktree add --force <路径> <分支>`；否则转为只读。`--force` 也会越过"分支已在别处检出"这道检查，所以前面那项检查必须自己做。
2. **只读会话的写调用**（send、`queue_resume`、`set_config` 等）回 `unavailable`，带附加的 `ErrorReason::session_read_only`。`session/get` 照常回答。
3. **T240 评审留下的三处缺口**：
   1. `project/remove` 不能部分归档：先问遍所有会话，包括正在准备工作区的，全部可以归档再动手；有一个不行，就什么都不改，回 `conflict/turn_running`。
   2. 未确认的大小写冲突（参数是 `remove`，结果为 `blocked [modified_files]`）补一行用例，加进 IT-session-archive。
   3. 索引 actor 写 `projects.removed_at` 和 `project.removed` 那一段，在进程级宿主上补断言。
4. **用例 IT-session-restore**（已登记）：
   - 恢复为 full；
   - 恢复为 read_only；
   - not_found 的重建；
   - not_found 因外来锁、或分支在别处检出而转为只读；
   - 只读会话的写调用被拒。
5. 写操作照 T121 的路径：拿 `RepoLock`，先写 `pending_external`；事件经会话 actor 写；每条流只有一个写者。
6. 协议照 ADR-04；契约已冻结，只能附加，`schema-compat` 0 breaking。

**不做**：界面（T151、T153）。

## 验收标准

- [ ] IT-session-restore、补过的 IT-session-archive 都通过，并在 runner 上连跑 10 次（由 `main` 触发 test-repeat）。
- [ ] 测试里没有任何 worktree 建在临时目录之外。
- [ ] `schema-compat` 0 breaking。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：300；硬停止线：360（T240 拆分时估 restore 约 220，加上三处缺口）

## 参考资料

- 设计：5.2.6（恢复）、5.2.10（worktree）、5.3.4（`session/restore`）、PS-3、ADR-10。
- 卷宗：T240（汇报里的遗留问题）、T236（接管逻辑、`listed`）、T125（`cleanup::check`）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 测试只用临时目录里的仓库。
- git 子进程一律经 `cupper-sys::spawn`；时间经 `Clock`；每条事件流只有一个写者。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
