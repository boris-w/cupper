# T071 工作笔记

## 设计草案（开工前）

整个 `@cupper/host-client` 不 import `electron` 和 `node:*`，渲染进程和主进程都能用：渲染进程经 `ShellAPI.host` 拿到 `HostPort`；主进程（T075、T076）把套接字包成同样的 `HostPort` 接口，复用切帧、握手和请求关联。

| 模块 | 内容 |
|---|---|
| `shell-api` 的协议部分 | `HostPort`、`Handshake`（`InitializeResult` 去掉 `host_proof`）、`ConnectionState`、`PrivilegedAction`、`PrivilegedResult`；`ShellAPI` 加 `host.open`、`host.onConnectionState`、`requestPrivileged`。`shell-api` 新增对 `@cupper/protocol` 的类型依赖。浏览器 mock：`host` 由调用方注入（避免 shell-api 反过来依赖 host-client），`requestPrivileged` 由 `controls` 设定结果并记录调用 |
| `frame.ts` | `u32 长度 \| u8 类型 \| 负载`，整帧 ≤ 64,000 字节；增量解码（字节块可以在任意位置切开）；`0x02` 头部（32 字节哈希、8 字节偏移、1 字节标志）的编解码；长度为 0 或超限时报错 |
| `handshake.ts` | 等 `host/hello` → 协商版本 → `initialize` → 校验结果。认证由调用方注入（`auth(hello)` 给出 `Auth`，`verifyHostProof` 校验 `host_proof`）：证明的计算在 T073 的原生扩展里，host-client 不做 HMAC。失败原因用语义码：`version_unsupported`、`unauthenticated`、`forbidden`、`weak_auth`（发布构建拒绝）、`host_unverified`（`host_proof` 缺失或不符）、`protocol_error`、`closed`。返回握手结果和响应之后已经收到的剩余字节 |
| `client.ts`（`HostClient`） | 连接状态机 `connecting → handshaking → ready → reconnecting`，另有 `failed`、`closed`；请求按 `id` 关联；`command()` 生成 UUIDv7 的 `command_id`，断线时保留，重连后原样重发，普通请求在断线时以 `ConnectionLost` 失败；`stream/*` 分给各流的 `StreamSync`，其他通知和 `0x02` 分片交给订阅者；每个字节块处理完立即 `ack` |
| `sync.ts`（`StreamSync`） | 与 Rust `Mirror` 同规则的纯核心，但不持有视图：视图由调用方的 `StreamSink`（T072、T080）持有。快照（不论原因）丢弃视图和游标；缺口带游标重新订阅；`ViewError`（sink 抛错）或信封解码失败不带游标重新订阅；`stream/reset` 保留游标；等订阅结果期间忽略旧订阅的数据。`events`、`ops` 先当 `unknown[]` 取出，再逐个 `zEvent`/`zEventBody.safeParse`；类型已知、数据残缺时记日志并照常交给 sink（显示占位）。三种计数：`gaps`、`duplicates`（seq 不大于游标）、`resets`（宿主的 `stream/reset`，以及客户端丢弃已有视图的重来） |
| `shell.ts` | `connectShell(shell, client)`：`host.open()` 和 `onConnectionState` 接到 `HostClient` |
| `mock/host.ts` | 帧级 TS mock 宿主：连接即发 `host/hello`；请求的默认结果取自 `protocol/fixtures/methods.json`，可按方法覆盖；可推送通知、`0x02` 分片、断开连接；记录收到的帧 |
| `mock/port.ts` | 浏览器 mock 的 `HostPort` 和 `ShellAPI.host`：按 windowBridge 的语义代发 `initialize`（role `window`）、本地拒绝 `initialize` 与批量数组、重新序列化后转发、异步投递字节块、断线后自动重连并推送状态 |
| `contract.ts` | HostPort 合同测试套件（导出给 T076）：转发与重新序列化、本地拒绝以错误响应经 `onData` 返回、`sendBlob` 的帧、`onData` 的字节块各自独立、退订、`close`、重连时"旧连接的数据先于 `reconnecting`、`ready` 之后才是新连接的数据" |

需要编排者确认的几处接口（ShellAPI 在 M1a 冻结）：

- `ConnectionState` 写成 `{status: "connecting" | "handshaking" | "ready" | "reconnecting"} | {status: "failed"; reason}`，`reason` 是开放枚举的语义码，T075 可以加拉起宿主相关的取值（退出码 4–6、宿主校验失败）。`ready` 时第二个参数带握手结果，与设计的签名一致。
- 本地拒绝（主进程或 mock 拒绝出方向的请求）以 JSON-RPC 错误响应经 `onData` 返回，`code: forbidden`；没有 `id` 的消息和批量数组直接丢弃。T076 的 windowBridge 要照此实现，合同测试会检查。
- 重连的顺序要求：旧连接的字节块全部投递之后才推 `reconnecting`，新连接的字节块在推 `ready` 之后才投递。T076 需要让状态和字节块经同一条有序通道到达（例如同一个 MessagePort）。

## 行数估计（开工前）

参照：Rust `mirror.rs` 的核心约 230 行、测试约 420 行（T049）；T070 的 `shell-api.ts` 118 行、浏览器 mock 144 行。计划 1.1 的经验是测试占 40%–55%；此前任务的实际行数一般比估计多 15%–25%。

| 模块 | 代码 | 测试 |
|---|---|---|
| shell-api：协议部分的类型、浏览器 mock 的 `requestPrivileged` 与 `host` 注入、导出 | ~120 | ~35 |
| host-client 包骨架：`package.json`、`tsconfig.json`、`vitest.config.ts`、`index.ts` | ~45 | — |
| `frame.ts`：切帧、`0x02`、上限 | ~100 | ~70 |
| `handshake.ts`：握手与各种失败 | ~110 | ~90 |
| `client.ts`：状态机、请求关联、命令重发、路由、ack | ~250 | ~150 |
| `sync.ts`：游标、`data_epoch`、快照分页、连续性、三种计数、逐个解码 | ~170 | ~230 |
| `shell.ts`：`connectShell` | ~35 | ~20 |
| `mock/host.ts`：帧级 mock 宿主 | ~150 | — |
| `mock/port.ts`：浏览器 mock 的 HostPort | ~120 | ~10 |
| `contract.ts`：合同测试套件（本身是测试代码） | — | ~140 |
| 合计 | **~1,100** | **~745** |

**预计合计约 1,850 行，超出约 1,500 行的上限约 20%**。压缩文档注释、测试改成表驱动之后，估计也在 1,650–1,700 行。按任务书和守则第 8 条，开工前停下来汇报，还没有写代码。

### 方案

- **方案 A：不拆，上限提到约 1,850 行（硬停止线约 1,900 行）。** 计划 2.1 对 T071 的说明是"连接状态机、切帧和续传共用一个连接对象；ShellAPI 的协议部分、mock 宿主和 HostPort 合同测试都围绕同一个 HostPort 语义"，这几块本来就要一起定。超出的部分主要是测试：`sync.ts` 的规则要逐条对照 Rust `Mirror` 的 7 个用例，合同套件要同时约束 mock 和 T076 的真实实现。代价是零松弛链上的 T071 变长。
- **方案 B：同步部分拆成 T071b，与 T075 并行。** T071 保留 shell-api 的协议部分、切帧、握手、`HostClient`（状态机、请求关联、命令重发、`0x02`、ack、按流路由的接口）、`connectShell`、mock 宿主、mock HostPort、合同套件，验收 UNIT-hc-conn、UNIT-hostport-contract-mock，估计约 1,350 行。T071b 做 `sync.ts` 和 `HostClient.subscribe`，验收 UNIT-hc-sync，估计约 450 行，在 T072 之前完成即可。T075 只用到切帧、握手、`HostClient` 和 mock 宿主，不依赖同步部分。代价：与计划里"连接和续传共用一个连接对象"的说法相违，T071b 要回头改 `client.ts`；多一轮派发和审阅。
- **方案 C：测试替身拆成 T071b。** mock 宿主、mock HostPort、合同套件（约 450 行）移出，T071 约 1,400 行。T076 依赖它们。不推荐：T075 测 `controlClient` 也要用帧级 mock 宿主，T071 的握手和请求关联用例也得另写一个假宿主，重复约 40 行。

建议方案 A；如果更看重缩短 T071 本身，选方案 B。
