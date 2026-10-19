# T033 网关 II：分发、订阅、历史与 blob 传输

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主网关）
- 分支：`feat/T033-gateway-dispatch`
- 依赖：T026、T027、T030、T031、T032、T045（均已合并）。**在关键路径上**：T031 → T033 → T043 → T044 → …。

## 背景

T030 做好了网关的连接、准入、读写循环和握手。`initialize` 之后，网关只认 `ping`，其他方法一律回 `unsupported`。

本任务把网关接上已有的各个部件：
- 策略（T031）；
- 账本（T032）；
- 流、订阅与历史（T026）；
- blob 存储（T027）。

在此基础上，实现 `stream/*`、`history/page`、`blob/*`，以及把会话命令、索引命令路由到对应的邮箱接口。会话和索引的业务本身归 T043、T034。

## 范围

**做**（计划 T033 一行；设计 5.2.4、5.3.3、5.3.4 中的"流与内容"）：
1. **分发流水线**：读循环 → 策略 → 账本 → 路由。
   - **策略**：每个请求和客户端通知先调用 `policy::method(p, M::NAME, M::SCOPE.as_ref())`；再对相应方法调用只看参数的规则 `settings_update`、`set_config`、`git_revert`。
   - **账本**：命令类请求调用 `begin`。
     - `Run(ticket)`：把 ticket 随命令交给执行者；
     - `Replay`：直接回复；
     - `Wait(w)`：另起 task，等 `w.outcome()` 后回复。
   - **路由**：会话命令按顺序投递到会话邮箱接口，索引命令投递到索引接口。这两个接口由本任务定义成 trait 或 channel，测试里用替身；T043、T034 来实现它们。
   - **在途限制**：每个连接最多 64 个在途请求。
   - **未注册的方法**回 `unsupported`。带 `x-builds` 的方法只在相应特性的构建里注册。
2. **`stream/subscribe`**：
   - 四种情况：调用 `sync::subscribe_mode(resume, &LogExtent)`。
   - 快照分页：用 `paginate(snapshot_ops(), 帧负载减去信封的余量)`。
   - 网关的发送队列实现 `store::streams` 的 `Sink`：两个方法都在流锁内调用，只能排队，不能回调流。
   - 订阅时，把响应、快照页或补发标记放进 `subscribe` 的 `start` 回调。
   - 补发用 `store::log::read_events`。补发时发现事件已被裁掉，按 `cursor_expired` 改发快照，或者发 `stream/reset`，由你定，写进汇报。
   - 发送时按 `coalesce_ms` 合并，`coalesce_ms` 已由 T030 存在 `Session` 里。
   - 订阅模式写日志（Q16）。
   - `stream/unsubscribe`；客户端通知 `stream/ack` 是 M3 的，按里程碑处理。
3. **`history/page`**：
   - 用 `store::history::page`；
   - 不带游标时，用 `StreamHandle::read` 取窗口里最早的 `turn_no`；
   - `max_bytes` 截到 48,000。
4. **blob 上传与下载**：
   - 先收 `0x02` 分片，再收请求，帧格式按 5.3.1、5.3.4；
   - `BlobStore::put(data, media_type, Some(hash))`：`Mismatch` 对应 `conflict`，`TooLarge` 对应 `too_large`；
   - 客户端声明的 `size` 由你和 `data.len()` 比对；
   - `get` 每次都读整个文件并核对哈希，`offset`/`length` 由你截取；
   - 上传缓冲有上限，断开时丢弃。
5. **`policy/describe` 的处理器**：按 `Confirmable` 的种类查好 `Facts`，再调用 `describe`。`Facts` 由各执行者提供，接口由你定。
6. **断开清理**：
   - 从所有流注销订阅；
   - 丢弃上传缓冲；
   - 通知索引接口更新 `clients`。
   - 撤销这条连接上的提权，提权本身归 T038，这里只留接口（5.2.4）。
7. **启动接线的入口**：
   - 账本：`Ledger::load` 和 `sweeper`；
   - 流：`Streams`；
   - blob：`BlobStore`。

   给 T037 提供组装入口，真正的启动顺序归 T037。

**不做**：
- 会话和索引的业务（T043、T034）；
- 进程级用例（T062）；
- 提权（T038）。

## 验收标准

只验收网关层。测试放在 crate 内，以 `Streams::append` 作写者，会话和索引接口用替身。

- [ ] 新增 IT-gw-subscribe，覆盖：
  - 四种订阅：无游标、续传、`cursor_expired`、`cursor_ahead`/`epoch_changed`；
  - 快照分页；
  - 慢客户端重置；
  - 帧上限；
  - 账本重放；
  - epoch 与游标规则。
- [ ] 新增 IT-command-order：同一会话的命令按到达顺序投递，不同会话之间互不阻塞；第 65 个在途请求按 5.2.4 处理。
- [ ] 新增 UNIT-unsupported-set：返回 `unsupported` 的方法集合，等于按里程碑允许的集合。M2 的 `diag/export` 也要算进去，T013 的交接提过。
- [ ] 策略拒绝、账本 `idempotency_mismatch`、`outcome_unknown`，在网关层各有用例。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T033-gateway-dispatch/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **订阅**（T014）：订阅时调用 `sync::subscribe_mode(resume, &LogExtent)`；快照分页用 `paginate(snapshot_ops(), 帧负载减去信封的余量)`。
- **流的接口**（T026）：
  - 发送队列实现 `Sink`，两个方法都在流锁内调用，只能排队；`push` 返回 `Full` 时，流会摘掉这个订阅者并调用 `reset`（`slow_consumer`）；
  - 订阅时，把响应、快照页或补发标记放进 `start` 回调；
  - 补发用 `read_events`；
  - `history/page` 用 `store::history::page`，`max_bytes` 自己截到 48,000。
- **补发与裁剪的竞态**（T026）：补发时读到的事件可能刚被裁剪掉。发现缺口时，按 `cursor_expired` 改发快照，或者发 `stream/reset`，由你定。
- **策略**（T031）：
  - `policy::method`，以及三个只看参数的规则；
  - `policy/describe` 按 `Confirmable` 查 `Facts`；
  - `Denial`、`DescribeError` 都实现了 `Into<RpcError>`。
- **账本**（T032）：
  - 启动时 `Ledger::load`，并 `spawn(ledger.clone().sweeper())`；
  - `begin` 的三种结果怎么处理，见范围第 1 条；
  - `LedgerError` 可以直接转成 `RpcError`；
  - `test/run_maintenance` 调用 `Ledger::sweep()`。
- **blob**（T027）：见范围第 4 条；图片的 10 MiB 限制（按 agent）不在 `BlobStore` 里。
- **按构建门控**（T013）：带 `x-builds` 的方法只在相应特性的构建里注册；UNIT-unsupported-set 要算进 M2 的 `diag/export`。
- **认证**：T039（密钥认证）正在并行进行。本任务的用例用 `peer` 认证，或者在 crate 内直接构造 principal。计划里"用到 `dev_key` 的用例由测试进程自己出示"这一条，等 T039 合并后由 T062 覆盖。
- **网关现状**（T030）：
  - 只认 `ping`，其他方法都回 `unsupported`；
  - `ping` 的 `params` 为 `null` 时按 `{}` 处理；
  - 连接任务还没有登记进静止屏障（T047）。
- **惯例**：
  - 时间只经 `Clock`；
  - 每条事件流只有一个写者；
  - 错误类型用 thiserror；
  - 改了协议类型就跑 `pnpm gen` 并提交生成物（T015）。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 并行注意

- T039 在改 `gateway/handshake.rs` 和 `cupper-cli`；本任务尽量把分发、订阅、blob 放进新文件，`conn.rs` 里只改分发的入口。
- T042、T041 在改 `orchestrator/`，本任务不碰。

## 规模

计划估为 L，上限约 1,500 行。这个任务的接线面很广，**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**；推算会超出时，开工前就停下来汇报，并给出拆分建议，例如把"订阅与历史"和"分发、账本与 blob"分开。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**预计会超出上限时，必须先停下来汇报，不要继续写**。

## 参考资料

- 设计：
  - `02-host.md` 5.2.3、5.2.4（网关）、5.2.5、5.2.8；
  - `03-protocol-v1.md` 5.3.1、5.3.3、5.3.4（流与内容）、5.3.9、5.3.11 末尾的各项裁定；
  - `08-security.md` 5.8.3。
- T026、T027、T030、T031、T032 的代码、`notes.md` 和汇报。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-19，编排者）

执行者开工前按模块估算，总规模约 2,800 行：分发约 1,165 行、订阅与历史约 1,260 行、blob 约 375 行。按守则第 8 条，在写代码之前停下汇报。

**裁定：拆成三个任务（计划 v1.3.7）。**
- **T033** 收窄为分发、账本与路由，仍在关键路径上，内容包括：
  - 读循环 → 策略 → 账本 → 路由；
  - 方法表在运行时登记；
  - 会话端口和索引端口；
  - 每连接 64 个在途；
  - `policy/describe`；
  - 连接与断开的通知；
  - 组装入口。
  - 验收：IT-command-order、UNIT-unsupported-set，以及账本重放、策略拒绝在网关层的用例。
- **订阅、历史与发送合并**另立 T019（IT-gw-subscribe）。
- **blob 传输**另立 T029（新增 IT-gw-blob）。
- T019、T029 都依赖 T033，可以与 T043 并行。

执行者提出的 6 项待裁定事项全部认可，已写进 `03-protocol-v1.md` 5.3.11 末尾的"T033 开工前的裁定"。其中第 4–6 项归 T019。

