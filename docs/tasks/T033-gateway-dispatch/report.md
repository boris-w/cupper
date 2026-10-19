# T033 汇报

- 执行者：子 agent（Rust 工程师，宿主网关）
- 分支：`feat/T033-gateway-dispatch`
- 完成日期：2026-10-19

## 做了什么

T033 已按拆分后的范围（方案 A，计划 v1.3.7）完成：分发、账本与路由。`cargo xtask ci pr` 在干净的 worktree 里全部通过：rust 1011、protocol 176、ts 232、license 1，失败均为 0。摘要已提交为 `ci/pr-3c80c9fa1d70.json`。testdata-scan、ui、e2e 三个作业还没实现，摘要里记为 `not_implemented`。

1. **分发流水线**（`gateway/dispatch.rs`，新）：
   - 读循环对每条请求同步调用 `Dispatch::request`，依次是：
     1. 查方法表，没有登记的回 `unsupported`；
     2. `policy::method`；
     3. 按方法的参数类型校验，不通过回 invalid params；
     4. 三条只看参数的规则：`settings_update`、`set_config`、`git_revert`；
     5. 申请在途名额；
     6. 命令走 `Ledger::begin`：`Run` 时 ticket 随调用交给执行者，`Replay` 直接回复，`Wait` 另起 task 等首次执行的结果；
     7. 投递。
   - 缺省的 `params` 一律按 `{}` 处理。
   - 客户端通知走同样的检查，任何一步不通过就静默丢弃。
2. **端口接口**：
   - `Port::deliver(Call) -> Reply`：只排队，回复的 future 由网关在单独的 task 里等待。所以同一连接发往同一会话的命令按到达顺序进入邮箱，回复按完成的先后发出。
   - `Call` 带连接、principal、方法、params 和 `Option<Ticket>`。
   - `SessionPort` 在 `Port` 之上加 `facts(&Confirmable)`。
   - `IndexPort` 在 `Port` 之上加 `opened(Opened)` 和 `closed(&ConnId)`。
3. **方法表**（`gateway/routes.rs`，新）：
   - 运行时登记：`add::<M>`、`notification::<N>`，带 `x-builds` 的方法只在相应构建里登记。
   - `Routes::sessions(port)` 登记 M1a 的会话方法：create、send、interrupt、queue_resume、get、capabilities，以及 `interaction/respond`、`interaction/get`；同时登记 `policy/describe`。
   - `Routes::index(port)` 登记 `project/add`、`project/trust`、`project/trust_info`、`session/mark_read`，以及 `client/presence`。
   - `Routes::default()` 只有 `ping` 和给 T019、T029 的留位，没有账本，供网关夹具和 `TestGateway::start` 使用。
4. **在途限制**：每条连接最多 64 个在途请求，`ping` 和当场回复的请求也计数。第 65 个回 `rate_limited`，不带 `retry_after_ms`（按你的裁定）。
5. **`policy/describe`**（`gateway/describe.rs`，新）：create、set_config、respond、exit、prepare_upgrade 这几类向会话端口取 `Facts`，其余用默认值，然后调用 `policy::describe`。
6. **连接与断开**：`initialize` 成功后调用 `IndexPort::opened`；连接结束时，在写 `conn.closed` 审计之前调用 `closed`。每条连接的 `Dispatch` 随连接一起丢弃。
7. **组装入口**：`Routes::start(conn, writer, clock, durations, sessions, index)` 装载账本、启动清理任务，返回 `(routes, sweeper)`；调用方再执行 `Gateway::new(config, clock, durations, routes)`。
8. **给 T019、T029 的接入点**：
   - `stream/subscribe`、`stream/unsubscribe`、`history/page` 在方法表里登记为 `Target::Streams`，`blob/*` 登记为 `Target::Blobs`。
   - 在它们实现之前，这些方法和没登记的方法一样，在策略检查之前就回 `unsupported`。
   - 具体怎么接，写在 `notes.md`"给 T019、T029 的接入点"一节。

## 关键取舍

- **方法表在运行时登记，不是写死的路由表。** 每个执行者在接上时登记自己的方法，所以 M1b 的方法不会登记到端口上，再由端口内部回 `unsupported`。UNIT-unsupported-set 因此能按真实行为断言。
- **端口返回回复的 future，而不是接收一个回复通道。** 投进邮箱的执行者（会话、索引）和"另起 task 执行"的执行者（设置、git 等）用同一个接口；在途名额和回复的写出都由网关统一管理。
- **在途名额先释放，再写回复。** 这样客户端读到回复后立即发出的下一条请求，不会因为名额还没释放而被误拒。
- **UNIT-unsupported-set 按行为断言。**
  - 用拥有全部 scope 的 principal，对协议目录里的每个请求真实分发一次，收集返回 `unsupported` 的集合，与带原因的允许表比较。
  - 每行写明原因：属于后续里程碑，或者"由哪个任务接上"（T019、T029、T036、T037、T038、T044、T047、T055、T183）。原因再与目录里的 `x-milestone` 交叉核对。
  - 客户端通知用"是否投递到端口"来判断。
- **`session/confirm_config` 不登记到会话端口**，在允许表里标为 T044，避免 T043 和 T044 之间这个方法藏在端口内部回 `unsupported`。
- **用例编号**：网关层的策略和账本用例没有新编号，挂在已有的 UNIT-policy-matrix、UNIT-ledger 下；走真实套接字的那条挂在 IT-command-order 下。如果要另立编号，请告诉我。
- **接线时顺带改的地方**：
  - `Gateway::new` 多了一个 `Routes` 参数，网关夹具和 `TestGateway` 跟着改了；
  - T039 的两条用例改用 `TestGateway::with(.., Routes::default())`；
  - `Phase::Ready` 里的 `Dispatch` 装进了 `Box`，因为 clippy 报了 `large_enum_variant`。

## 验证方式

- **IT-command-order**（3 条）：
  - 同一会话的命令按到达顺序投递，命令带 ticket，查询不带；
  - 会话 1 阻塞时，会话 2 照常回复；回复按完成的先后发出；
  - 第 65 个在途请求，以及在途满时的 `ping`，都回 `rate_limited`，不带 `retry_after_ms`，也不会投递出去；释放一个名额后恢复；
  - 执行者丢掉调用时，客户端收到 `internal`；
  - 真实套接字上（`peer` 认证）：流水线式发出的请求按顺序投递，回复乱序返回；`opened`、`closed` 都被通知；`client/presence` 投递到索引端口。
- **UNIT-unsupported-set**（2 条）：上面说的集合与原因核对；构建门控：`test/clock_advance` 只在 test-harness 构建里能用，`diag/recording_export` 只在 test-harness 或 dev-auth 构建里能用。
- **网关层的策略**：
  - scope 拒绝（`write`、`privileged`）；
  - 三条只看参数的规则（scope、role、`user_activation_required`）；
  - invalid params；
  - 以上被拒的请求都没有执行；规则允许的请求照常执行；
  - 被拒的通知静默丢弃；
  - `policy/describe` 能用 `Facts`，不需要确认的命令回 invalid params。
- **网关层的账本**：
  - 重放带 `replayed: true`，宿主重启后从表里装载同样能重放；
  - 首次执行还在进行时，重复请求等待它的结果；
  - `idempotency_mismatch`：同一 `command_id` 换参数、换方法都会触发；
  - `outcome_unknown`：执行者 `mark_external` 之后没有记录结果就停下。
- **稳定性**：网关用例连续跑 10 次，30 条全部通过。四种特性组合（无特性、dev-auth、test-harness、全部）的 clippy 和测试都通过。
- **规模**：新增约 1,355 行，在上限之内。

  | 部分 | 增 | 删 |
  |---|---|---|
  | 实现 | 572 | 52 |
  | 测试 | 783 | 0 |

  测试比估计的 630 行多，多在替身和驱动分发的辅助代码上。

## 提交列表

```
50aefc2 docs(task): record the T033 pr CI summary
3c80c9f docs(task): T033 notes after the split, access points for T019 and T029
bfe96ee test(host): IT-command-order, UNIT-unsupported-set, policy and ledger at the gateway
d0fc9e2 fix(host): answer gateway methods not served yet before their params
1843340 feat(host): dispatch requests through policy, ledger and routes
936c859 Merge branch 'main' into feat/T033-gateway-dispatch
4753797 docs(task): T033 size estimate and split proposal
```

## 遗留问题 / 范围外发现

各后续任务的接入点详见 `notes.md`"给其他任务的接入点"一节，下面是需要交接的要点：

- **T043**：
  - 实现 `SessionPort`：`deliver` 按 `params.session_id` 投进会话邮箱，不等待；用 `Call.ticket` 结束命令。
  - 实现 `facts(&Confirmable)`，给出 `mode_tier`、`option` 和 `running_turns`。
- **T044**：登记 `session/confirm_config`，并删掉允许表里那一行。
- **T034**：
  - 实现 `IndexPort`。
  - `ClientInfo.auth` 由 `Opened` 里的 principal 种类和 `weak_auth` 推出。
- **T036**：登记 `settings/*`。`settings_update` 规则网关已经执行过。
- **T037**：
  - 按 `Routes::start` → `Gateway::new` 组装；有序退出时 abort 清理任务。
  - 登记 `host/status`、`host/exit`。
- **T038**：
  - 撤销提权挂在 `IndexPort::closed` 上。
  - `Dispatch` 里的 principal 在 `initialize` 时定下，之后不再变。授予提权需要让这条连接的 scope 能更新，T038 要改这里。
- **T047**：
  - `test/run_maintenance` 要用账本，届时给 `Routes` 加一个公开的账本访问函数（现在是 `pub(super)`）。
  - 账本清理任务、各连接等待回复的 task，还没有登记进静止屏障。
- **T019、T029**：订阅表和上传缓冲放进每条连接的 `Dispatch`，断开时自动清理；`HOST_FEATURES` 仍为空，由它们补上。
- **范围外**：客户端通知不计入在途，每条通知另起一个 task，数量没有上限。目前只有 `client/presence` 一种，代价很小；以后如果加入代价大的通知，需要再定上限。

worktree 和分支已保留，没有执行 merge、rebase 或 push。
