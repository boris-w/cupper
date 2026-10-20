# T163 审计产生者补全

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主核心）。T138 的执行者接着做
- 分支：`feat/T163-audit-producers`
- 依赖：T059、T138（合并中）。T099 依赖本任务

## 背景

T138 把审计落成了 5.8.6 的正式格式（计划 v1.3.32）。盘点时发现三类事件至今没有产生者：`host.*`、`git.unsafe_repo_config`、`privileged.executed`。另外，有序退出时的 `conn.closed{host_exiting}` 只进了 host.log、没进表：`Host::exit` 先停写线程、再关连接。

## 范围

**做**（计划 T163 一行；设计 5.8.6、5.2.13、5.2.10）：
1. **`host.*`**（T059 的交接）：
   - 启动：在 `Host::start`；
   - 退出及原因：在 `Host::run` 交出 `Exit` 的地方（空闲、孤立、待升级各有取值）；
   - 进入待升级：在 `supervisor/watch` 的自检路径；
   - 处于弱认证模式：启动时按握手的认证方式。
2. **有序退出的顺序**：先关连接，再排空审计写入，最后停写线程。这样 `conn.closed{host_exiting}` 和 `host.exiting` 都能进表。改动在 `Host::exit`，不要破坏 T059 的退出语义（退出码、`last_clean_exit_at`、host.log 的 `orderly exit`）。
3. **`git.unsafe_repo_config`**：在仓库档拒绝执行处发出，记命令类别和配置来源位于工作区内的键。`ScanContext` 等路径要接审计发送端；`project/add` 和会话重扫两条路径都经过 `index::projects::locate`。
4. **`privileged.executed`**：在网关受理 privileged scope 的调用处发出，记方法、操作摘要（按 `policy/describe` 的 action 与语义码，不写文案）、principal、连接。
5. **`project/add` 新路径扫描失败**（T138 的发现）：这时还没有项目 ID，看是否按路径记一笔 `project.trust.scan_failed`。路径要按 T028 的约定脱敏；不值得做就在汇报里说明理由。
6. 各自的正式格式映射，放在 T138 的 `formal()` 里。

7. **检查 `admit` 里的 `verify(ui_requirement)`**（T138 的提醒）：它在异步工作线程上同步调用 Security 框架，未正式签名的大程序或第一次调用会慢，可能卡住宿主的定时器。必要时移到 `spawn_blocking`，并说明不会因此让未通过校验的连接提前进入受理。

**不做**：审计的界面（M1b）；诊断包的导出（已有）。

## 验收标准

- [ ] UNIT-audit-producers：每类新产生者至少一条用例。
- [ ] 进程级的有序退出之后，`audit` 表里有 `conn.closed{host_exiting}` 和 `host.exiting`。可以并进已有的 IT（例如 IT-audit-retention、IT-idle-exit）。
- [ ] 不写秘密：令牌、密钥、登录 shell 的环境都不进表或 host.log。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T163 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T163-audit-producers/ci/` 通过，摘要随任务提交。

## 交接事项

- T138：`formal()`、`kept()`；审计写入 task；`ConfigUnconfirmed` 的新结构。
- T059：`Exit` 的取值；`Watch` 的自检路径；`Host::run`、`Host::exit`。
- T098、T097：仓库档的 `GitError::Unsafe` 各变体。
- T038、T134：privileged scope 的方法与 `policy/describe` 的 action。

## 注意

- 代码约束：每条事件流只有一个写者；时间只经 `Clock`；进程级用例照 T008 修复 1 的守卫做法，不留残留进程。

## 规模

S。**开工前先在 `notes.md` 里按模块写行数估计；推算超过约 400 行（包括贴线）就先停下汇报**；硬停止线 470。

## 工作方式

T138 合并之后，在原来的 T037 worktree 里从最新的 main 开新分支：`git checkout -b feat/T163-audit-producers main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
