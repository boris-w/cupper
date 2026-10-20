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
