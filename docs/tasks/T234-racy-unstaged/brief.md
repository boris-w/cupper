# T234 unstaged 视图漏掉同一秒内同大小的改动

- 派发日期：2026-11-04
- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`fix/T234-racy-unstaged`
- 依赖：T229（发现者；它的故障点用例里有一处绕法，指向本任务）

## 背景

T229 在进程级宿主上查出：用户仓库刚提交（或暂存）之后的同一秒内，把文件改成大小相同的新内容（例如 `a.txt` 从 `a\n` 改成 `b\n`），宿主的 `git/diff`（`scope: unstaged`）返回空列表，`git/stage` 随之报 `stale_diff`（"nothing to apply"）；用户自己的 `git diff --name-only` 能列出 `a.txt`。三组对照：

| 改动 | 结果 |
|---|---|
| 同大小、同一秒 | 漏掉，`files` 为空 |
| 不同大小 | 能看到 |
| 同大小，提交后先等 2 秒再改 | 能看到 |

这是 git 的 racy 检测一类的问题。已知的线索：
- 快照档自己的 racy 用例 `it_git_racy_a_file_changed_within_the_index_second_is_snapshotted` 能过，复制 index 时也保留了 mtime（5.2.10 第 395 行）；
- 进程内的会话夹具复现不了，宿主的 git 换成 Homebrew 2.54 照样漏，所以漏点可能和完整宿主里另外跑的东西有关，例如仓库档的 `update-index --refresh`（索引刷新），或者回合前后的快照、`DiffCache`、stamp 指纹按 stat 判"没变"而沿用了旧结果。

复现草稿和三组对照在 T229 的 `notes.md` 里。

## 范围

**做**：
1. **先查清漏点**：在进程级宿主上复现，逐步缩小到具体的调用或缓存。查清后用 SendMessage 报给 `main`：原因、修法、预计行数。等回复再改产品代码。
2. **修复**：按定下的修法改。涉及 5.2.10 的规则（例如复制 index、刷新、缓存失效的条件）时，在汇报的"设计偏差/补充"一节写明，设计由编排者在合并时改。
3. **用例 IT-git-racy-unstaged**（已登记）：进程级宿主，覆盖上面三组对照里漏掉的那一组，也覆盖暂存之后的同一秒（不只是提交之后）。
4. **去掉 T229 的绕法**：`tests/crash_concurrency.rs` 的 `crash_in_git` 改回同大小的改动，删掉指向 T234 的注释。

**不做**：与 racy 无关的 Git 服务改动。查到其他缺陷时，记下来报给 `main`。

## 验收标准

- [ ] IT-git-racy-unstaged 通过，修复前失败（汇报里写上修复前的失败输出）。
- [ ] IT-git-racy-unstaged 在 GitHub runner 上连跑 10 次全过（test-repeat，由 `main` 触发）。
- [ ] T229 的绕法已去掉，IT-NFR9-failpoints-git 照常通过。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：250；硬停止线：300（查清原因后的估算报给 `main` 时可以调整）

## 参考资料

- 设计：5.2.10（快照档、仓库档、"复制 index 时保留 mtime"、DiffCache）。
- 卷宗：T229（发现与复现草稿）、T061（快照档）、T120、T221（DiffCache）、T222（`git/status`）。
- git 文档：`Documentation/technical/racy-git.txt`。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 测试只在临时目录的仓库上操作。
- 时间经 `Clock`；git 子进程经 `cupper-sys::spawn`。用例里需要"同一秒"时，不要靠 sleep 去碰运气，用能确定复现的做法（例如设定文件的 mtime）。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
