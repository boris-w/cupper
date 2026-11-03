# T121 Git 服务 IV：暂存、取消暂存与还原（含 SP-5 的 82 个用例）

- 派发日期：（派发时填）
- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`feat/T121-git-stage`
- 依赖：T221（`DiffCache`、`git/file_diff` 与 hunk 切分）。T130（审查面板）依赖本任务（M1b 关键链）

## 背景

T120、T221、T222 做完了读的一侧：四种范围的 `git/diff`、`DiffCache`（存 tree 对和文件列表）、`git/file_diff` 的三种操作单位和按内容寻址的 `hunk_id`、`git/status`。T121 做写的一侧：按 `diff_token` 和 `hunk_id` 暂存、取消暂存、还原，这是第一批在仓库档上改动用户 index 和工作区的操作。设计 5.2.10 的"diff 与 hunk"、"并发"，5.2.5 的 `pending_external`，ADR-10 已经定了做法。

## 范围

**做**（计划 T121 一行，第 3 节的注）：
1. `git/stage`、`git/unstage`、`git/revert`：
   - 按 `diff_token` 从缓存取出 tree 对，重新生成补丁，按 `hunk_id` 取单位（T221 的交接：缓存没有 `hunk_id` 到路径的索引，要么加索引，要么逐个文件重新生成去找，取舍写进汇报）；
   - 在仓库档执行 `git apply --whitespace=nowarn [--cached] [-R]`；
   - token 不存在、过期，或者补丁应用失败，都返回 `conflict`（`reason: stale_diff`），不做模糊匹配；
   - 暂存前在用户仓库里对目标路径用 `check-attr filter` 再核对一次，白名单以外的过滤器返回 `git_error`（`filter_unsupported`）；
   - `git/revert` 来自 window 角色时必须带真实输入证据（5.2.5、5.7.8）。
2. `RepoLock`：每个仓库一把锁，会改用户 index 或工作区的操作都拿它；快照档和只读操作不拿。
3. `pending_external`：执行前先写记录，`synchronous=FULL` 加 `fullfsync` 落盘后才执行 git；中途崩溃后重试得到 `conflict(outcome_unknown)` 一类的结果，按 5.2.5 处理。
4. 把 SP-5 的 82 个 hunk 用例（`spikes/SP-5-git-snapshot/` 里的 `edge_hunk.py`、`hunks.py` 等）改写成 Rust 测试，即 IT-git-hunk。spike 代码只作参考，产品代码不依赖它。
5. Q32：仓库档写操作覆盖的可执行类键逐个确认有效；LFS 路径用锁定版本的 git-lfs 实测。
6. 在 T069 的故障点夹具上登记写操作前后的故障点，即 IT-NFR9-failpoints-git。
7. 用例：IT-git-hunk、IT-NFR9-failpoints-git、IT-git-stage，都已登记。

**不做**：提交与推送（T123）；worktree（T124、T125）；界面（T130、T131）。

## 开工前先估算（重要）

T120 开工时估 935 行，做完约 1,650 行，只好中途拆分。T121 的范围更大，**开工前**先在 notes.md 里按模块估行数，测试单独列，82 个用例单独列，然后马上用 SendMessage 报给 `main`，等回复再写代码。估算超过 1,100 的话，可以按下面的方式先拆：
- (a) stage、unstage、revert，`RepoLock`，`pending_external`，IT-git-stage；
- (b) SP-5 的 82 个用例（IT-git-hunk）；
- (c) Q32 与 LFS 实测、IT-NFR9-failpoints-git。

## 验收标准

- [ ] 三个用例通过。
- [ ] `schema-compat` 0 breaking（只能做附加式改动）。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：550；硬停止线：650（2026-11-03 开工前估算后拆分：82 个用例归 T228，Q32、LFS 与故障点归 T229；会话流 notice 的通路留在本任务）

## 参考资料

- 设计：5.2.10（三档调用、diff 与 hunk、并发、调用约定）、5.2.5（策略、命令账本、`pending_external`、真实输入证据）、5.2.8（事件存储）、5.9.8（故障点）、ADR-10、13 的 Q32。
- 计划：T121 一行，第 3 节的注，1.6。
- 卷宗：T120、T221、T222（report 和 review；交接事项里写给 T121 的条目）、T069（故障点夹具）、T098（仓库档规则）、SP-5、T060、T061。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- git 子进程一律经 `cupper-sys::spawn`；时间经 `Clock`；有状态的决策写成纯核心。
- 测试只在临时目录的仓库上操作，不碰本机任何真实的仓库。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
