# T124 worktree 创建（CP-5）

- 派发日期：2026-11-04
- 执行者：子 agent（Rust 工程师，宿主 Git 服务与会话编排）
- 分支：`feat/T124-worktree-create`
- 依赖：T099。T125（worktree 检查、归档、恢复）、T144（新建会话页）、T172（NFR-9 与容量）依赖本任务

## 背景

Git 服务的读写两侧都齐了：diff、暂存与还原、提交与推送（T120–T123、T221、T222、T228、T229）。T124 做"新建会话时新建 worktree"（CP-5）：
- `session/create` 带 worktree 参数时，宿主在仓库之外 `worktree add` 一个新分支；
- 加锁、刷新，再交给 agent；
- 宿主崩溃后，复用已经登记的 worktree。

协议的 `session/create` 参数和 `workspace.preparing`、`workspace.ready`、`workspace.failed` 事件都已冻结。

## 范围

**做**（计划 T124 一行，第 3 节"T124（L）"的注；设计 5.2.10 的"worktree"、5.5.2、5.2.13）：
1. **`worktree add`**（仓库档）：
   - 位置 `<worktrees.root>/<仓库名>-<短哈希>/<会话短名>`，根目录 0700，`worktrees.root` 可在设置中修改，默认 `~/.cupper/worktrees`；
   - 分支 `cupper/<短名>`，先 `check-ref-format --branch` 校验，冲突时加后缀；起点是 `base_ref`；
   - 检出不运行自定义过滤器和 post-checkout hook（仓库档的覆盖）；过滤器设了 `required` 时检出会失败，回语义码，界面提示的文案在 5.2.10，由界面做；
   - 超时 10 分钟，输出进度事件（`workspace.preparing` 的进度）；
   - 拿 `RepoLock`，写 `pending_external`（照 T121 的写路径）。
2. **建好之后**：`worktree lock --reason "cupper session <id>"`；等 1 秒（经 `Clock`）执行一次 `update-index --refresh`；不用 `--relative-paths`。
3. **`session/create` 的 worktree 分支**：照 5.5.2 的时序，`workspace.preparing` → `workspace.ready`（路径、分支）或 `workspace.failed`；之后的项目信任扫描、Attach 都用 worktree 的 realpath。
4. **崩溃恢复**：宿主在 `worktree add` 途中或之后崩溃，重启后能识别已登记的 worktree 并复用，不重复创建，也不留下孤儿；做不到的情形写 `workspace.failed`。
5. **故障点**：在 T069 的夹具上登记"`worktree add` 进行中"的故障点（IT-NFR9-failpoints-worktree，已登记），按 5.9.8 断言。T229 登记 git 写操作故障点的方式可以照搬。
6. **用例**：IT-worktree-create（已登记），覆盖：
   - 新建成功；
   - 分支名冲突时加后缀；
   - 非法的短名；
   - `required` 过滤器导致失败；
   - 锁与刷新；
   - 崩溃后复用。
   worktree 根目录用 test-harness 的配置指向临时目录。
7. 协议照 ADR-04；契约已冻结，只能附加，`schema-compat` 0 breaking。

**不做**：`worktree/check`、归档、恢复、重命名（T125）；新建会话页的界面（T144）；删除 worktree。

## 开工前先估算（重要）

规模是 L。**开工前**先在 notes.md 里按模块估行数：
- worktree add 与命名；
- 锁与刷新；
- `session/create` 接线；
- 崩溃恢复；
- 故障点；
- 用例。
估完用 SendMessage 报给 `main`，等回复再写代码。超过停止线的话，提一个拆分方案（例如把崩溃恢复和故障点拆出去）。

## 验收标准

- [ ] IT-worktree-create、IT-NFR9-failpoints-worktree 通过，并在 runner 上连跑 10 次（由 `main` 触发 test-repeat）。
- [ ] 测试里没有任何 worktree 建在临时目录之外，也不碰本机真实的 `~/.cupper`。
- [ ] `schema-compat` 0 breaking。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：700；硬停止线：850（开工前的估算报给 `main` 之后再定，可能拆分）

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项，估完报给 `main`。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.2.10（worktree、调用约定、并发）、5.2.5（`pending_external`）、5.2.13、5.5.2、5.9.8（故障点）、ADR-10。
- 卷宗：T121（写路径）、T123（`RepoLock` 与 `pending_external` 的用法）、T229（git 故障点）、T069（故障点夹具）、T098（仓库档覆盖）。
- 交接事项：`docs/tasks/handoff-notes.md` 里写给 T124 的条目。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 测试只用临时目录里的仓库；不读本机的 SSH 密钥、GPG 或凭据。
- git 子进程一律经 `cupper-sys::spawn`；时间和等待都经 `Clock`；每条事件流只有一个写者，Git 服务经会话 actor 的信箱写事件。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
