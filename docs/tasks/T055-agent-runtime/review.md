# T055 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：
  - 开工前估计约 2,400 行，执行者停下汇报，拆成三份（计划 v1.3.18）：本任务收窄为运行时核心与宿主组装；生命周期归 T008，能力与状态归 T009。
  - 进行中编排者告知"发现链由 T034 负责、T055 复用"；执行者回复它已写好 `supervisor::discover`，编排者改为让 T034 复用它（在 `Discovery` 里加 `GitTool` 字段）。
  - 合并 main（没有冲突）后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-2b6779fa4329.json`。执行者那次的 `pr-65595768ea28.json` 一并保留。
- 核对：没有新增 crate；不运行真实的 CLI，进程级用例都注入假 agent，IT-lifecycle 的宿主一律传空的 `login_env`（不跑用户的登录 shell）；`unsafe` 没有出现在 `cupper-host`；没有个人信息。
- 审阅：
  - **宿主能跑通回合**：`it_lifecycle_runs_a_turn_with_the_fake_agent` 启动真实宿主进程、注入假 agent、经网关 `session/create`，回合 `completed`，SIGTERM 后以 0 退出、`adapters` 行已结束。这是 M1a 第一次端到端跑通宿主。
  - **进程卫生**（IT-process-hygiene）：适配器只有 fd 0、1、2，环境只有白名单里的变量（宿主环境里的 `SECRET` 被过滤），自成会话和进程组。
  - **拉起前核对**按内嵌清单的 `files` 核对 sha-256 和 cdhash，四种情况有用例；真正的运行时目录由 T056 接上。
  - **实例何时算结束**：等"进程已退出"和"连接已关闭"都到，同一条按实例的信道保证最后的更新不丢；`setsid` 的孙进程拿着 stdout 时实例不结束，由 T008 的后代清理解决。认可。
  - **宿主退出时的会话**：`TurnMachine` 加 `HostExit`，运行中的回合以 `interrupted` 结束、请求 `expired`、队列暂停；`SessionManager` 实现 `Part`。
  - 范围外的小改动（三处 `cargo doc -D warnings` 的链接、`orchestrator::testkit` 改为 `pub(crate)`）都有理由，认可。
- **规模与流程**：新增 1,590 行（实现约 1,360、测试约 230），删除 35 行，超过收窄后约 1,500 的上限、在 1,600 的硬停止线以内。超出主要在 `runtime/mod.rs`，执行者为守住硬线压缩过测试和组装代码，在汇报里如实说明。接受。
- 转交事项已写进 `handoff-notes.md`：T008、T009、T034、T044（`SetConfig` 被丢弃）、T056（运行时目录的两个路径）、T140（`Steer` 被丢弃）、设置任务（`env_passthrough` 未接）、第二种 agent 的任务（`ActorKind`）。
