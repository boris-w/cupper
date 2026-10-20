# T109 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）
- 日期：2026-10-20

## 读到的现状（开工前）

- `index::projects::prepare(path, Context)` 已经做了"realpath → 经仓库档取 git 根目录 → 阻塞线程池里 `trust::scan`"，会话这边可以直接复用（改成 `pub(crate)`），不另写一套。
- 项目的信任状态在 `projects` 表里，`IndexActor` 内存里也有一份（`Tables.projects`）；会话可以经 `Readers` 只读地取，改写交给 `IndexActor`。`IndexActor` 现有的写入都绑在命令上（`save(&mut Call, ..)`），没有命令的写入要加一条小路径。
- `SessionDeps` 里还没有 `GitTool`；`Discovery::git` 在 `supervisor` 里。
- 会话测试夹具的项目路径是不存在的 `/work/app`，扫描会失败；接上重扫后要改成临时目录里的真实目录（IT-orch-lifecycle 里断言 `/work/app` 的两处跟着改）。
- `TurnMachine` 在空闲发送时立即写 `turn.started`。

## 方案草图

1. **闸门改成原因的集合**：`Input::Gate { reason, shut }`；任一原因在就关着；去掉一个原因而另一个还在时，队列的暂停原因换成剩下的那个。T044 的 `TierGuard` 接线跟着改。
2. **发 prompt 前的检查**：`TurnMachine` 在要开回合（空闲时发送、或空闲时从队列取）之前发 `Action::Check`，等 `Input::Checked(ok)`；通过才写 `turn.started`。自发回合不检查。
3. **三个时机**：发 prompt 前（上一条）；Attach 前（actor 的 attach 任务先重扫，再按结果决定 `Load`/`Ignore`）；回合结束（`turn.completed` 之后在后台重扫，结果回到 actor）。
4. **判定**：会话当前是 `Load` 而重扫结果不是 `trusted`（变成 `stale`，或扫描失败）→ 关闸门（`project_config_changed`）、写 notice、`IndexNotes::rescanned(project, scan)` 交 `IndexActor` 改表；会话已经是 `Ignore` 时不受影响。
5. **"不加载项目配置继续"**：`SessionManager` 上一个宿主内部的方法，发给 actor：`session/close`、链接转为待 resume、策略改为 `Ignore`、`session.configured.project_config: ignored`、去掉 `project_config_changed` 这个原因。

## 开工前的规模估计

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `TurnMachine` | 闸门原因集合；发 prompt 前的检查握手；`Detach`（"继续"用） | 90 | — |
| `SessionCore` 与 `TierGuard` 接线 | `Check`/`Checked` 透传；闸门接口改动 | 15 | — |
| `SessionActor` | 重扫任务（读项目信任、可写根目录、复用 `prepare`）；attach 前重扫与策略；回合结束重扫；判定后的闸门、notice、`project_config`、索引通知；"继续"的内部入口（邮箱改成枚举） | 215 | — |
| `SessionManager`、`SessionDeps`、`supervisor` | `git: Option<GitTool>`、会话的项目 ID 与当前策略 | 20 | — |
| `IndexNotes::rescanned` 与 `IndexActor` | 无命令的项目行写入与 `project.upserted` | 30 | 25 |
| 夹具 | 真实的项目目录；`GitTool` 的测试环境口子 | 10 | 20 |
| IT-orch-trust-gating | 三个时机；发 prompt 前发现 `stale` 时消息留在队列、不写 `turn.started`；未信任时 `Ignore` 与 `project_config: ignored`；运行中变 `stale`；两个原因同时在；"继续"；git 根目录以上的 `.claude/` 在范围内（真实 git） | — | 240 |
| **合计** | | **约 380，乘 1.3 约 495** | **约 285** |

合计约 **780 行**；按以往测试多出 20% 左右算约 840。离约 850 的停止线只有不到一成，属于贴线，按 brief 开工前停下汇报。

## 需要裁定的问题

1. **空闲时发送的回复**。裁定第 4 条要求"扫描期间消息留在队列"。两种做法：
   - (a) 照字面：空闲发送也先进队列，回复 `queued`，检查通过后从队列出队开回合。代价是：每条空闲发送都会多写两条 `queue.updated`；回合的 `origin` 从 `user` 变成 `queued`；IT-orch-lifecycle 的事件序列和几处断言要改；客户端会先看到"排队"再看到"开始"。
   - (b) 消息留在一个"待开始"的位置（不写进队列），命令的回复等检查结束：通过就写 `turn.started`、回复 `started`；发现 `stale` 就把消息放进（因闸门而暂停的）队列、回复 `queued`。协议语义不变；代价是 actor 要能把一条命令的回复延到下一步（约多 25 行）。

   建议 (b)。
2. **扫描失败**（工作区不见了、文件太多、悬空链接）按"不可信"处理，对 `Load` 的会话同样关闸门、写 notice，是否可以？
3. **Attach 前发现 `stale`**（例如适配器退出后的 resume）：建议用 `Ignore` 去 attach 并关闸门，不阻止 attach 本身（闸门已经扣住 prompt）。另一种是等用户决定后再 attach。
4. **重新信任之后谁打开闸门**：`project/trust` 在 `IndexActor` 里，会话不知道。建议闸门因 `project_config_changed` 关着时，`session/send` 和 `session/queue_resume` 各触发一次重扫，结果是 `trusted` 就去掉这个原因。不需要 `IndexActor` 反过来通知会话。

按这几条的建议做，估计约 780–840 行。如果要留出余量，可以把"不加载项目配置继续"的内部入口（约 60 行连用例）和 git 根目录的真实 git 用例（约 40 行）另拆一个小任务。

## 实际行数（逐模块）

`git diff --numstat`，不含本文件和生成物（`packages/protocol/src/gen/`、`protocol/schema/v1.json`，共 13 行）。

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计（实现 + 测试） |
|---|---|---|---|
| 闸门的原因集合、开回合前的检查、`Detach`（`feat(host): the gate takes several reasons ...`） | 158 / 45 | 1 | 136 |
| `NoticeParams.reason`（`feat(protocol)`） | 4 / 0 | — | — |
| 三个时机的重扫、判定、索引通知、内部入口、`SessionDeps.git`、夹具改用真实目录（`feat(host): rescan the project's trust ...`） | 383 / 41 | 36 | 330 + 20 |
| IT-orch-trust-gating 与索引一半（`test(host)`） | 267 / 1 | 258 | 265 |
| 只在项目变为 `stale` 时通知索引（`fix(host)`） | 17 / 0 | — | — |

**合计 829 行**（另有生成物 13 行）：实现约 535，测试约 295。在约 950 的上限之内。

## 实现时的取舍

1. **检查只在会话里打开**：`TurnMachine::require_checks()` 由 `SessionCore::require_trust_checks()` 打开，会话管理器给每个会话都打开；回合状态机自己的单元测试和两个性质模型不打开，保持原样（它们不涉及信任）。检查的行为由 IT-orch-trust-gating 覆盖。
2. **空闲发送挂起**（裁定 1 (b)）：消息放在 `TurnMachine` 的"待开始"位置，回复也挂起；actor 把这条 `session/send` 的命令留到检查结束那一步再回复（`Command.deferrable`）。挂起期间再来的消息照常进队列、立即回复 `queued`。
3. **判定的状态**：`load_wanted` 表示"只要项目可信就加载项目配置"：新建时随策略；之后任何一次重扫发现可信都会打开它（未信任时新建的会话，在项目被信任后的下一次 attach 改为加载）；"不加载项目配置继续"永久关掉它。`load_wanted` 而项目不可信（`stale`、未信任或重扫失败）时关闸门、写一次 notice；可信时开闸门。Attach 按"可信且 `load_wanted`"选 `Load`，否则 `Ignore`，策略变了就改写 `session.configured.project_config`。
4. **重扫失败按不可信处理**（裁定 2）：项目行不见了、工作区不见了（`workspace_missing`）、仓库档失败（错误的 reason，如 `unsafe_repo_config`）、扫描失败（`ScanError::code()`）都给出语义码，写在 notice 的 `params.reason` 里。为此 `NoticeParams` 加了 `reason` 字段（协议改动，生成物已重生成）。
5. **改表交给索引**：会话只读 `projects` 的 `trust_state`、`trust_digest` 和全部项目路径；扫描结果从"信任"变成 `stale` 时，经 `IndexNotes::rescanned(project, scan)` 交 `IndexActor`，它用 `Trust::rescanned` 改行并写 `project.upserted`（没有命令的写入）。判定规则抽成 `trust::state_after`，`Trust::rescanned` 也用它，没有两份。
6. **复用索引的扫描**：`index::projects::git_of`（从 `prepare` 里拆出）和 `ScanContext` 改为 `pub(crate)`，会话的重扫和 `project/add` 走同一段"git 根目录 + 阻塞线程池里扫描"的代码。可写根目录取全部未删除项目的路径（裁定 5）。
7. **触发点**：检查由 `TurnMachine` 的 `Action::Check` 触发；Attach 前由 actor 的 attach 任务先重扫；回合结束看本步事件里有没有 `turn.completed`；闸门因 `project_config_changed` 关着时，`session/send`、`session/queue_resume` 各触发一次（裁定 4）。
8. **"不加载项目配置继续"**：`SessionManager::ignore_project_config(&SessionId)`，邮箱改成枚举（`Mail::Call` 装箱，免得枚举过大）；会话空闲时关闭 agent 会话（`TurnMachine::Input::Detach`），下一个回合带 `Ignore` resume。
9. **夹具**：项目目录改为临时目录里真实的 `app/`（IT-orch-lifecycle 里断言 `/work/app` 的两处改为 `f.project()`）；`Index` 多记一份重扫的摘要；`GitTool::with_test_env` 只在测试构建里存在，用例以此让 git 不读本机的系统配置。

## 交接

- **M1b 的确认界面任务**：给"不加载项目配置继续"加协议方法时，直接调用 `SessionManager::ignore_project_config`；`notice{project_config_changed}.params.reason` 说明是重扫失败还是内容变化（没有 `reason` 即内容变化）。
- **T089**（从库恢复）：`load_wanted`、闸门的原因和"继续"的选择都只在内存里；恢复会话时可以从 `session.configured.project_config` 和最后一次重扫重新得出，"继续"的选择需要另记（例如一个会话行字段）。
