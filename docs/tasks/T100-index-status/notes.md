# T100 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前的规模估计

估计前读过：
- 任务书；
- `handoff-notes.md` 第一列含 T100 的各行（T034、T009、T047）；
- 设计 5.2.9、5.2.3 的 actor 表、5.3.5 的索引流、`03-protocol-v1.md` 里 `session/mark_read`、`client/presence`、`notification.intent` 三行；
- 代码：`index/`（actor、`rows.rs` 的对账）、`orchestrator/session/`（`IndexNotes`、`commit`、`manager::create`、`rows.rs`）、`driver/runtime/status.rs`（`AgentNotes`）、`gateway/routes.rs`（`presence`）、`gateway/dispatch.rs`（客户端通知的投递）、`pure/`（`Core`、`TimerTable`、`Alarm`、`CoreDriver`）、`supervisor/`（组装、`seed`）。

行数指 rustfmt 展开后的行，含文档注释和空行。

### 实现

| 模块 | 内容 | 估计 |
|---|---|---|
| `index/status.rs` 纯核心 | 每个会话一项：摘要、是否在跑、待处理请求（数量、最早一个的类别和 ID）、最近结束回合的 seq、已读位置、未被成功回合清掉的失败；按 5.2.9 的优先级算状态、`unread`、`error_code`；`desktop_ui` 前台在场；第一处变化起 100 ms 合并成一次写出（`Core` 的定时器）；通知意图在变化时判定、在写出时按在场过滤；启动和 `test/seed` 后从表里补入会话 | 260 |
| `IndexNotes` 扩充与会话侧接线 | `IndexNotes::note(session, at, Vec<Note>)`，`Note`：创建、回合开始、回合结束（seq、状态、失败码）、待处理请求、分支、只读；会话 actor 在提交成功后从本批事件里摘出；`manager::create` 发创建；会话测试里的桩跟着改 | 100 |
| 索引 actor | 新邮件（会话通知、agent 状态）；循环里加 `Alarm`（仍在 `Participant::idle` 里等）；动作落地：一批写出摘要和意图（`intent_id` 经 `IdGen`），已读位置写 `sessions.last_read_seq`（只前移）；`session/mark_read`、`client/presence` 在 `call` 里分派；连接关闭时清在场；`IndexHandle` 实现 `AgentNotes`，`agents.updated` 按 agent 合并 | 170 |
| `rows.rs` | 读出 `last_turn_end_seq`、`last_read_seq`、`state`、最早待处理请求；状态不再在这里算；对账里的会话补写交给纯核心 | 30 |
| 宿主组装 | `routes.presence(..)`；`AgentNotes` 换成 `IndexHandle`（`NoAgentNotes` 只剩测试在用，挪进测试）；`IndexDeps` 加写线程句柄 | 25 |
| **实现小计** | | **585** |

### 测试

| 项 | 内容 | 估计 |
|---|---|---|
| UNIT-index-status | 纯核心经 `CoreDriver`：优先级、`pending_kind`、`unread` 与已读只前移（含超过最近结束回合的部分不算）、`error` 被成功回合或查看清掉、100 ms 合并、启动时从表补入 | 160 |
| UNIT-index-intent | 真实 `host.db` 上直接向 `IndexHandle` 输入：三种原因各写一条；前台 `desktop_ui` 在看时不写；CLI 的在场不算；进入 `needs_action` 后 100 ms 内又答完则不写 | 120 |
| IT-lifecycle 里调一次 | 真实宿主上 `client/presence` 和 `session/mark_read` 不再回 `unsupported`，`mark_read` 前移 `last_read_seq` | 40 |
| 既有用例的调整 | 索引对账用例、会话测试的桩、`UNIT-unsupported` 一类的列表（如有） | 20 |
| **测试小计** | | **340** |

### 合计

| 口径 | 合计 |
|---|---|
| 估计 | 约 925 |
| 按前几个任务的实测比（T047 实测约为估计的 0.86，T034 约 1.0） | 约 800–925 |

在约 1,100 的上限之内，不贴线，开工。每完成一个模块用 `git diff --numstat` 统计一次，记在下面。

## 统计（`git diff --numstat`，只算 `crates/`）

| 时点 | 增 | 删 | 净增 |
|---|---|---|---|
| 实现完成（纯核心、会话侧接线、索引 actor、`rows.rs`、组装） | 672 | 109 | 563 |
| 加上 UNIT-index-status、UNIT-index-intent、IT 与 `Client::notify` | 1,078 | 119 | 959 |
| 自查后定稿 | 1,084 | 123 | 961 |

定稿里实现约 563 行（估计 585），测试约 398 行（估计 340，多在 UNIT-index-status 的四个用例）。

## 过程记录

- `IndexNotes` 原来只有 `pending`。改成 `created(summary)` 和 `note(session, at, Vec<Note>)` 两个方法：创建单独一个方法，`Note` 里就不必有一个"已有会话收到也不该出现"的变体。
- `Note` 由会话 actor 在提交成功后从本批事件里摘出（`turn.started`、`turn.completed`、`workspace.ready`、`session.configured`），请求有变时再加一条 `Pending`（数量、最早一个的类别和 ID）。条目的流式输出不产生通知。
- "出错"的判定和 T043 的 `session/get` 一致：最近结束的回合失败且未读；之后有回合以任何状态结束都不再算出错。
- 状态核心按 5.2.16 的 `Core` 写，一个定时器 `Flush`；`CoreDriver` 驱动 UNIT-index-status。启动和 `test/seed` 后从 `sessions` 行补入核心（`Status::load`），和视图里已有的摘要不同才写；视图里的 `error_code` 沿用（行里没有失败码）。
- 已读写库不经事件：`StreamWriter::append` 遇到空批次直接返回，`with` 不会执行，所以 `IndexDeps` 加了写线程句柄。
- 有序退出时，索引写检查点之前先把攒着的变化写出，免得最后 100 ms 内的摘要和意图丢失。
- 索引 actor 的等待点仍是一个：`Participant::idle` 里同时等邮箱和 `Alarm`；没有新开脱离的 task。
- `NoAgentNotes` 换掉后只剩运行时的测试在用，挪成测试里的本地桩。
- IT 要发 `client/presence`，CLI 的客户端没有发通知的方法，加了 `Client::notify`（10 行）。

## 自行决定的几处（汇报里列出，请编排者确认）

1. **`sessions.last_read_seq` 由索引写。** 5.2.3 的表把 `sessions` 行归会话 actor，但已读是索引的输入（同一张表），而且已读要对没有加载的会话也生效，会话 actor 不一定在。会话 actor 从不写这一列（只在 `session/get` 里读），所以按列分写者：这一列只有 `IndexActor` 写，语句带 `last_read_seq < ?` 守住只前移。
2. **已读位置不超过索引知道的最近结束回合的 seq。** 未读和"出错未查看"都只看回合结束的 seq，所以截在那里不改变任何状态；否则一个 `up_to_seq` 很大的调用会让以后的回合都显示为已读。
3. **意图的原因**：回合以 `completed` 结束写 `turn_completed`，以 `failed` 结束写 `error`；`cancelled`（用户自己停的）和 `interrupted`（宿主重启时补写的）不写。`needs_action` 只在进入时写（待处理从 0 变为大于 0），`request_id` 取最早的待处理请求。
4. **合并与意图的时机**：第一处变化起 100 ms 后一次写出全部有变化的摘要，意图在同一批里；写出时如果有前台的 `desktop_ui` 在看这个会话，丢掉它的意图；`needs_action` 的意图在写出时请求已经全部答完的也丢掉。已读写库不等合并（`session/mark_read` 在落库后才回复），摘要的变化仍随下一次写出。
5. **"agent 不可用"按回合失败处理**：会话的 agent 起不来时回合以 `failed` 结束并带失败码（`cli_missing`、`not_logged_in` 等），所以不另看 `agents` 的状态；否则一个 agent 未登录会让它名下所有会话都显示出错。
6. **标题、归档没有接**：宿主里还没有改标题和归档的路径（`session/rename`、`session/archive` 在 T125；agent 起的标题也还没有来源），现在加 `Note` 的变体只会是没人用的代码。交接给 T125。
7. **最近活动**：取会话每次发来通知的时间（创建、回合开始和结束、请求出现和答完、分支、只读），不随条目的流式输出变化，免得回合进行时每 100 ms 写一次摘要。
8. **`agents.updated` 的 `git` 两项留空**：`GitTool` 不知道 git 和 git-lfs 的版本，交接给以后需要显示它们的任务。
