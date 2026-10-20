# T109 会话 actor II-b：信任门控

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主安全）。T054、T060、T098、T044 的执行者接手，因为闸门、仓库档都是它写的
- 分支：`feat/T109-trust-gating`
- 依赖：T034、T044（都已合并）。T068（宿主集成测试 III）依赖本任务

## 背景

T044 开工前拆成两份（计划 v1.3.19，见 `docs/tasks/T044-orch-gating/brief.md` 末尾的补充裁定）：T044 做了档位、越权、发送闸门、`confirm_config`、崩溃与晚到输出；本任务做信任门控，复用 T044 的闸门（设计 5.2.6"项目信任"）。

## 范围

**做**（计划 T109 一行；5.2.6"项目信任"；T044 补充裁定第 2–5 条）：
1. **三个时机重扫信任**：每次 Attach（新建或 resume）之前、**每次发 prompt 之前**、每个回合结束时调用 `trust::scan` 和 `Trust::rescanned`。扫描放进阻塞线程池；git 根目录经 T098 的仓库档取得（`Discovery::git` 的 `GitTool`、`RepoOp::RevParse` 加 `--show-toplevel`）；仓库档的可写根目录取 `projects` 表里全部未删除项目的路径。
2. **发 prompt 前的重扫放在写 `turn.started` 之前**：扫描期间消息留在队列；发现 `stale` 就保持排队并暂停，不出现"回合已开始又要收尾"（裁定第 4 条）。
3. **未信任或 `stale` 时**：Claude 在 Attach 的 `_meta.claudeCode.options` 里传 `settingSources: ["user"]`（T055 的 AgentRuntime 已支持），`session.configured` 写 `project_config: ignored`。
4. **已信任、运行中变为 `stale`**：经 `IndexNotes` 交给 T034 的 `IndexActor` 改 `projects` 表（它是唯一写者，裁定第 3 条）；本会话关闸门（`turn::Input::Gate(Some(PauseReason::ProjectConfigChanged))`），写 `notice{code: project_config_changed}`。**闸门同一时刻只有一个原因**（T044 的交接）：`config_unconfirmed` 和 `project_config_changed` 可能同时出现，改成原因的集合，任一原因在就关着。
5. **"不加载项目配置继续"**：M1a 只做宿主内部的入口（先 `session/close` 再带 `settingSources: ["user"]` resume），协议方法由 M1b 做确认界面的任务加（裁定第 2 条）。

**不做**：Codex 的 `project_untrusted`（M1b）；进程级用例（T068）。

## 验收标准

- [ ] 新增 IT-orch-trust-gating（进程内）：三个时机的重扫；发 prompt 前发现 `stale` 时消息留在队列、不写 `turn.started`；未信任时 `settingSources` 与 `project_config: ignored`；运行中变 `stale` 时 `projects` 表经索引改为 `stale`、闸门关、notice；两个原因同时出现时任一解除都不放行；"不加载项目配置继续"的内部入口。
- [ ] 不睡眠：全部用 `ManualClock`。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T109 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T109-trust-gating/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

T035（三个时机的调用方式）、T044（闸门的用法、原因改成集合）、T034（`IndexNotes`、`Discovery::git`）、T098（仓库档的调用方式）。

## 注意

- 用例只在临时目录里建项目和仓库，不对本机真实的仓库运行 git，不读 `~/.claude`、`~/.codex`。
- 代码约束：每条事件流只有一个写者；有状态的决策逻辑写进纯核心；时间只经 `Clock`。

## 规模

计划估为 M，T044 的执行者估约 620 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 850 行（包括贴线）时，必须先停下来汇报**。

## 参考资料

- 设计：`02-host.md` 5.2.6"项目信任"；`03-protocol-v1.md`（`session.configured`、notice）。
- `docs/tasks/` 下 T035、T044、T034、T098 的 `notes.md`。

## 工作方式

在原来的 T054 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T109-trust-gating main`。T044 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 780 行（可能到约 840），贴近约 850 的上限，并提出四个影响协议可见行为的问题，停下汇报。

**裁定：不拆，上限放宽到约 950 行，硬停止线 1,000。** 可拆出去的部分只有约 100 行。执行者的方案草图（扫描复用 `index::projects::prepare`、`IndexNotes::rescanned`、`SessionDeps` 接上 `Discovery` 的 git、夹具改用临时目录里的真实项目、闸门改成原因的集合、`TurnMachine` 开回合前先发检查请求、自发回合不检查、回合结束后后台重扫）认可。

四个问题：
1. **空闲时发送**：选 (b)。消息先挂起、不进队列，命令的回复推迟到检查结束：通过就写 `turn.started`、回复 `started`；发现 `stale` 就放进因闸门而暂停的队列、回复 `queued`。协议语义不变，`origin` 仍是 `user`，IT-orch-lifecycle 的事件序列不受影响。
2. **扫描失败**（工作区不见了、文件太多、悬空链接等）：按不可信处理，失败时关闭（fail closed）。对 `Load` 的会话同样关闸门、写 notice（`project_config_changed`，`params` 里带扫描失败的语义码），Attach 用 `Ignore`。
3. **Attach 前发现 `stale`**（例如适配器退出后的 resume）：用 `Ignore` 去 attach，同时关闸门；闸门已经扣住 prompt，不阻止 attach 本身。
4. **重新信任之后开闸门**：闸门因 `project_config_changed` 关着时，`session/send` 和 `session/queue_resume` 各触发一次重扫，结果是 `trusted` 就去掉这个原因；不需要 `IndexActor` 反过来通知会话。
