# T050 工作笔记

## 结构

`crates/cupper-host/src/driver/`：

| 文件 | 内容 |
|---|---|
| `transport.rs` | `AcpTransport`（读端是适配器的 stdout，写端是 stdin）；`StdioTransport` 接 `cupper_sys::Child` 的两个管道 fd；`MemoryTransport` 只在测试和 test-harness 构建里有 |
| `tap/mod.rs` | `Tap`：编号、原始帧（`RawFrame`、`RawSink`，有界 4,096，满了丢弃并计数）、`wire()` 把传输接成 SDK 的 `Lines` 组件 |
| `tap/redact.rs` | 结构化脱敏：`_auth/status_update` 只留 `kind`、`label`、`account.plan`；已知格式的凭据按文本原位遮盖 |
| `tap/k19.rs` | K19 改写，`k19::original()` 供提取层读回原值 |
| `acp.rs` | `AcpConnection`：处理器链、`initialize`、扩展消息类型、K27 兜底、`request` / `notify` |

## 取舍和试错

- **分接层挂在 SDK 的 `Lines` 上**。SDK 3.3.0 公开了 `Lines`（按行的 `Sink<String>` 加 `Stream<io::Result<String>>`），分接层直接在 SDK 自己的收发 actor 里按行处理，不必像 q9 探针那样再垫一对内存管道和两个转发 task。
  - 写端要自己实现 `Sink`：`futures::sink::unfold` 关闭时碰不到底层写端（SDK 自己的 `LineWriter` 注释里也这么说），内存双工就收不到 EOF。`LineSink::poll_close` 调 `poll_shutdown`。
  - 读端按字节读到换行，非法 UTF-8 用替换字符，不让一行坏输出断开连接。
- **非 JSON 行不交给 SDK**。SDK 收到解析不了的行，会回给适配器一个 parse error。首个非空白字符不是 `{` 或 `[` 的行只记成 `recv_nonjson`。
- **只解析含关键字的行**（5.6.10 第 4 条）：`_auth/status_update`、`"stopReason"`、`"session/request_permission"`、`"tool_call`。凭据遮盖不解析 JSON，在原文上扫描，所以其余字节原样保留。
- **凭据规则只在"能开始一个词"的位置匹配**：前一个字节是字母、数字、`_-.+/` 时不匹配（JSON 转义 `\n` 等之后算词首）。这样 base64 图片数据里出现 `sk-`、`eyJ` 也不会误伤，测试里有这个用例。
- **K19 的"已知"由 SDK 判定**：用 SDK 的枚举类型试着反序列化，失败才改写。以后升级 SDK，枚举多了取值，分接层自动跟上，不需要维护一份取值表。工具调用的 `status` 在 SDK 里是 `DefaultOnError`，未知值不会报错，但会被悄悄当成默认值，所以照样改写，保留原值。
- **原值放在 `_meta.cupper.original.<字段名>`**，按字段名分开。设计只写了"放进 `_meta.cupper.original`"，按字段名放更明确，以后同一个对象上要改写第二个字段也不冲突。原始帧记录的是改写前的原文。
- **K28 用 API 形态解决**：`AcpConnection` 不暴露 `SentRequest`，`request()` 发出时就用 `on_receiving_result` 把回调交给 SDK。宿主侧没有可以丢弃的请求对象或 future，"放弃等待"就是不理会之后到来的回复。字面上的 `detach()` 用不上：`block_task()` 会消耗 `SentRequest`，等待中途已经没有可以 `detach` 的对象了。
  - 回调是有序的：SDK 等回调返回后才派发 agent 的下一条消息。所以 T055 把回复也投进同一个邮箱时，prompt 的响应一定排在它之前的 `session/update` 后面（K12 需要的顺序）。邮箱的类型由调用方定（`M: From<FromAgent>`），回复和回调可以进同一个 channel。
- **`initialize` 在连接任务的前台里 `block_task()`**：那是 SDK 文档认可的位置，future 不会被中途丢弃；`open` 的调用方放弃时，只是收不到结果，连接随即结束。
- **测试里的等待**：clippy 禁止 `tokio::time::timeout`，测试用 `SystemClock::timer_at` 加 `select` 写了 5 秒上限的 `soon()`。"没有发出 `$/cancel_request`"用屏障判定：SDK 的发送队列是先进先出的，放弃等待之后再发 `session/cancel` 和 `session/close`，对端读到的下一行必须就是它们。
- **两个"金丝雀"用例**：`unit_acp_unhandled_without_the_last_handler_the_sdk_never_answers` 和 `unit_acp_detach_dropping_an_sdk_request_sends_cancel_request` 直接验证 SDK 3.3.0 的原始行为。以后 SDK 修掉了其中一个坑，对应用例会失败，提醒重新评估规避措施（5.6.10 说升级要重跑 T003 的探针）。
- **变异核对**：临时去掉 K27 兜底处理器，`unit_acp_unhandled_requests_...` 立即失败（with-session 的请求没有回应，先收到的是 without-session 的）。核对后已恢复，没有提交。

## 依赖

- `agent-client-protocol = "=3.3.0"`，开 `unstable_plan_operations`：`plan` 客户端能力和 `plan_update` 都在这个特性后面。不开的话，`plan_update` 解析失败，会被 SDK 记一条日志后丢掉。
- `agent-client-protocol-derive = "=3.3.0"`：SDK 只要求 `^3.3.0`，不锁的话会解析到 3.4.0（T003 的探针就是 3.4.0）。我们直接用它的派生宏，按 ADR-02 和 SDK 锁在同一个版本。
- `futures`（`Sink`、`Stream`、`select`）；tokio 增加 `io-util`、`net`（`unix::pipe`），测试另开 `macros`。
- SDK 打开了 serde_json 的 `preserve_order`。特性在工作区里统一，`cupper-protocol` 在整个工作区构建时也会拿到这个特性。全部测试都通过；`CLAUDE.md` 早就要求导出物不依赖 map 键顺序。

## 阻塞项：许可证

`cargo deny` 拒绝 `foldhash 0.2.0`（Zlib）。依赖链是：SDK 和 SDK 的 schema crate 打开了 serde_json 的 `preserve_order`，这个特性引入 `foldhash`。绕不开：特性是 SDK 自己的 `Cargo.toml` 打开的。Zlib 经 OSI 认证，也是 FSF 认定的自由许可证，属于宽松许可证，性质和 T001 review 加进去的 Unicode-3.0 一样。但 `deny.toml` 规定新许可证要先经人工评审，所以我没有改它，留给编排者决定。可以把 `"Zlib"` 加进 allow，也可以只给 `foldhash` 开一个单 crate 例外。除此之外的 CI 都通过了。

## 行数（代码加测试，不含 lock）

| 提交 | 行数 |
|---|---|
| transport | +153 |
| tap、脱敏 | +616 |
| K19 | +203 −5 |
| ACP 连接 | +522 −3 |
| detach 用例补 `notify` | +5 −3 |
| 合计 | 约 +1,490 |

刚好在约 1,500 行的上限以内。其中测试 584 行，约占四成。

## 留给后续任务

- **T053**：用 `tap::k19::original(meta, field)` 读原值；`AuthStatus` 只反序列化 `kind`、`label`、`account.plan`。
- **T055**：
  - 用 `StdioTransport::new(child.stdin, child.stdout)` 和 `AcpConnection::open(transport, Tap::new(instance_id, clock, raw_sink), mailbox)`；
  - 回复建议投进同一个邮箱，保持线上的顺序；
  - `FromAgent::Closed` 是邮箱里的最后一条；
  - drop `AcpConnection` 就会关闭适配器的 stdin（5.6.8 关闭顺序的第 2 步）；
  - `RawSink::bounded()` 只建一个，各实例共用，读端交给存储写线程；
  - `steering.supported` 从 `open` 返回的 `InitializeResponse.meta` 里读。
- **T025、T047**：`RawFrame` 的字段和 `raw_frames` 表一一对应；`Dir::as_str()` 就是 `dir` 列的取值。`stderr` 和 `note` 两种方向由写它们的任务加。
- **T028**：日志脱敏也要遮盖凭据。可以把 `tap/redact.rs` 的凭据规则挪到 `diag`，分接层改为调用 `diag` 里的版本，避免两份规则各自演变。
