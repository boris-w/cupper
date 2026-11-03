# T226 宿主写 `error` 条目与恢复指引（MS-5）

- 派发日期：（派发时填）
- 执行者：子 agent（Rust 工程师，宿主编排）
- 分支：`feat/T226-error-items`
- 依赖：T143（错误条目的界面）。T112（K17 文字错误转 `error` 条目）依赖本任务

## 背景

协议里早有统一的错误条目 `ErrorItem {code, agent_code?, retryable, guidance, raw?}`（`crates/cupper-protocol/src/item.rs`，开放枚举 `Guidance`），设计也要求宿主写它：
- 5.5.2 的失败分支：CLI 找不到、没登录、运行时安装失败时，`Attach` 返回 `unavailable`，会话流写一条 `error` 条目（带恢复指引），会话状态为 `error`；
- 06 的 K17：Codex 的文字形式错误转成 `error` 条目（T112）；
- 11：适配器 stderr 的尾部进入 `error` 条目的错误详情（MS-5）。

T143 执行时发现，宿主从来没有构造过 `Item::Error`：回合失败只在 `turn.completed.error_code` 里带 `FailureCode`。计划没有把这件事落到任何任务上，所以开 T226。E2E-error-guidance 从 T143 移到这里。

## 范围

**做**：
1. **`FailureCode` 到 `Guidance` 的映射**：纯函数，有单元用例。T143 提的初稿可以作起点，取舍写进汇报：
   - `not_logged_in` → `run_login_in_terminal`
   - `cli_too_old` → `upgrade_cli`
   - `cli_missing` → `install_cli`
   - `quota_exhausted` → `quota_exhausted`
   - `model_unsupported` → `change_model`
   - `adapter_crashed`、`runtime_failed` → `restart_agent`
   - `session_not_found`、`process_exited`、`restartable`、`adapter_exited` → `retry`
   - 其余 → `none`
   - `retryable` 一并给出。
2. **写条目的时机**，都经会话流唯一的写者：
   - attach 返回 `unavailable` 时（5.5.2 失败分支）；
   - 回合以 `failed` 结束时，在 `turn.completed` 之前；
   - 适配器意外退出时，带 stderr 尾部（已脱敏、截断）。
   - 同一个回合不重复写。
3. `raw` 取 `AgentError` 已经截断、脱敏的文本；不写进任何个人信息，确保 `tools/sanitize` 的规则覆盖到。
4. 用例：
   - **IT-error-item**（已登记，挂在 T226 下）：上面三种时机，加映射表；
   - **E2E-error-guidance**（已登记，由 T143 移来，nightly）：真实 App、宿主和假 agent 出错时，界面显示警告框和对应的按钮或指引。
5. 金样例或视图快照如果受影响，按现有流程重新生成，并在汇报里说明。

**不做**：K17 文字错误的识别（T112，它用本任务的写法）；界面（T143 已做）；设置页、模型选择器这些入口（T141、T151）。

## 验收标准

- [ ] IT-error-item、E2E-error-guidance 通过，附运行链接（E2E 在 nightly 路径上）。
- [ ] `schema-compat` 0 breaking（预计不改协议）。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：350；硬停止线：450

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.5.2（失败分支）、5.6.2（`AgentError`）、5.3 与 03 的第 7 条（`FailureCode`）、11 的 stderr 尾部、5.2.6 崩溃表、12-milestones 的 MS-5。
- 计划：T226 一行，1.6。
- 卷宗：T143（界面与 guidance 的入口）、T063、T218、T111（`AgentError` 分类、`project_untrusted`）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 每条事件流只有一个写者；时间经 `Clock`；有状态的决策写成纯核心。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
