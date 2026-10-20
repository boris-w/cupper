# T088 汇报

- 执行者：子 agent（Rust 工程师，宿主编排；T043 的执行者）
- 分支：`feat/T088-orch-interaction`（从 main 开出）
- 完成日期：2026-10-20

交互代理已经接进组合核心和 actor，请求计数替身已删除。IT-orch-interaction 和两条组合性质都通过，`cargo xtask ci pr` 第二次运行通过（第一次失败，原因见"验证方式"）。规模 886 行，估计约 860 行，在约 1,000 行以内。要留意一件事：会话管理器的组装要多传两项（审计发送端和索引接口），见"遗留问题"第一条。

## 做了什么

- **组合核心（`session/core.rs`）**：`InteractionBroker` 替换了原来的请求计数。
  - **请求的顺序**：
    1. 先送裁判 `Asked`；空闲时裁判先开自发回合。
    2. 请求进暂存表。
    3. 只要有开着的回合（从 `turn.started` 到它的 Settle），就依次送合并器 `Interject`、再由交互代理写出，请求带上这个回合的 ID。
  - **回合收尾期间到达的请求**（T058 的交接，已确认）：没有开着的回合，所以先留在暂存表。裁判已经置了 `start_next` 并保持队列，回合 `turn.completed` 之后先开自发回合，请求写进去。请求解决之前，裁判不会结束这个回合。
  - **`CancelAsks`、`Settle`**：交给交互代理。Settle 的顺序是：结束开着的条目 → 解决挂着的请求 → 裁判收到 `Resolved` → 裁判收到 `Settled`。之后合并器才写 `turn.completed`。
  - **撤回、适配器退出**：已经写出的请求由交互代理处理；还在暂存表里的直接删掉（撤回时对 ACP 应答 `Cancelled`）。每次解决都送裁判 `Resolved`。
  - **还没 attach 时到达的请求**：直接应答 `Cancelled`。
- **actor**：
  - **`interaction/respond`**：带上 `resolved_by`（principal 和连接自报的客户端种类、名字），`Respond.tier` 按当前 `permission_mode` 和能力里的 `risk` 换算。
  - **`interaction/get`**：从会话视图取。
  - **`reply_to`**：只接受本会话里已经 `resolved` 的请求，否则回 invalid params；通过后写进用户消息和排队条目。
  - **审计**：CLI 或 test_harness 的回答提交成功后，发审计记录 `interaction.responded`。
  - **通知索引**：请求写出或解决之后，用新的 `IndexNotes::pending` 告诉索引待处理的个数和最早一个的类别；T034 之前用空实现 `NoIndex`。
- **`interactions` 行**：与 `interaction.requested`、`interaction.resolved` 同一事务。`request_json` 存整个 `interaction.requested`，T089 重启时可以据此恢复。
- **`facts.option`**：从会话视图里待处理请求的选项中取出所选的那一个。
- **网关**：
  - `Call` 带上连接在 `initialize` 里自报的 `ClientIdentity`；
  - 审计事件加 `InteractionResponded`；
  - `Gateway::new` 改为接收外面建好的审计发送端，以便会话共用。`supervisor`、网关夹具、`testkit` 的调用处相应改了。
- **协议（按你的裁定）**：
  - `interaction/respond` 的 `x-errors` 加 `not_found`，交互代理对不存在的请求回 `not_found`；
  - 跑了 `pnpm gen`，只有 schema 变了，TS 类型没有变化；
  - 改了 5.3.4 表里的这一行。
- **UNIT-unsupported-set**：`interaction/respond`、`interaction/get` 在 T033 时已经登记在 `Routes::sessions` 里，允许表里没有要删的行。
- **性质测试**：改用真实的交互代理。
  - 请求是 `DriverEvent::Ask`；解决时，三次里两次由 test_harness 回答，其余撤回。
  - 模型按事件记下待处理的请求，并新增一条检查：请求只写进正在进行、还没 Settle 的回合。
  - 生成器的权重没有变，T058 的种子仍然对应原来的操作序列。

## 关键取舍

- **`reply_to` 无效时回 invalid params，没有新增错误码**：客户端只在拒绝成功之后才发这条消息，所以出错说明客户端有 bug。
- **`interaction/get` 只看视图**：待处理的请求总在视图里（窗口里的回合，或 `detached`）。已经淘汰出视图、又已解决的请求回 `not_found`；从表里补归 T089。
- **核心处理完 `Settle` 后照常输出它**：性质测试靠它看到"结局已定"，actor 不处理。
- **同一个审计 channel**：宿主组装时，同一个发送端交给网关和会话。这比让会话另开一个 channel 更符合"发进现有的审计 channel"的裁定。

## 验证方式

- **IT-orch-interaction**：4 个用例，全部经 `Dispatch` 走策略 → 账本 → 会话端口 → actor → 写者，覆盖验收的 9 项：
  - **回合内审批**：请求带 `turn_id`；CLI 回答后 ACP 收到所选选项，`resolved_by` 正确。
  - **先到先得**：第二个回答得到 `already_resolved`，并带 `resolved_by`。
  - **被拒的回答**：摘要不符、principal 不在 `actionable_by` 内、选项要 `privileged`、window 不足 400 ms、请求不存在（`not_found`），各自被拒。
  - **附带核对**：审计记录、索引收到的待处理个数、`interactions` 行、`interaction/get`、`policy/describe`。
  - **回合外的请求**：开出自发回合；静默 30 s 后回答仍然有效，回答之后回合才结束。
  - **中断和撤回**：挂着的请求变为 `cancelled`（`turn_cancelled`、`withdrawn`），ACP 收到 `Cancelled`，都在 `turn.completed` 之前。
  - **`reply_to`**：指向已回答的请求时写进排队条目和用户消息；指向待处理或不存在的请求时被拒。
- **变异检查**：让裁判不再等请求解决就结束自发回合，"回合外"用例连跑 3 次都抓到。
- **组合性质**：默认用例数通过；`PROPTEST_CASES=20000` 各跑 3 次通过。临时计数显示，默认 256 例里约 60 个请求，解决方式覆盖了回答、撤回、中断取消、回合结束取消、适配器退出；回合收尾期间到达、先暂存的请求约 26 次。
- **不睡眠**：全部用 `ManualClock`。"回合外"用例推进时钟后 `yield_now` 一次，让同一线程上的 actor 先处理定时器；这不是睡眠。
- **CI**：`cargo xtask ci pr --record docs/tasks/T088-orch-interaction/ci/` 在干净的 worktree 里，第二次通过：
  - 第一次失败，是协议目录测试里 `interaction/respond` 的错误码表漏改；补了一个提交。
  - 第二次 rust 1,181 个、protocol 185 个、ts 314 个全部通过，license、testdata-scan 通过。

  摘要 `ci/pr-3ef960802e15.json` 已提交。
- **规模**：不含笔记和生成的 schema，新增 886 行、删除 195 行。估计约 860 行，在约 1,000 行的停止线以内。

## 提交列表

```
38fc857 docs(task): T088 CI summary
3ef9608 test(protocol): expect not_found among interaction/respond's errors
68083bd docs(task): T088 sizes, notes and handoffs
b46e41a test(host): IT-orch-interaction
70cb93a feat(host): wire the interaction broker into the session
9f487d6 feat(host): put the client on calls; audit answers given by the CLI
89d1f34 feat(protocol): interaction/respond may answer not_found
ca037d3 docs(task): T088 size estimate
```

## 遗留问题 / 范围外发现

- **T055（会话管理器的组装）**：`SessionDeps` 多了 `audit` 和 `index`。`audit` 用交给 `Gateway::new` 的同一个发送端（`supervisor` 里的 `records`）；`index` 在 T034 之前用 `NoIndex`。
- **T034**：实现 `IndexNotes`，换掉 `NoIndex`。
- **T059**：落盘新的审计记录 `InteractionResponded`，记为 `interaction.responded`。
- **T089**：
  - 用 `interactions.request_json`（整个 `interaction.requested`）和状态，经 `Input::Ended` 把已结束的请求交给交互代理；
  - `interaction/get` 对淘汰出视图的请求可以改从表里读。
- **T053、T055**：`DriverEvent::Ask` 带上 `toolCallId` 之后，在 `SessionCore::write_held` 里填上 `item_id`。
- **提交独立性的一处瑕疵**：实现提交（`70cb93a`）单独编译测试代码时，有一条 dead_code 警告（夹具的 `audit`、`index` 字段要到下一个提交才被读到）。最终的 HEAD 是干净的，CI 已通过。
- worktree 保留着。