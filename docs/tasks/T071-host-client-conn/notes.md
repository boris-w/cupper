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

## 编排者裁定（2026-10-19）

- 采用方案 A：不拆，上限约 1,850 行，硬停止线 1,900 行。预计超过 1,900 行时再停下来。
- 上面列出的三处接口，以及握手放在 host-client、认证由调用方注入、三种计数的定义，全部认可，作为约定。用 T039 的测试向量交叉核对证明，改归 T073。
- 范围外：方法目录（方法名 → scope、幂等）由 T076 扩展生成器得到；"生产构建只校验非 delta 事件"留给 T080。

## 实际行数

每完成一个模块统计一次（`git diff --numstat` 相对分支起点，不含 `pnpm-lock.yaml` 和 `docs/`）。

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计（代码 + 测试） | 累计新增 |
|---|---|---|---|---|
| shell-api 的协议部分（`2345741`） | 159 / 6 | 29 | ~155 | 159 |
| 包骨架、ESLint 规则、`frame.ts`（`4c83658`） | 298 | 91 | ~215 | 457 |
| 帧级 mock 宿主、读 fixtures 的测试辅助（`08eb25b`） | 269 | 9 | ~150 | 726 |
| `handshake.ts` 与握手用例（`3ad2cb3`） | 326 | 159 | ~200 | **1,052** |

偏差：这四个模块估计约 720 行，实际 1,052 行，多 46%。

- mock 宿主多了 73%：字节级的连接对象（客户端开始监听前的积压、关闭通知），"不经检查的 HostPort"适配，以及和真宿主一致的握手规则（nonce 核对、失败或重复的 `initialize` 之后断开）。
- 握手多了 59%：表驱动的失败用例被 Prettier 展开成每行一个多行数组（约 45 行），另有一个按脚本发帧的端口辅助和中止路径。

### 第二次超限预警（2026-10-19，四个模块完成后）

剩下的模块按已完成部分的偏差重估：

| 模块 | 代码 | 测试 |
|---|---|---|
| `client.ts`：状态机、请求关联、命令重发、路由、ack、UUIDv7、订阅接口 | ~240 | ~160 |
| `sync.ts` | ~160 | ~220 |
| `shell.ts`：`connectShell` | ~30 | ~20 |
| `mock/port.ts`：浏览器 mock 的 HostPort 与 `ShellAPI.host` | ~120 | — |
| `contract.ts` 与它在 mock 上的调用 | — | ~150 |
| 两个 `index.ts` 的补充 | ~20 | — |
| 合计 | ~570 | ~550 |

**预计总计约 2,170 行（已完成 1,052 + 剩余约 1,120），超出 1,900 行的硬停止线约 14%**。即使每个剩余模块都压到最省（约 1,000 行），也有约 2,050 行。按裁定停下来汇报。已完成的四个模块都已提交，构建、lint、测试都通过；剩下的模块还没有开始写。

方案：

- **方案 A：上限提到约 2,200 行，硬停止线约 2,300 行。** 一起做完，与第一次裁定的理由一致。
- **方案 B：同步部分拆成 T071b。** 内容是 `sync.ts`、`HostClient.subscribe` 及其在断线和重连时的处理、UNIT-hc-sync，约 470 行，与 T075 并行，在 T072 之前完成即可。T071 做完其余部分约 1,700 行。T075 用不到同步部分。代价同第一次汇报：`client.ts` 要在 T071b 里再改一次，多一轮派发和审阅。
- 只把 mock HostPort 和合同套件移给 T076（约 270 行）不够：T071 仍有约 1,900 行。

## 第二次裁定后（2026-10-19，计划 v1.3.9）

- 采用方案 B：同步部分（`sync.ts`、`HostClient.subscribe`、断线重连后的重新订阅、UNIT-hc-sync）移到新任务 T087。T071 收窄为连接、切帧、握手、请求关联、命令重发、ShellAPI 的协议部分、mock 宿主、mock HostPort 和合同套件，上限约 1,750 行。
- `testing.ts` 用 `node:fs`、握手的超时由调用方经 `AbortSignal` 传入，两条都认可。

接着 1,052 行往下统计（相对分支起点的净新增）：

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 累计 |
|---|---|---|---|
| 握手失败表压成一行一例（`6ba4622`） | 17 / 43 | 17 / 43 | 1,026 |
| 浏览器 mock 的 HostPort、合同套件及其在 mock 上的运行（`a4e7ff9`） | 328 | 169 | 1,354 |
| `client.ts`、`connectShell`、UNIT-hc-conn 的连接用例（`3677194`） | 504 | 149 | **1,858** |

- 合计 1,858 行：代码 1,278 行，测试 580 行（含合同套件 155 行、读 fixtures 的辅助 9 行）。比约 1,750 行的上限多约 6%，低于第一次裁定的 1,900 行硬停止线。
- 超出来自 `client.ts`：估计约 200 行，实际 309 行。多出的是两条顺序规则：监听方在处理途中发请求、回复又立即到达时，字节块仍要按到达顺序逐块处理，并且每块处理完才确认；`call` 的回调必须同步执行，回复才能和通知保持原来的先后。另一部分是 Prettier 把 import 和对象字面量展开成多行。
- 写 `client.ts` 的用例之前已经是 1,748 行。剩下的只有 UNIT-hc-conn 必需的连接用例，于是写完再统一说明。删掉了没有使用方、也没有测试的 `notify` 和 `sendBlob`（13 行），需要时由 T080、T145 加回 `client.ts`。

## 实现时的取舍

- **包的边界**：整个 host-client 不 import `node:*` 和 `electron`，ESLint 规则强制（测试和 `testing.ts` 除外）。主进程用同一份代码：把套接字包成 `HostPort`，`handshake()` 之后 `client.attach(port, handshake, rest)`。
- **shell-api 不依赖 host-client**：浏览器 mock 的 `host` 由调用方注入（`createBrowserShell({ host: createMockShellHost(mockHost) })`），否则两个包会互相引用。
- **`Handshake`** 是 `InitializeResult` 去掉 `host_proof`：`host_proof` 由主进程校验，渲染进程用不到。合同测试检查结果里没有这个字段。
- **握手**：
  - 认证用的是哪种方式，决定要不要校验 `host_proof`：`dev_key`、`harness` 两种基于密钥的认证都要求有 `host_proof`，并且必须通过校验。不按 `weak_auth` 判断，因为只开 `test-harness` 的构建里 `weak_auth` 为 false，却仍用 `harness` 认证。
  - 宿主回 `shutting_down` 时记为 `closed`（可重试）。
  - 宿主回 `version_unsupported`、`unauthenticated` 或 `forbidden` 时，原样作为失败原因。
  - 其他错误记为 `protocol_error`。
  - 逐帧读取，`initialize` 响应之后的字节留给下一个读取方。
- **状态机**由驱动方调用 `progress`、`attach`、`detach`、`fail`、`close` 推进。第一次 `ready` 之后，重新拨号和重新握手都算 `reconnecting`，界面只显示一种"正在重新连接"。状态没有变化时不重复通知；`ready` 每次都通知，因为它带着新的握手结果。
- **断线时的请求**：在途的普通请求以 `ConnectionLost` 失败（TanStack Query 自己重试）；命令留下，下次 `attach` 时按原顺序、用同一个 `command_id` 先发。没有连接时发出的普通请求立即失败，命令排队等待。只有 `close()` 让命令失败。
- **区分命令**：生成的协议包里没有方法目录，所以由调用方用 `command()` 标明。`command_id` 已有就沿用，否则生成 UUIDv7。
- **坏帧**（长度为 0、超限、`0x02` 头部截断）：关闭端口，状态记为 `failed: protocol_error`。不是 JSON-RPC 的消息、没有人等的响应，只记日志后跳过。
- **mock 宿主**和真宿主一致：先发 `host/hello`；核对 nonce；`initialize` 失败或重复时先回错误、再断开；没握手就请求的回 `not_initialized`；没有 fixture 也没有处理函数的方法回 `-32601`。
- **浏览器 mock 的 HostPort** 像 MessagePort 一样稍后、按顺序投递，第一个监听方出现之前的数据先存着。状态和数据走同一个微任务队列，所以"旧数据 → `reconnecting` → `ready`"的顺序自然成立。合同套件做过变异检查：放行 `initialize`，或者让 `reconnecting` 抢在旧数据前面，对应的用例都会失败。

## 给 T087 的接入点

T087 在 `HostClient` 之上做订阅，不需要改 `client.ts` 的现有逻辑，只加 `subscribe(stream, sink)`：

- **`stream/*` 通知**：`client.onNotification((method, params) => …)` 按到达顺序收到全部通知，`stream/events`、`stream/snapshot`、`stream/reset` 也在其中，按 `params.stream` 分给各流。`params` 是原样的 JSON；按交接事项，`events`、`ops` 先当 `unknown[]` 取出，再逐个 `zEvent`/`zEventBody.safeParse`。
- **`stream/subscribe` 的结果**：用 `client.call("stream/subscribe", params, done)`，不要用 `request()`。`done` 在处理该帧时同步调用，排在同一个字节块里后面那些通知之前；`request()` 的 Promise 要到微任务里才完成，那时后面的 `stream/events` 已经分发出去，会被当成旧订阅的数据丢掉。`done` 收到三种结果：`{result}`、`{error}`、`{lost: true}`。
- **断线**：`client.onState(cb)` 收到 `reconnecting`（或 `failed`、`closed`）时，各流进入"等待"并保留游标；在途的订阅请求同时以 `{lost: true}` 结束，忽略即可。
- **重连**：收到 `ready` 时第二个参数是新的握手结果，其中有 `data_epoch`。这时按游标重新订阅：`attach` 先通知 `ready`，再处理端口上的剩余字节、重发排队的命令，所以订阅请求排在命令前面。
- **首次订阅**：`client.state.status === "ready"` 时立即发出；否则等下一个 `ready`。
- **三种计数**放在 T087 的订阅层，按裁定的定义统计；E2E 的 `window.__cupperTest` 从那里导出。
