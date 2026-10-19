# T033 工作笔记

- 执行者：子 agent（Rust 工程师，宿主网关）

## 开工前的规模估计

按模块估计，行数指 rustfmt 展开后的行，含文档注释和空行。按任务书的拆分思路分成三组：分发（D）、订阅与历史（S）、blob（B）。

### D：分发、账本与路由

**实现**

| 模块 | 内容 | 估计 |
|---|---|---|
| `gateway/dispatch.rs`（新） | `Call`（连接、principal、方法、params、ticket）；`Port`：`deliver` 由读循环按到达顺序调用，只排队，返回回复的 future；方法表 `Routes`：`add::<M>` 记下 `SCOPE`、`IDEMPOTENCY`，按 `M::BUILDS` 只在相应构建登记；流水线：查表（没有就 `unsupported`）→ `policy::method` → 三条只看参数的规则 → 在途许可（64，超出 `rate_limited`）→ 命令过 `Ledger::begin`（`Run` 带 ticket 投递，`Replay` 直接回，`Wait` 另起 task）→ 投递后另起 task 等回复；客户端通知同样过策略，被拒时静默丢弃 | 320 |
| `gateway/services.rs`（新） | `Services`：账本、两个端口、方法表；组装入口 `Services::start`（`Ledger::load`、`spawn(sweeper)`）；M1a 的会话方法登记到会话端口，索引方法登记到索引端口；`SessionPort`（另有 `facts`）、`IndexPort`（另有 `opened`、`closed`） | 110 |
| `gateway/describe.rs`（新） | `policy/describe`：`Confirmable::parse` → 需要时向会话端口要 `Facts`（create、set_config、respond、exit、prepare_upgrade）→ `describe` | 45 |
| `conn.rs`、`mod.rs`、`testkit.rs` 的接线 | 读循环把请求、客户端通知交给分发；`initialize` 成功后通知索引端口 `opened`，断开时 `closed`（T038 的撤销提权也挂在这里）；`Gateway::new` 多一个 `Services` 参数 | 60 |
| 小计 | | **535** |

**测试与测试辅助**

| 项 | 内容 | 估计 |
|---|---|---|
| `gateway/testkit.rs` 扩充 | 会话端口替身：按到达顺序记下调用，由用例决定何时回复；索引端口替身：记下 `opened`、`closed`；带 host.db、写线程、账本的夹具；在 crate 内构造 principal、直接驱动分发并从 `Outbox` 解出帧的辅助 | 180 |
| IT-command-order | 同一会话的命令按到达顺序投递；前一个会话阻塞时，另一个会话的命令照常回复；第 65 个在途请求 `rate_limited`，释放一个后恢复 | 130 |
| UNIT-unsupported-set | 枚举协议目录的全部请求和客户端通知，断言"返回 `unsupported` 的集合"等于允许集合；允许集合是一张表，每项写明原因 | 100 |
| 网关层的策略与账本 | scope 拒绝；参数规则（用例给 `settings/update` 登记替身）；通知被拒时静默；账本 `Replay`（`replayed: true`）、`Wait`、`idempotency_mismatch`、`outcome_unknown`；缺 `command_id` | 160 |
| `policy/describe`、连接与断开 | 要 `Facts` 和不要 `Facts` 的两类；真实套接字上 `initialize` 之后 `opened`，断开之后 `closed` | 60 |
| 小计 | | **630** |

**D 合计约 1,165 行。**

### S：订阅、补发、发送时合并与历史

**实现**

| 模块 | 内容 | 估计 |
|---|---|---|
| `gateway/subscribe.rs`（新） | 每连接的订阅表：上限 64；`Opening`、`Live` 两态，保证同一连接上 subscribe 和 unsubscribe 的先后；`stream/subscribe`：`open` 后在 `start` 回调里排响应、快照页或补发标记；快照分页的预算是帧负载减去最长的信封；`stream/unsubscribe`；Q16 记录（模式、原因、游标落后量） | 220 |
| `gateway/outbox.rs` 改动 | 实时项改存 `Arc<Published>`，发送时才编码，这样才能合并；`Replay` 标记项；`reset` 带原因，并一并丢掉这条流的标记；直接实现 `Sink`；与 `streams::Push` 合成一个类型 | 90 |
| `gateway/send.rs`（新，写循环） | 取出队列；`coalesce_ms > 0` 且取到实时项时，按 `Clock` 再等一个窗口；同一流相邻的实时事件合成一条 `stream/events`，协商了 `stream.coalesce` 时再用 `coalesce` 合并 delta；按帧负载装帧；`Replay` 项按 seq 分段从读池 `read_events`，发现缺口时摘掉订阅、清掉这条流已排队的实时项、发 `stream/reset` | 190 |
| `gateway/wire.rs` | 由事件的 JSON 原文拼 `stream/events` 帧；`stream/snapshot` 帧 | 40 |
| `gateway/history.rs`（新） | `history/page`：不带游标时用 `StreamHandle::read` 取窗口里最早的 `turn_no`；`max_bytes` 截到 48,000；`store::history::page` | 60 |
| 接线 | `audit.rs` 加订阅记录；`HOST_FEATURES` 加 `stream.coalesce`、`history.page`；`Services` 加 `Streams`、`Readers`；断开时注销全部订阅 | 40 |
| 小计 | | **640** |

**测试与测试辅助**

| 项 | 内容 | 估计 |
|---|---|---|
| 夹具 | 网关接上真实的 `Streams`（临时 host.db、写线程、读池、可调的保留参数）；以 `Streams::append` 作写者的追加辅助；读裸帧的客户端 | 120 |
| IT-gw-subscribe | 无游标（快照页，之后是实时事件）；续传；`cursor_expired`（小保留参数加检查点裁剪）；`cursor_ahead`、`epoch_changed`；只带 `epoch` 或只带 `after_seq` 按无游标处理；快照分页（多页、页号、`last`、每帧不超过 64,000 字节）；补发分帧；慢客户端重置（不读，追加超过 4 MiB）；发送时合并（协商与不协商 `stream.coalesce`）；订阅上限；unsubscribe 之后不再收到；Q16 记录 | 400 |
| `history/page` | 窗口之外的回合、无游标、`max_bytes` 截断、`not_found` | 70 |
| outbox 单元测试的改动 | 新的项；标记随重置丢弃 | 30 |
| 小计 | | **620** |

**S 合计约 1,260 行。**

### B：blob 传输

| 项 | 内容 | 估计 |
|---|---|---|
| `gateway/blob.rs`（新） | 上传缓冲：按 hash 收分片，分片必须连续；单个 32 MiB、每连接合计 64 MiB，超出或乱序时作废并记下原因；没有 `write` 的连接发来的分片直接丢弃；`blob/put` 在读循环里取走缓冲，再另起 task：比对声明的 `size` 和长度，调用 `BlobStore::put(.., Some(hash))`，`Mismatch` 和长度不符对应 `conflict`，`TooLarge` 对应 `too_large`；`blob/get`：整读核对后按 `offset`/`length` 截取，按 `BlobChunkHeader::MAX_DATA` 分片，先发分片再回结果 | 190 |
| 接线 | 读循环把 `0x02` 帧交给上传缓冲；`Services` 加 `BlobStore`；`HOST_FEATURES` 加 `blob.get`、`blob.put`；断开时丢弃缓冲 | 25 |
| 测试 | 上传下载往返；hash 不符、长度不符、`too_large`；乱序分片；`offset`/`length`；`not_found`；只读连接的分片被丢弃 | 160 |

**B 合计约 375 行。**

### 合计

约 2,800 行（D 1,165 + S 1,260 + B 375），接近上限的两倍。按任务书，开工前停下，见下一节。

## 汇报与拆分建议

**方案 A：拆成三个任务（建议）。**

- **T033（D，约 1,165，关键路径）**：分发流水线、方法表与按构建登记、会话端口与索引端口、在途限制、账本接线、`policy/describe`、连接建立与断开时通知索引端口、组装入口（账本）；IT-command-order、UNIT-unsupported-set、网关层的策略与账本用例。T043、T034、T036 只依赖这一部分。
- **T033b（S，约 1,260）**：订阅、补发、快照分页、发送时合并、慢客户端重置、Q16、`history/page`；IT-gw-subscribe。依赖 T033，可以和 T043 并行。
- **T033c（B，约 375）**：blob 上传下载。依赖 T033，可以和 T033b、T043 并行。T033b 和 T033c 都只在 `conn.rs` 的分发入口和 `Services` 上附加，冲突很小。

**方案 B：按任务书的例子拆成两个。**

- 分发、账本与 blob（约 1,540）：略超上限，而且把 blob 放上了关键路径；
- 订阅与历史（约 1,260）。

**方案 C：拆成两个，blob 跟着订阅。**

- 分发（约 1,165）；
- 流与内容，即订阅、历史和 blob（约 1,635）：超过上限。

计划把"账本重放"列在 IT-gw-subscribe 里。按方案 A，它归 T033 的网关层账本用例，IT-gw-subscribe 只留订阅相关的部分。

## 设计要点与待裁定

拆分后照下面的设计实现；带"待裁定"的几条请编排者一并定。

1. **方法表与 UNIT-unsupported-set**：
   - 方法表在运行时登记：执行者用 `routes.add::<M>(port)` 登记自己的方法。`M::BUILDS` 不含当前构建时不登记。网关自己处理的方法（`ping`、`stream/*`、`history/page`、`blob/*`、`policy/describe`）总是登记。
   - T033 只登记 M1a 的会话方法和索引方法：
     - 会话端口：`session/create`、`send`、`interrupt`、`queue_resume`、`confirm_config`、`get`、`capabilities`，以及 `interaction/respond`、`interaction/get`；
     - 索引端口：`project/add`、`project/trust`、`project/trust_info`、`session/mark_read`，以及 `client/presence`。
   - M1b 的方法由实现它们的任务登记，这样 `unsupported` 不会被藏在端口里面。
   - **待裁定 1**：UNIT-unsupported-set 的允许集合。表里每项写明原因：
     - 里程碑：M1b、M2（`diag/export`）、M3（`stream/ack`）；
     - 构建门控：`test/*`、`diag/recording_export`；
     - `host/prepare_upgrade`：T183 之前；
     - 执行者还没合并的 M1a 方法，写明由哪个任务接上：
       - `host/status`、`host/exit`：T037；
       - `settings/*`：T036；
       - `auth/*`：T038；
       - `agents/capabilities`、`agents/refresh`：T055；
       - `test/*`、`diag/recording_export`（相应构建里）：T047。

     每个任务接上执行者时，从表里删掉自己那几行。T099 核对时，表里不应再有 M1a 的方法。这样理解"按里程碑允许的集合"是否可以？
2. **端口接口**：
   - `Port::deliver(Call) -> BoxFuture<Outcome>`：读循环同步调用它，端口只排队（会话端口投进会话 actor 的邮箱）；网关另起 task 等回复，再把结果写进发送队列。这样，同一连接发往同一会话的命令按到达顺序进入邮箱，不同会话之间互不阻塞。
   - "另起 task 执行"的执行者（设置、git 等），在 `deliver` 里直接返回执行用的 future。
   - 命令的 `Ticket` 随 `Call` 交给执行者。
3. **`Facts`**（**待裁定 2**）：`SessionPort::facts(&Confirmable)` 由会话一侧（T043）提供。`mode_tier` 和 `option` 本来就在会话一侧；`running_turns` 也由 SessionManager 给出，不另设接口。
4. **`clients`**：`IndexPort::opened(conn, principal, client, weak_auth)` 在 `initialize` 成功后调用，`closed(conn)` 在断开时调用；`ClientInfo` 由 T034 组装。T038 撤销这条连接上的提权，也挂在 `closed` 上（IndexActor 本来就收提权请求）。
5. **在途限制**（**待裁定 3**）：
   - 所有请求都计数，包括 `ping`；客户端通知不计。
   - 第 65 个请求回 `rate_limited`，不带 `retry_after_ms`：在途许可随这条连接自己的回复释放，与时间无关。5.3.8 写的是 `rate_limited` "带 `retry_after_ms`"，是否要给一个固定值？
6. **写 scope 的用例**：`peer` 认证只有 `read`。需要 `write`、`approve` 的用例，在 crate 内构造 principal，直接驱动分发，不经套接字。走套接字的用例用 `peer`。
7. **`not_found`**（T033b，**待裁定 4**）：订阅和 `history/page` 遇到会话流的 head 为 0 时回 `not_found`，不查 `sessions` 表。依据是会话创建时，第一批事件与受理在同一事务里落盘，存在的会话 head 至少是 1。
8. **补发遇到裁剪**（T033b，**待裁定 5**）：
   - 订阅在流锁里判定 `replay` 之后，写循环读日志时发现开头有缺口，说明补发与裁剪发生了竞态。这时：
     - 摘掉这个订阅；
     - 丢掉这条流已排队的实时项；
     - 发 `stream/reset`。
   - 客户端带游标重新订阅，会得到 `cursor_expired` 快照。
   - `reason` 打算沿用 `rebuilt`，不新增取值（开放枚举，客户端对任何 `reason` 的处理都一样）。是否改为新增一个取值？
9. **Q16 的订阅日志**（T033b，**待裁定 6**）：宿主还没有 tracing（T028 未合并）。打算把它作为网关记录 `stream.subscribed {stream, mode, reason, lag}` 发进现有的审计 channel，由 T037 只写进 host.log，不写 `audit` 表。
10. **发送时合并**（T033b）：
    - `coalesce_ms > 0` 时，写循环取到实时项后再等一个窗口；
    - 只有协商了 `stream.coalesce`，才把 delta 合成带 `from` 的区间事件；
    - 不论是否合并，同一流相邻的实时事件都装进同一条 `stream/events`。
11. **与 T039 的冲突面**：
    - `Gateway::new` 多一个参数；
    - `conn.rs` 改分发入口、`initialize` 之后的一行、断开清理；
    - `HOST_FEATURES` 一行（在 `handshake.rs`）；
    - `testkit.rs` 的 `TestGateway::start`。

    都是附加，合并时由编排者处理。

## 拆分之后（计划 v1.3.7）

编排者采用方案 A，T033 只做 D 部分（分发、账本与路由）；订阅与历史归 T019，blob 归 T029。待裁定的 6 项全部认可，写进了 `03-protocol-v1.md` 5.3.11 末尾"T033 开工前的裁定"，其中第 1–3 项归本任务。开工时 main 已经带来 T039（密钥认证、握手期限、网关夹具）。

### 实际规模

按 `git diff --numstat` 统计，不含本笔记和 CI 摘要。

| 提交 | 内容 | 增 | 删 |
|---|---|---|---|
| `feat(host): dispatch requests …` | `dispatch.rs`、`routes.rs`、`describe.rs`；`conn.rs`、`mod.rs`、`testkit.rs`、夹具、T039 用例的接线 | 558 | 50 |
| `fix(host): answer gateway methods not served yet …` | 留位的方法在参数检查之前就回 `unsupported` | 14 | 2 |
| `test(host): IT-command-order, …` | `dispatch/tests.rs` | 783 | 0 |
| 合计 | | **1,355** | 52 |

实现约 570 行，比估计的 535 多一点。测试 783 行，比估计的 630 多，多在替身与驱动分发的辅助上；仍在上限之内。

### 实现要点

- **流水线**（`dispatch.rs`）：
  - 读循环对每条请求同步调用 `Dispatch::request`，顺序是：
    1. 查方法表，没有登记就回 `unsupported`；
    2. `policy::method`；
    3. 按方法的参数类型校验，不通过就回 invalid params；
    4. 三条只看参数的规则；
    5. 在途许可；
    6. 命令过 `Ledger::begin`；
    7. 投递。
  - `Answer::Later` 的回复在单独的 task 里等待。先释放在途许可，再写回复，这样客户端读到回复就能发下一条请求。
  - 缺省的 `params` 一律按 `{}` 处理。
- **在途**：每条连接一个 64 许可的信号量，所有请求都计数，包括 `ping` 和同步回答的请求；客户端通知不计。第 65 个请求回 `rate_limited`，不带 `retry_after_ms`。
- **客户端通知**：同样经过查表、策略和参数校验；任何一步不通过就静默丢弃。投递返回的 future 另起 task 执行，结果丢弃。
- **方法表**（`routes.rs`）：
  - `add::<M>`、`notification::<N>` 记下 `SCOPE`、`IDEMPOTENCY`、参数校验函数和目标，按 `M::BUILDS` 只在相应构建里登记。
  - 命令必须登记在带账本的表上，否则组装时断言失败。同一方法重复登记也断言失败。
  - `Routes::default()` 只有网关自处理的方法（`ping` 和留位），没有账本，供网关夹具和 `TestGateway::start` 使用。
- **断开**：
  - `initialize` 成功后调用 `IndexPort::opened`；连接结束时，在写 `conn.closed` 审计之前调用 `IndexPort::closed`。
  - 每条连接的 `Dispatch` 随连接一起丢弃。
- **用例编号**：网关层的策略和账本用例没有新编号，分别挂在已有的 UNIT-policy-matrix、UNIT-ledger 下（`unit_policy_matrix_gateway_*`、`unit_ledger_gateway_*`）。走真实套接字的那条挂在 IT-command-order 下。

## 给 T019、T029 的接入点

- **方法表里的留位**：
  - `Routes::default()` 把 `stream/subscribe`、`stream/unsubscribe`、`history/page` 登记为 `Target::Streams`，把 `blob/get`、`blob/put` 登记为 `Target::Blobs`。
  - `Target::serves()` 对这两种返回 false，所以它们现在和没有登记的方法一样，在策略之前就回 `unsupported`。`Dispatch::answer` 里对应的分支是 `unreachable!`。
  - T019、T029 各自把留位换成带依赖的目标，例如 `Target::Streams { streams, readers }`，由 `Routes` 上的一个登记函数（如 `routes.streams(streams, readers)`）放进去。然后在 `Dispatch::answer` 里处理，并删掉 `serves()` 里的对应项和 UNIT-unsupported-set 允许表里的对应行。
  - 这几个方法的幂等都是"天然"，不经过账本。
- **每连接的状态**：
  - 订阅表、上传缓冲放进 `Dispatch`。它在 `initialize` 成功时创建，装在 `Phase::Ready(session, Box<Dispatch>)` 里，在 `conn::run` 末尾随连接丢弃。
  - 订阅和上传缓冲放在这里，断开时就自动注销、丢弃，不用另写清理。
- **回复与帧的先后**：
  - `Answer::Later(reply, place)` 的回复，要等 future 完成后才写进发送队列。
  - `blob/get` 可以在 future 里先把分片帧推进 `Dispatch` 持有的 `Outbox`，完成后网关再写结果。
  - `stream/subscribe` 要在 `start` 回调里排响应，不能走 `Answer::Later` 的回复路径，要在 `Dispatch` 里另开一条分支：直接写 `Outbox`，由同步分支持有在途许可。
- **`0x02` 帧**：`conn.rs` 的 `on_frame` 在 `initialize` 之后忽略非 JSON 帧（注释写明归 T029）。T029 在这里把帧交给 `Dispatch` 的上传缓冲。分片本身不经过方法表，`write` scope 要自己用 `policy::method(principal, BlobPut::NAME, BlobPut::SCOPE.as_ref())` 检查。
- **特性**：`handshake.rs` 的 `HOST_FEATURES` 仍为空。T019 加 `stream.coalesce`、`history.page`，T029 加 `blob.get`、`blob.put`。
- **发送时合并**：`coalesce_ms` 和协商出的特性在 `Phase::Ready` 的 `Session` 里。写任务（`conn.rs` 的 `write_loop`）在 `initialize` 之前就已启动，T019 要把这两个值传给它。
- **组装入口**：`Routes::start` 目前装载账本、启动清理任务、登记两个端口。T019、T029 在这里加上 `Streams`、`Readers`、`BlobStore`，或者另给登记函数，由 T037 调用。

## 给其他任务的接入点（汇报时一并交接）

- **T043**：
  - 实现 `SessionPort`：`deliver` 按 `params.session_id` 投进会话 actor 的邮箱，不等待；`Call.ticket` 是命令的首次执行；返回的 future 给出回复。
  - 实现 `facts(&Confirmable)`：给出 `mode_tier`、`option` 和 `running_turns`。
  - `Routes::sessions` 已登记 `session/create`、`send`、`interrupt`、`queue_resume`、`get`、`capabilities` 和 `interaction/respond`、`interaction/get`。
- **T044**：`routes.add::<SessionConfirmConfig>(sessions)`，并删掉允许表里那一行。
- **T034**：
  - 实现 `IndexPort`：`project/add`、`trust`、`trust_info`、`session/mark_read`，以及 `client/presence` 通知。
  - `opened` 和 `closed` 维护 `clients`。`ClientInfo.auth` 由 `Opened` 的 principal 种类和 `weak_auth` 推出。
- **T036**：`routes.add::<SettingsGet>`、`routes.add::<SettingsUpdate>`。`settings_update` 规则网关已经执行过。
- **T037**：
  - 组装：`Routes::start(&conn, writer, clock, &durations, sessions, index)` 返回 `(routes, sweeper)`，然后 `Gateway::new(config, clock, durations, routes)`；有序退出时 abort 清理任务。
  - 登记 `host/status`、`host/exit`。
- **T038**：
  - 撤销提权挂在 `IndexPort::closed` 上。
  - `Dispatch` 里的 principal 目前在 `initialize` 时定下、不再变化。授予提权时，要让这条连接的 scope 能更新，例如改成共享的可变 principal 或 scope 叠加。
  - 登记 `auth/*`。
- **T047**：
  - 登记 `test/*`、`diag/recording_export`。`add::<M>` 会按构建门控。
  - `test/run_maintenance` 要调用 `Ledger::sweep()`，届时给 `Routes` 加一个公开的账本访问函数（现在是 `pub(super)`），或者由组装时把账本交给执行者。
  - 账本清理任务、各连接等回复的 task 还没有登记进静止屏障。
- **T055**：登记 `agents/capabilities`、`agents/refresh`。
- **T183**：登记 `host/prepare_upgrade`。
