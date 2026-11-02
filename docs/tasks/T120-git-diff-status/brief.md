# T120 Git 服务 III：diff、file_diff、status 与 branches

- 派发日期：（派发时填）
- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`feat/T120-git-diff-status`
- 依赖：T099（M1a 已退出）。T121、审查面板等依赖本任务（M1b 关键链）

## 背景

M1b 的关键链从 T120 开始：审查面板要的 diff、按文件的 diff、仓库状态和分支列表，都由宿主的 Git 服务提供。只读，写操作（暂存、还原）在 T121。设计 5.2.10 的"diff 与 hunk"和 `git/status` 两节，以及 5.3.4 的协议方法，已经定了范围、语义和错误。

## 范围

**做**（计划 T120 一行和第 3 节的注）：
1. `git/diff`：四种范围，`diff_token`、`caveats`、`index_unmerged`、`stat`、`filter_unsupported`，按 5.2.10。
2. `git/file_diff`：三种操作单位。
3. `DiffCache`：四种范围共用 `diff_token` 与缓存。
4. `git/status`、`git/branches`。
5. `cupper diff`（CLI）。
6. 用例：IT-git-diff-scopes（已登记，挂在 T120 下）。需要新编号先告诉 `main`。
7. 协议方法照 ADR-04 声明 `SCOPE`、`IDEMPOTENCY`，`pnpm gen` 生成 TS；契约已在 M1a 冻结（T016），**只能做附加式改动**，`schema-compat` 必须报 0 breaking。

**不做**：写操作（暂存、取消暂存、还原，T121）；worktree 创建（T124）；界面（审查面板）。

## 验收标准

- [ ] 四种范围和三种操作单位都有用例覆盖，IT-git-diff-scopes 通过。
- [ ] `schema-compat` 0 breaking。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：900；硬停止线：1100

<!-- 生成物不计。执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.2.10（Git 与 worktree 服务：diff 与 hunk、`git/status`）、5.3.4（协议方法）、ADR-10（只用 git 命令行，自动调用分快照、仓库、用户三档）、5.2.5（并发）。
- 计划：T120 一行，第 3 节的注，1.6（编号）。
- 卷宗：T068、T069（Git 服务 I、II 和故障点夹具）、T186（快照）、SP-5（hunk 用例的原型实测，T121 会改写）、T016（契约冻结）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）；不在用户机器上跑性能或高负载测试。
- git 子进程一律经 `cupper-sys::spawn`；时间经 `Clock`。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
