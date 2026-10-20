# T100 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T100-index-status`
- 完成日期：2026-10-20

## 做了什么

T100 已完成。索引现在会算会话状态、记录已读、采信在场，并写出通知意图；`agents.updated` 由各 agent 运行时喂入。在干净的 worktree 上 `cargo xtask ci pr` 一次通过。净增 961 行（增 1,084、删 123，只算 `crates/`），在约 1,100 的上限之内。其中实现约 563 行（估计 585），测试约 398 行（估计 340）。开工前的估计是约 925。

1. **状态计算纯核心**（`index/status.rs`，按 5.2.16 写成 `Core`，只有一个定时器 `Flush`）
   - 每个会话一项，记录以下内容：
     - 不由状态决定的摘要字段；
     - 是否有回合在跑；
     - 待处理请求的数量，以及最早一个的类别和 ID；
     - 最近结束回合的 seq；
     - 已读位置；
     - 最近结束的回合是否失败，以及失败码。
   - 状态按 5.2.9 的优先级计算，`pending_kind` 取最早的待处理请求。"出错"的判定和 T043 的 `session/get` 一致：最近结束的回合失败且未读。
   - **100 ms 合并**：从第一处变化起 100 ms 后一次写出全部变了的摘要，和流里已有的相同就不写。
   - **启动与 `test/seed`**：`Status::load` 从 `sessions` 行补入核心不认识的会话，和视图不同的才写出。行里没有失败码，所以沿用视图里的 `error_code`。至此 `rows::reconcile` 只管项目、`host` 和已删除的会话。
2. **`IndexNotes` 扩充与会话侧接线**
   - `IndexNotes` 改为两个方法：`created(summary)` 和 `note(session, at, Vec<Note>)`。`Note` 有六种：回合开始、回合结束（seq、状态、失败码）、待处理请求、分支、只读。
   - 会话 actor 在提交成功后，从本批事件里摘出这些通知，请求有变时再附上 `Pending`。`session/create` 发 `created`。
   - 条目的流式输出不产生通知。最近活动取每次通知的时间。
3. **已读与在场**
   - `Host::start` 调用了 `routes.presence(..)`；索引 actor 在邮箱里直接处理 `session/mark_read` 和 `client/presence`。
   - **已读**：只前移，并截在索引知道的最近回合结束的 seq。它写进 `sessions.last_read_seq`，经写线程提交，语句带 `last_read_seq < ?` 守住只前移，写完才回复。为此 `IndexDeps` 加了写线程句柄。
   - **在场**：只采信前台的 `desktop_ui`；连接关闭时清掉。
4. **通知意图**
   - 触发条件：进入 `needs_action` 时写 `needs_action`，带最早的请求 ID；回合以 `completed` 结束写 `turn_completed`；以 `failed` 结束写 `error`。
   - 意图在变化时判定，和摘要同一批写出。写出时如果有前台的 `desktop_ui` 在看这个会话，就丢掉；`needs_action` 的意图如果在这 100 ms 内请求已经答完，也丢掉。
   - `intent_id` 经 `IdGen` 生成。
   - 有序退出写检查点之前，先把攒着的变化写出。
5. **`agents` 接口**
   - `IndexHandle` 实现了 `AgentNotes`，宿主组装时换掉了 `NoAgentNotes`。每次上报替换对应 agent 那一项，没有变化就不写。
   - `NoAgentNotes` 换掉后只剩运行时的测试在用，挪成了测试里的本地桩。
6. **静止屏障**：索引 actor 的等待点仍然只有一个，在 `Participant::idle` 里同时等邮箱和 `Alarm`。没有新开脱离发起方的 task，也没有新的 actor 种类。
7. **CLI**：IT 要发 `client/presence`，CLI 客户端原来没有发通知的方法，加了 `Client::notify`（10 行）。

## 关键取舍

以下几处是我自己定的，记在 `notes.md` 里，请确认：

1. **`sessions.last_read_seq` 由 `IndexActor` 写。** 5.2.3 的 actor 表把 `sessions` 行归给会话 actor。但已读是索引的输入，而且要对没有加载的会话也生效，那时会话 actor 不一定存在。会话 actor 从不写这一列（只在 `session/get` 里读），所以按列分写者。如果认可，5.2.3 那张表需要补一句。
2. **已读位置截在最近回合结束的 seq。** 未读和"出错未查看"都只看回合结束的 seq，截在这里不改变任何状态。不截的话，一个 `up_to_seq` 很大的调用会让以后的回合都显示为已读。
3. **意图的原因**：`cancelled`（用户自己停的）和 `interrupted`（宿主重启后补写的）不写意图；`needs_action` 只在待处理数从 0 变成大于 0 时写。
4. **"agent 不可用"按回合失败处理**，不看 `agents` 的状态。否则一个 agent 未登录，它名下所有会话都会显示出错。
5. **标题、归档没有接。** 宿主里还没有改标题和归档的路径（`session/rename`、`session/archive` 在 T125；agent 起的标题也还没有来源），现在加 `Note` 变体只会是没人用的代码。交接给 T125。
6. **`agents.updated.git` 两项留空**：`GitTool` 不知道 git 和 git-lfs 的版本。
7. **摘要的写出不重试。** 写失败的摘要，下次启动时对账会补写，不在当次重试。

## 验证方式

- **UNIT-index-status**（`index/status.rs`，经 `CoreDriver`，4 个用例）：
  - 优先级与 `pending_kind`（approval 和 input 都覆盖）；
  - 失败显示 `error_code`，查看后清掉，之后有回合成功则只算未读；
  - 已读只前移，且截在最近回合结束处；
  - 100 ms 内的变化只写一次，回到原值不写；
  - 从行启动，不覆盖已知的会话。
- **UNIT-index-intent**（`index/tests.rs`，真实 `host.db` 上直接向 `IndexHandle` 输入）：
  - 三种原因各写出一条；
  - 前台 `desktop_ui` 在看时不写；CLI 的在场不算；
  - 100 ms 内答完的请求不写，`cancelled` 不写；
  - 转到后台之后会写，连接关闭之后也会写。
  - 连跑 20 次稳定。
- **IT-lifecycle**：
  - 新用例 `it_lifecycle_read_marks_and_presence_reach_the_index`：真实宿主上发 `client/presence`，然后依次 `session/mark_read` 5、3、100，`last_read_seq` 依次为 5、5、9（9 是最近回合结束的 seq）。
  - 原有的假 agent 回合用例加了检查：索引流里会话最终为 `unread`，有一条 `turn_completed` 意图，并且有 `agents.updated`。
- **其余**：IT-seed（3 条 `session.upserted`）和 IT-clock-advance（种类枚举）不改也照样通过。
- **本地**：`cargo clippy --workspace --all-targets` 在默认特性和 test-harness 下都无警告；`cargo test --workspace --features cupper-host/test-harness` 全过。
- **CI**：在干净的 worktree 上跑 `cargo xtask ci pr --record docs/tasks/T100-index-status/ci/` 一次通过：rust 1,694、protocol 185、ts 353 通过；sensitive-words 因本机没有词表而跳过。摘要 `pr-c88c08e11c38.json` 已提交。
- 测试不睡眠，用 `ManualClock`；测试名以用例编号开头，`tests/ids.toml` 没有改动。

## 提交列表

```
09c66f7 docs(task): record T100 CI summary
c88c08e docs(task): record T100 line counts and choices
3af6454 test(host): read marks, presence and the index on the real host
fea3e12 feat(cli): send client notifications
234f347 feat(host): write agents.updated from the agent runtimes
6e574ff feat(host): compute session status, read marks and intents in the index
1a5d87e docs(task): estimate T100 by module
```

## 遗留问题 / 范围外发现

**交接（建议写进 `handoff-notes.md`）**

| 接手任务 | 事项 |
|---|---|
| T125 | 改标题和归档要通知索引：给 `Note` 加标题变体（标题、来源）；归档时核心删掉这一项，并写 `session.removed{archived}`（启动时的对账已经会补）。agent 起标题的任务同样走标题变体 |
| T150 | 系统通知接在 `IndexActor::step` 处理 `Action::Intent` 的地方，或者读索引流里的 `notification.intent` |
| T089 | 恢复会话时，会话 actor 的 `Pending` 等通知会照常进入核心；启动时核心已经从行补入了这些会话 |
| T046 | CLI 上报在场可以用 `Client::notify::<ClientPresence>`；索引不采信 CLI 的在场 |
| 显示 git 版本的任务 | `AgentsUpdated.git` 现在为空，`GitTool` 要先能取到 git 和 git-lfs 的版本 |
| 设计文档 | 5.2.3 的 actor 表：`sessions.last_read_seq` 归 `IndexActor` 写（待确认，见关键取舍第 1 条） |

**范围外发现（没有修改）**

1. **`StreamWriter::append` 遇到空批次会丢掉行写入。** 空的 `bodies` 直接返回，`with` 里的行写入不执行，文档里也没写。会话 actor 遇到无事件的提交走 `db.submit`，所以没出问题；本任务的已读也因此改走写线程。建议给 `append` 补文档，或者在 `bodies` 为空、`with` 非空时报错。
2. **`session/get` 自己算状态。** T043 的 `session/get` 按行和会话视图算状态，规则和索引一致，但实现有两份。以后可以考虑让它读索引的摘要。