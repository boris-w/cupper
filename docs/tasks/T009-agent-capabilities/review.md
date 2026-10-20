# T009 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：
  - 由 T055 拆出（计划 v1.3.18）。开工前估计约 700–750 行，贴线，执行者停下汇报；编排者裁定不拆、上限放宽到约 850（硬停止线 900），5 条取舍全部认可。最终 +883 −62 行。
  - 合并 main 时与 T034、T044 有三处冲突，编排者保留两边：
    - `supervisor/mod.rs`：本分支把发现链改成可复用的 `Finder`（`discover` 原来拿着数据库连接跨 `await`，future 不是 `Send`）；main 的 T034 加了 `GitTool` 和索引。合并后 `Finder` 拆成 `discover` 和 `launch_found`，启动时只发现一次（登录 shell 只跑一次），索引的 git（新函数 `git_tool`，取自同一个 locator）和 Claude 的拉起共用这次结果；`agents/refresh` 仍经 `launch` 重新发现。`Discovery` 不再带 `git` 字段。
    - `orchestrator/session/actor.rs`：T044 的 `agent()` 处理 `ConfigChanged`，其余事件先经本分支的 `commands_changed` 再交给核心。
    - `durations.rs`：`probe_commands` 与 `config_revert` 都保留。
  - 合并后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-feb3b363cfc3.json`。执行者那次的 `pr-dd1cfff255c1.json` 一并保留（它的第一次运行撞上本机磁盘满，删掉本 worktree 的增量缓存后重跑通过）。
- 核对：没有新依赖；用例一律用假 agent；没有个人信息。
- 审阅：
  - **命令列表按会话去重**（摘要比较），会话打开期间收到的列表只记下、不发事件；attach 不等命令列表。
  - **能力缓存**：attach、列表变化、探测完成时写，启动时删掉版本不符的行；探测目录每次删掉重建（0700）。
  - **`agents/refresh`** 重跑发现和版本检查、作废旧缓存、换掉下次拉起用的 `Launch`；Codex 在 M1a 回 `unavailable`。
  - **登录状态**只取 kind 和套餐名（K16）。
  - **超出 850 的约 4%** 来自把发现链抽成可复用的 `Finder`（为了 `Send`），理由成立，在硬停止线以内，接受。
- 转交事项已写进 `handoff-notes.md`：T034/T100（`AgentNotes::agent(AgentStatus)` 是索引接 `agents` 的入口，宿主现在传 `NoAgentNotes`）、T008（`rss_bytes`、`crashed`）。`agents/refresh` 不刷新索引里的 `GitTool`（启动时定下），以后需要时再补。
