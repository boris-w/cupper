# T236 worktree 创建 II：崩溃恢复与故障点

- 派发日期：2026-11-04
- 执行者：子 agent（Rust 工程师，宿主 Git 服务与会话编排）
- 分支：`feat/T236-worktree-recovery`
- 依赖：T124（worktree 创建，已合并）。T125、T172 依赖本任务

## 背景

T124 开工前估算后拆分（计划 v1.3.87）。T124 已合并（PR #66），完成了以下几项：
- `session/create` 的 worktree 分支；
- `workspace.preparing` 那一批用 FULL 落盘，作为 `worktree add` 的在途标记；
- 准备失败时写 `workspace.failed`，第一条消息转成暂停的排队消息。

崩溃恢复和故障点拆到本任务。在本任务合并之前，宿主在 `worktree add` 途中崩溃的话，重启后会话停在 Preparing，不会恢复。

## 范围

**做**（计划 T236 一行；设计 5.2.13 第 6 条、5.2.10"worktree"、5.9.8）：
1. **载入时恢复**：会话的视图停在 Preparing、`initial_prompt_json` 不为空时，交给会话 actor 重新准备：
   - worktree 已登记（`worktree list` 里有这条路径）就复用，补上 lock 和 refresh 中没做完的；
   - 分支已在、worktree 没登记，就不带 `-b` 再 add；
   - 两者都没有，就照常 add。
   不重复创建，不留孤儿目录或孤儿分支；做不到的情形写 `workspace.failed`，第一条消息照 T124 转成暂停的排队消息。
2. **故障点**（IT-NFR9-failpoints-worktree，已登记）：在 T069 的夹具上登记两个确定的点：
   - `worktree.adding`：preparing 已落盘，add 还没执行；
   - `worktree.added`：add 已完成，lock 和 ready 还没做。
   每个点都按 5.9.8 断言：`integrity_check`、seq 连续、历史是前缀，以及同一 `command_id` 重试 `session/create` 得到原来的 `session_id`。另外断言重启后会话走到 `workspace.ready`，第一条消息照常开回合。写法照 T229 的 `crash_in_git`。
3. **IT-worktree-create 的崩溃复用一条**：在 T124 的用例旁补"复用已登记的 worktree"和"分支已在时不带 `-b`"两种情形，可以用进程内夹具直接构造状态，不必真的崩溃。
4. 写进设计的部分，在汇报的"设计偏差/补充"一节列出，由编排者在合并时改。

**不做**：`worktree/check`、归档、恢复、重命名（T125）；界面（T144）。

## 验收标准

- [ ] IT-NFR9-failpoints-worktree 通过，并在 runner 上连跑 10 次（由 `main` 触发 test-repeat）。
- [ ] 崩溃复用的两种情形有用例。
- [ ] 测试里没有任何 worktree 建在临时目录之外。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：250；硬停止线：300

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.2.6（新建与在途标记）、5.2.10（worktree）、5.2.13 第 6 条（崩溃恢复）、5.9.8（故障点）。
- 卷宗：T124（`Worktrees::plan`、`add`，汇报里的遗留问题）、T229（git 故障点的写法）、T069（故障点夹具）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 测试只用临时目录里的仓库。
- git 子进程一律经 `cupper-sys::spawn`；时间和等待都经 `Clock`；每条事件流只有一个写者。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
