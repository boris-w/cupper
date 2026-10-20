# T044 会话 actor II：档位、信任门控、崩溃与晚到输出

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主安全）。T054、T060、T098 的执行者接手，因为信任门控要用 git 仓库档取 git 根目录
- 分支：`feat/T044-orch-gating`
- 依赖：T035、T043、T088、T095、T098（都已合并）。T046（CLI 命令）、T063、T068 依赖本任务；离剩余关键路径有 2 个单位的松弛

## 背景

会话 actor 的骨架（T043）和交互接线（T088）已经合并。本任务给会话加上四类"驱动事件或外部变化 → 暂停、中断或写提示"的处理（设计 5.2.6；计划第 4 节 T044 的注：四类共用暂停与恢复逻辑和 IT-orch-gating 的夹具）。

## 范围

**做**（计划 T044 一行；设计 5.2.6 的"配置项""项目信任""崩溃与恢复""晚到的工具输出"）：
1. **档位核对**：`session/create`、`set_config` 的档位用能力里的 `risk` 换算成 `Tier`（T095 的 `capabilities::tier()`，表外模式按 `full_access`），调用 `policy::tier_change`；`config_unconfirmed` 的判定用 `policy::ceiling`；`facts` 对 `session/set_config` 补上 `mode_tier`（交接，T031、T043、T095）。
2. **回合中越权**（安全核对 4）：回合进行中收到 `ConfigChanged`，发现实际档位越权（不是经有权限的 `session/set_config` 或选项设定的），先改回、再中断，写提示。
3. **`session/confirm_config`**：登记进 `Routes::sessions`，删掉 UNIT-unsupported-set 允许表里标为 T044 的那一行（交接，T033）。
4. **信任门控**：每次 Attach、每次发 prompt 之前、每个回合结束时调用 `trust::scan` 和 `Trust::rescanned`（扫描放进阻塞线程池）；git 根目录经 T098 的仓库档取得（`RepoOp::RevParse` 加 `--show-toplevel`）。未信任或 `stale` 时：Claude 在 Attach 的 `_meta.claudeCode.options` 里传 `settingSources: ["user"]`、`session.configured` 写 `project_config: ignored`；已信任运行中变为 `stale`：暂停队列（原因 `project_config_changed`），写 `notice{code: project_config_changed}`；"不加载项目配置继续"：先 `session/close` 再带 `settingSources: ["user"]` resume（交接，T035、T043、T098）。
5. **崩溃与恢复**：适配器退出后 resume；K6（CLI 崩溃后 `session/resume`）、K7（`restartable`）。
6. **晚到的工具输出**：`late_tool_output` 与 `may_still_run`（纪要 D4）。

**不做**：进程级集成测试（T062、T063、T068）；Codex 的 `project_untrusted`（M1b）。

## 验收标准

- [ ] 新增 IT-orch-gating（进程内，驱动用替身）：档位核对与 `config_unconfirmed`、`confirm_config`；回合中越权先改回再中断；三个时机的信任重扫、`stale` 暂停与提示、"不加载项目配置继续"；适配器退出后 resume、K6、K7；晚到输出写进 `late_tool_output`、`may_still_run`。
- [ ] 不睡眠：全部用 `ManualClock`。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T044 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T044-orch-gating/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T044 的行，开工前逐行读一遍）

T031、T033、T035、T043、T095、T098。

## 注意

- 用例只在临时目录里建项目和仓库，不对本机真实的仓库运行 git，不读 `~/.claude`、`~/.codex`。
- 代码约束：每条事件流只有一个写者（会话流的写者是 actor）；有状态的决策逻辑写进纯核心，核心里不做 I/O、不 await；时间只经 `Clock`。
- 会话管理器正在由 T055 组装进宿主，`SessionDeps` 可能新增字段；你的分支开出后如果它先合并，合并时由编排者处理冲突。

## 规模

计划估为 L，上限约 1,500 行。**开工前先在 `notes.md` 里按模块写行数估计（测试单独列），推算超线（包括贴线）就开工前停下汇报，并给出拆分建议**（例如"档位与越权、`confirm_config`"、"信任门控"、"崩溃与晚到输出"）。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

## 参考资料

- 设计：`02-host.md` 5.2.6（配置项、项目信任、崩溃与恢复、晚到的工具输出，含末尾"T043 开工前的裁定"）；`06-agent-driver.md` 5.6.2、5.6.9（K6、K7）；`03-protocol-v1.md`（`session.configured`、notice、`late_tool_output`）。
- `docs/tasks/` 下 T031、T035、T043、T088、T095、T098 的 `notes.md`。
- `crates/cupper-host/src/orchestrator/session/`、`trust/`、`git/`、`policy/`。

## 工作方式

在原来的 T054 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T044-orch-gating main`。T098 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
