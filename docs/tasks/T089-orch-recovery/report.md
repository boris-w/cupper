# T089 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T089-orch-recovery`（从 main 911a9dc 开出，没有合并 main）
- 完成日期：2026-10-20

## 做了什么

按裁定（计划 v1.3.38）收窄后的范围完成了载入与恢复。

1. **按需载入**（`session/load.rs`，`manager.rs`）
   - 调用到达时，如果会话没有 actor，就先在表里放好它的邮箱，再起一个任务：从库载入会话，然后运行 actor。
   - 同时到达的调用共用这一个邮箱，所以一个会话只有一个 actor。
   - 载入失败时（例如会话不存在，回 `not_found`），邮箱撤出表，期间收到的调用都回这个错误。投递发现邮箱已关闭时会重试一次，下一次调用会重新载入。
   - "不加载项目配置继续"这条宿主内部入口也走按需载入，出错时如实回错。
2. **从库恢复**：载入时读这些内容：
   - `sessions` 行；
   - `queue_entries`，包括每条排队消息的 `command_id`、`reply_to` 和发送者；
   - 最后一个回合的 `turn_no` 和 `ended_at`；
   - 已结束的 `interactions`；
   - 会话流的视图：配置、能力、仍在运行的回合、仍待处理的请求。

   这些内容组成 actor 的 `Parts`，以及作为第一个输入的 `Input::Restored`。各核心的恢复入口：
   - `TurnMachine::restore`：要恢复的 agent 会话（`Detached(agent_session_id)`）、回合号、队列、闸门原因；
   - `QueueCore::restored`；
   - `TierGuard::restore`：最后的权限模式及其档位，以及是否在等确认。载入时立即按找回的设定者检查一次；
   - 交互代理：经 `Input::Ended` 收到已结束的请求。
3. **设定者**：按 `mode_set_by` 的 ID，用握手时的映射（`gateway::principal_of`）找回。
   - `pr_desktop`：有 `full_access_confirmed_at` 时按 control 角色算，否则按 window 角色算。
   - `pr_cli`：只有 read。
   - `pr_test`：只在带测试 harness 的构建里存在。
   - 其他 ID，或者没有 ID：上限按 `normal` 算。
   - 两种假设的差别写在 `notes.md` 里，按裁定取"需要确认"。
4. **信任闸门**
   - 闸门原因的来源：`config_unconfirmed` 来自 `session.configured.confirmation_required`；`project_config_changed` 来自队列的暂停原因。
   - "想加载项目配置"重新得出为：`project_config` 是 `loaded`，或者闸门因 `project_config_changed` 关着；并且没有选过"不加载项目配置继续"。
   - 之后照原规则，由第一次重扫更新。
5. **`ignore_for_good` 持久化**：存进 `sessions.project_config_ignored`，与做出选择的那一步同一个事务提交。
6. **`canonicalize` 移出异步运行时**：重扫里的 `canonicalize` 和 `is_dir` 一起挪进了 `spawn_blocking`。
7. **`interaction/get`**：视图里没有的请求，改从 `interactions` 表读（`request_json` 加上结束状态）。
8. **补写与队列**
   - 以视图为准，先补写待处理请求的 `interaction.resolved{expired, host_restarted}`，再补写运行中回合的 `turn.completed{interrupted}`。`ended_at` 取启动清理写进行里的时间。
   - 队列里有消息又没暂停的，一律以 `turn_interrupted` 暂停：重启后在用户操作之前不会自己开始回合。
   - 这些事件随行和索引通知一起提交。
9. **两项零碎**
   - `StreamWriter::append` 遇到空 `bodies` 时，单独开一个事务执行 `with`，并补了文档。actor 的每一步因此统一经流写者提交，去掉了 `Parts.db` 和 `SessionDeps.writer`。
   - `session/get` 与索引改为共用 `index::status_of`，两边算状态的规则只剩一份。
10. **IT-orch-recovery**，共 4 个用例：
    - 带运行中回合、待处理请求、已答请求、带 `reply_to` 的排队消息时重启。检查：补写的事件和顺序、队列暂停、对过期请求作答被拒、`session/get` 与索引行一致、恢复 agent 会话后继续、排队消息带着 `command_id` 和 `reply_to` 跑完、回合号接着往下编；
    - 视图淘汰的请求改读表；
    - 设定者按 ID 找回，两个调用同时到达只起一个 actor；
    - 重启后仍不加载项目配置。
11. **启动清理**：清理那段 SQL 抽成 `supervisor::recover::end_previous_run`，测试里的重启调用同一段代码。

## 关键取舍

- **`0002` 还是改 `0001`**：裁定写的是迁移 `0002`，但计划 P-4 规定"T160 之前表结构只改 `0001`"，而且 `store_schema_*` 用例要求只有 `0001`，并与设计 5.4.2 一致。所以我把列加进 `0001`，并在 `docs/design/04-data-model.md` 的 5.4.2 表结构里加了一行（这个用例从设计文档读 SQL）。**这处改了设计文档**，与以往任务改表结构时的做法相同，请编排者确认。
- **重启后的第一次检查**：载入时守卫用库里最后的权限模式立即检查一次，而不是等第一次连接。
  - 恢复出的设定者更保守、结论变成需要确认时，载入当时就进入 `config_unconfirmed`，界面上马上能看到，`confirm_config` 也能立即生效。
  - 崩溃前已经在等确认的状态会照原样恢复，不会因为按 control 角色算而丢掉。
- **补写以视图为准**，不以行为准：视图不认识的回合或请求，写进去也会被拒。日志被修剪、视图缺内容的情况归 T169。视图里没有配置时，载入回内部错误，等 T169 来重建。
- **队列一律暂停**：没有运行中回合、但队列没暂停的情况也暂停，原因同样记为 `turn_interrupted`。我选了"重启后不自己跑"这一侧。
- **重新连接时的选项**：恢复的会话用 `config_json`（最后已知的全部选项值）作为 `Attach.config`。驱动只设置与当前值不同的选项；权限模式若需确认，闸门会挡住 prompt。
- **规模**：自分支点起新增 **958 行**，其中实现 722、测试 236，另有 1 行设计文档。
  - 低于硬停止线 980，**但超出约 900 的上限**。主要来源：读行和类型转换比估计多约 120 行；夹具为模拟重启重构了约 40 行。
  - 中途发现超出后做了压缩（合并载入路径、精简映射），没能压到 900 以内。分模块数字在 `notes.md`。

## 验证方式

- `cargo test -p cupper-host`：无特性和全特性都跑了，全部通过（lib 390 个，`lifecycle` 16 个）。
- `cargo clippy -p cupper-host --all-targets`：全特性和无特性都没有警告。
- 变异检查，三项都会被用例抓到：
  - 去掉 `ignore_for_good` 的读回 → 第 4 个用例失败；
  - 不恢复 `command_id` → 第 1 个用例失败；
  - 邮箱不先入表 → 第 3 个用例的并发载入失败。
- `cargo xtask ci pr --record docs/tasks/T089-orch-recovery/ci/`：在干净工作区运行，各作业全部通过（rust 1892、protocol 185、ts 454、license、testdata-scan、ui、e2e 5），摘要已提交。
  - `ui/test-ui` 跳过：截图依赖的路径没有改动；
  - `sensitive-words` 跳过：本机没有词表。
- 对照验收标准：
  - IT-orch-recovery 覆盖了收窄后范围的各项；
  - `session/get` 与索引已对齐（共用规则，用例核对状态和索引用到的行字段）；
  - 测试名以用例编号开头；
  - CI 已通过并留了记录。
- 有一轮测试因为没先构建 e2e CLI 而失败，留下了 cupper-host 和假 agent 进程，我核对是本 worktree、本次测试起的之后，用 SIGTERM 结束了它们。现在没有残留进程。

## 提交列表

```
c630d69 chore(ci): record T089's PR run
c4dc4d8 docs(task): T089 line counts per module
242a9c4 feat(orch): load sessions from the store on demand
1592647 refactor(index): one status rule for the index and session/get
b3bf845 style(store): format the empty-batch change
f2b0ee4 feat(orch): restore entries for the session's cores
5a61274 feat(orch): store the choice to go on without the project's configuration
b12dfe6 perf(orch): resolve the workspace off the async runtime
678b114 fix(store): an append without events commits its rows
4568987 docs(task): T089 notes after the ruling
65de872 docs(task): T089 size estimate and split proposal
```

## 遗留问题 / 范围外发现

1. **被中断回合里没结束的条目**（例如进行中的工具调用）不补写 `item.completed`。回合已经 `interrupted`，但这些条目在视图里仍是进行中。建议界面把已结束回合里的条目一律当作已结束，或者在 T169 里补写。
2. **`SessionPort::facts`**（只供 `policy/describe` 用）仍要求会话已在内存里。会话未载入时，描述里不带所选选项的信息。重启后待处理的请求都已过期，影响很小；没有改。
3. **设计文档**：本任务在 5.4.2 加了 `project_config_ignored` 一行。另外，5.2.13 的启动恢复没有写"载入后有消息的队列一律暂停"这条规则，建议补上。
4. **`pr_test` 的映射**：没有测试 harness 的构建里，`pr_test` 按"来源不明"处理，这是有意的。CLI 提权期间设定的档位，重启后按 CLI 只读的上限算，比崩溃前更严。
5. **ID 被提交两次**：裁定后我对本分支最后一个提交做过一次 `git reset --soft` 再重新提交，为了把设计文档那一行并入同一个提交。没有 rebase、merge 或 push。
6. T169、T175 可以直接用 `session/load.rs` 的读行，以及 `Fixture::restart`。空闲退出可以用管理器的 `forget`：只在邮箱匹配时才从表里撤出。
