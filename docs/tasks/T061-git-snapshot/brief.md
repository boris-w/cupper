# T061 Git 服务 II：快照档与回合快照

- 派发日期：2026-10-26
- 执行者：子 agent（Rust 工程师，宿主安全）。T054、T060、T098、T059、T129、T089、T169、T175、T177 的执行者接着做（可信 PATH、git 的绝对路径、`GitCmd` 与三档类型都是它做的）
- 分支：`feat/T061-git-snapshot`
- 依赖：T043、T060（已合并）。T099 依赖本任务

## 背景

Git 自动调用分快照、仓库、用户三档（ADR-10）。T060 建了 `GitCmd` 和三档类型，以及快照档的最小入口；T097、T098 做了仓库档的规则与指纹。现在回合的快照还是占位：会话 actor 的快照端口 `skipped()` 什么都不做。本任务把快照档做完整，接进回合。

原型实测在 `spikes/SP-5-git-snapshot/`，**只参考、不依赖**。

## 范围

**做**（计划 T061 一行；设计 5.2.10 的快照档、5.2.6、5.9.8）：
1. **快照档的隔离**：
   - 私有对象库、每工作区一个 `GIT_DIR`；
   - 白名单配置：`GIT_CONFIG_NOSYSTEM`、`GIT_CONFIG_GLOBAL` 指向受控文件，并决定是否带 `HOME`，理由写进 notes；
   - `SnapshotTier::prepare` 补齐 `GIT_INDEX_FILE`、`GIT_OBJECT_DIRECTORY`、`GIT_ALTERNATE_OBJECT_DIRECTORIES`（T060 的交接）。
2. **内容一致**：
   - 复制 info 文件与 sharedindex；
   - 保留 mtime，避免 racy git；
   - 自定义过滤器换成占位，按原样处理内容；设了 `required` 的按失败处理、不重试（D02 的实验结论）；
   - 生成补丁的命令带 `--no-ext-diff --no-textconv`。
3. **LFS**：
   - 用户有效配置里 `filter.lfs.*` 恰好是 git-lfs 标准取值时，cupper 写入同样的取值，程序一律用宿主启动时解析出的 git-lfs **绝对路径**；
   - 测试用的 git-lfs 由 xtask 按锁定版本下载、核对哈希。只下载公开物料，请求不带个人信息；
   - 本机没有 git-lfs 时，相关用例跳过，并给出清楚的原因。
4. **排除**：超大文件与嵌套仓库按设计排除。
5. **接入回合**：
   - 快照端口 `skipped()` 换成真正的快照任务；
   - 写 `snapshot.recorded`；
   - `turns.snapshot_status` 只有一列，最后记结束快照的状态（T043 的交接）；
   - 快照任务另起时要持有静止屏障的 `Hold`，git 子进程只经 `cupper-sys::spawn`，用 `GIT_SNAPSHOT` 超时。
6. **Q32 快照档逐键确认**：用"恶意配置仓库"样例，对可执行类键（`filter.*`、`diff.*.textconv`、`merge.*.driver`、`core.sshCommand`、`core.pager` 等）逐键确认快照档不执行它们。结果写进 notes 和汇报，由编排者更新 Q32。

**不做**：diff（T120）；仓库档的写操作（T121）；worktree（T124）。

## 验收标准

- [ ] IT-git-snapshot-parity、IT-git-racy、IT-git-filter-placeholder、IT-git-snapshot-malicious、IT-git-lfs-snapshot 通过；PERF-snapshot 在每晚跑，写下基准数据。
- [ ] Q32 快照档部分的逐键结果写进 notes。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T061 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T061-git-snapshot/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T061、git 执行者的行；开工前逐行读一遍）

- T060：`SnapshotOp`、`SnapshotTier::prepare` 要补的环境变量；`--no-ext-diff --no-textconv`；`GIT_SNAPSHOT` 超时。
- T043：快照端口 `skipped()`；`turns.snapshot_status`。
- T032、T043：`ticket.complete`、`completion.row()`、`mark_external()` 的用法；快照档不改用户仓库，多数情形用不到 `mark_external`，用不用要说明。
- T034：git 从 `Discovery::git` 取，找不到时为 `None`。
- T109：`rescan` 里同步的文件操作要放进 `spawn_blocking`。
- T163：`git.unsafe_repo_config` 的审计目前只在 `index::projects::locate`；快照档拒绝时是否也发，照设计决定。

## 注意

- 宿主运行期间不读取 App 包内的文件；子进程只经 `cupper-sys::spawn`。
- 不运行真实的 Claude Code、Codex CLI；测试仓库全在临时目录里，不读本机用户的 git 配置。
- 产品代码不依赖 `spikes/`。

## 规模

- 停止线：1300；硬停止线：1450

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项；每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报，并给出拆分建议。 -->

## 工作方式

在原来的 T054 worktree 里从最新的 main 开新分支：`git checkout -b feat/T061-git-snapshot main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
