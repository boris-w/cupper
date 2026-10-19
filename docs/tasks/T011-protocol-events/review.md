# T011 Review

## 第 1 轮（2026-10-19，编排者）：开工前的拆分

- 执行者按守则第 8 条先做了模块估算，约 2,150 行，超出上限，没有写代码就停下来汇报。这是新守则第一次起作用。
- 编排者采纳了拆分：本任务收窄为"事件基础、领域类型与索引流"，另新增 T017。计划改为 v1.3.1，设计改为 v1.7（5.3.11、5.4.4），T011 的分支变基到最新的 main。

## 第 2 轮（2026-10-19，编排者）

- 结论：通过，已合并
- 核对：
  - 在任务的 worktree 里重跑 `cargo xtask ci pr`，通过：rust 93 项，UNIT-proto-events-index 为 pass；
  - 能编译到 `aarch64-apple-ios-sim`；
  - 个人信息为 0；
  - 规模 1,491 行，在上限以内，比估算多约 40%，原因是文档注释和开放枚举比预计的多。
- 接受的改动与取舍：
  - 删除 T010 的 `UnknownTagged`，以 `open_union!` 取代；
  - `Fallback.kind` 改为开放枚举，线上格式不变；
  - 降级以一个事件为单位；
  - 几个开放枚举的取值由执行者补全（见下文）。
- 编排者补进设计 5.3.11：`AgentState`、`CliInstall`、`ConfigDirState`、`AuthState`、`DiffSide` 的取值，以及 `FailureCode` 新增的三个值。
- 转交（记入 `docs/tasks/handoff-notes.md`）：交给 T012、T015、T017 的事项，以及 `EVENT_VERSION`。
