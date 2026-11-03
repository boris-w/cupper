# MAN-S1：并行开发

- 场景：PRD S1。在项目 A 里同时开两个会话：Claude Code 修一个 bug，Codex 补测试；两个会话各在自己的 worktree 里运行，互不干扰；侧边栏看得出哪个在运行、哪个在等我。
- 对应需求：CP-5、AG-3、PS-2、PS-3、PS-4、PS-8、GT-5。假 agent 的自动化版本是 E2E-S1（T170）。
- 共同约定见 [README.md](README.md) 第 4 节，记录格式见第 1 节。两种 agent 本来就在同一个场景里，所以只走一遍：A 用 Claude，B 用 Codex。

## 准备

- 项目已在 Cupper Local 里添加并确认信任（PS-2）。
- 设置页的 agent 状态里，两种 agent 都显示已安装、已登录，版本在已验证范围内（AG-2、AG-5，依赖 T151）。在这里记下 CLI 和适配器的版本。
- 选一个小 bug（几行就能修好），再选一个缺测试的函数。两件事碰的文件最好不同。

## 步骤

| # | 操作 | 预期与通过条件 | 依赖 |
|---|---|---|---|
| 1 | 新建会话 A：agent 选 Claude，工作区选"新建 worktree"，起始分支选主分支，分支名 `s1-fix`。发出修 bug 的 prompt | 会话页显示分支名 `s1-fix`。终端里 `git worktree list` 多了一个 worktree，位于 `~/.cupper-local/worktrees/` 之下。agent 读写的路径都在这个 worktree 里 | T124、T144 |
| 2 | A 还在运行时，新建会话 B：agent 选 Codex，新建 worktree，分支名 `s1-tests`。发出补测试的 prompt | 两个会话同时运行。侧边栏里两者同属项目 A，各带 agent 标识，都显示"运行中" | T124、T144 |
| 3 | 让其中一个停在审批上（不处理），另一个继续跑 | 停住的那个显示"需要处理"，另一个仍是"运行中"。按 ⌘⌥A 跳到需要处理的会话 | T153（⌘⌥A） |
| 4 | 处理掉审批，等两个回合都结束；先不点开 B | A 和 B 都显示"完成未读"。点开 B 后变为已读，A 不受影响 | — |
| 5 | 在终端分别看两个 worktree 和主工作区的 `git status`、`git diff` | A 的改动只在 `s1-fix` 的 worktree 里，B 的只在 `s1-tests` 的里；主工作区干净。两边都没有改到对方的文件 | — |
| 6 | 两个会话各再发一条跟进消息，然后 ⌘Q 退出 Cupper Local，再打开 | 两个会话都在，历史完整，分支名还在。再发一条消息，都能继续对话（PS-8） | — |
| 7 | 归档 A，按提示清理它的 worktree；B 先留一个未提交的改动，再归档 B | A 的 worktree 被删除，分支保留。B 提示有未提交的改动，不删除 worktree（GT-5） | T125 |
| 8 | 恢复刚归档的 A | 分支还在，worktree 被重建，历史可读，可以继续对话（PS-3） | T125 |

## 收尾

- 不需要的分支和 worktree 在终端里删掉（`git worktree remove`、`git branch -D`），不要推送。
- 记录的"逐项结果"按本表的编号写。
