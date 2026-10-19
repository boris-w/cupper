# T050 汇报

- 执行者：子 agent（Rust 工程师，agent 驱动）
- 分支：`feat/T050-driver-transport`
- 完成日期：2026-10-19

**结论：** 验收标准里只有一项没过：`cargo xtask ci pr` 的 license 作业失败。原因是 SDK 引入的 `foldhash` 用的是 Zlib 许可证，`deny.toml` 的白名单里没有它。按规定新增许可证要经人工评审，所以我没改 `deny.toml`，需要你决定。其余都通过：rust 作业 292 项，ts 作业 64 项，四个新用例全部 pass。

## 做了什么

代码都在 `crates/cupper-host/src/driver/`，另外 `src/lib.rs` 加了一行 `pub mod driver;`。

- **`transport.rs`：`AcpTransport`**
  - 接口提供两个端：读端是适配器的 stdout，写端是 stdin。
  - `StdioTransport::new(stdin, stdout)` 直接接收 `cupper_sys::Child` 的两个管道 fd（`OwnedFd`），所以不依赖 `cupper-sys`。
  - `MemoryTransport::pair()` 只在测试和 test-harness 构建里编译。
- **`tap/`：线路分接**
  - **接法**：直接挂在 SDK 公开的 `Lines` 组件上，在 SDK 自己的收发 actor 里逐行处理，不用额外的转发 task。
  - **编号与原始帧**：
    - 两个方向共用一个实例内的 `seq`，从 1 开始。
    - `RawFrame` 的字段和 `raw_frames` 表一一对应。
    - `RawSink` 是有界队列，容量 4,096，满了就丢弃并计数，不阻塞读路径。
  - **非 JSON 行**：只记成 `recv_nonjson`，不交给 SDK，否则 SDK 会回给适配器一个 parse error。
  - **结构化脱敏（`redact.rs`）**，只作用于原始帧，SDK 拿到的仍是原行：
    - `_auth/status_update` 只保留 `kind`、`label`、`account.plan`；
    - 已知格式的凭据在原文上就地遮盖：`sk-`、`ghp_` 等前缀的密钥、JWT、`Bearer`、URL 里的 userinfo 和查询参数。其余字节不动。
  - **K19 改写（`k19.rs`）**：
    - 范围是 `stopReason`、选项 `kind`、工具调用 `status`；
    - 改写成 `end_turn`、`reject_once`、`in_progress`，原值放进 `_meta.cupper.original.<字段名>`，用 `k19::original()` 读回；
    - 原始帧记录改写前的原文；
    - 只有含关键字的行才做 JSON 解析。
- **`acp.rs`：ACP 连接**
  - `AcpConnection::open(transport, tap, mailbox)` 建立连接并发送固定的 `initialize`：
    - 声明 form 类 elicitation、`_meta.terminal_output`、unstable 的 `plan`；
    - `fs` 和 `terminal` 声明为不支持；
    - `clientInfo.name` 为 `cupper`。
  - 回调只做 channel 发送：消息以 `FromAgent` 投进调用方的邮箱，连接结束时最后一条是 `Closed`。
  - 扩展消息全部走 SDK：
    - `_session/steering` 用派生宏声明成请求类型；
    - `_auth/status_update` 由类型化的通知处理器接收，只读 `kind` 和套餐名；
    - `$/cancel_request` 由 SDK 把 `Responder::cancellation()` 标为已取消。
  - **K27**：处理器链最后注册无类型的 `on_receive_dispatch`。未知请求回 method not found，未知通知丢弃，响应照常转发。
  - **K28**：不对外暴露 `SentRequest`。`request()` 发出时就用 `on_receiving_result` 把回调交给 SDK，宿主侧没有可以被丢弃的请求或 future。

## 关键取舍

- **K28 没有字面调用 `detach()`。** `block_task()` 会消耗 `SentRequest`，等待的过程中已经没有可以 `detach` 的对象。所以改为发出时就把回调交给 SDK，"放弃等待"就是不理会之后到来的回复。
  - 回调是有序的：SDK 等回调返回后才派发下一条消息。
  - 邮箱类型由调用方定（`M: From<FromAgent>`）。T055 把回复投进同一个邮箱时，prompt 的响应一定排在它之前的 `session/update` 后面，这是 K12 需要的顺序。
- **K19 判断"未知"交给 SDK**：用 SDK 的枚举试着反序列化，失败才改写，不维护取值表，升级 SDK 时自动跟上。工具调用 `status` 在 SDK 里遇到未知值不会报错，但会悄悄变成默认值，所以照样改写，保住原值。
- **原值按字段名放**：放在 `_meta.cupper.original.<字段>` 下。设计只写了"放进 `_meta.cupper.original`"，按字段名放更明确。T053 要按这个结构读。
- **凭据只在词首匹配**：base64 图片里出现 `sk-` 或 `eyJ` 不会被误伤，有测试覆盖。
- **两个"金丝雀"用例**：直接锁定 SDK 3.3.0 在 K27、K28 上的原始行为。以后升级 SDK，哪个坑被修掉，对应用例就会失败，提醒重新评估规避措施。
- **依赖**：
  - SDK `=3.3.0`，开了 `unstable_plan_operations`（`plan` 能力和 `plan_update` 都在这个特性后面）；
  - `agent-client-protocol-derive` 也锁 `=3.3.0`，不锁的话会解析到 3.4.0；
  - 新增 `futures`；tokio 加了 `io-util`、`net` 特性，测试另开 `macros`；
  - 没有提到 `[workspace.dependencies]`。

## 验证方式

- 在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T050-driver-transport/ci/`：
  - rust 通过，292 项；ts 通过，64 项；
  - license 失败，原因见遗留问题第 1 条；
  - 摘要已提交：`docs/tasks/T050-driver-transport/ci/pr-09d12823db32.json`；
  - 这一轮没有碰到 `cupper-sys` 那个已知的偶发失败。
- 逐条对照验收标准：
  - **UNIT-tap**：通过。覆盖编号；脱敏（含账号字段和凭据）；队列满时丢弃并计数；K19 改写后经 SDK 解析，`k19::original` 读到原值，原始帧仍是原文；已知值逐字节不变。
  - **UNIT-transport-memory**：通过。覆盖双向收发；关闭写端后对端读到 EOF；对端断开后读到 EOF、写入报 BrokenPipe。
  - **UNIT-acp-unhandled**：通过。沿用 T003 的写法：带 sessionId 和不带的未知请求、声明为不支持的 `fs/read_text_file`，都回 -32601；未知通知没有进邮箱。另有一个用例确认不加兜底时，SDK 不回带 sessionId 的请求。
  - **UNIT-acp-detach**：通过。放弃等待后发 `session/cancel` 和 `session/close`，对端没有收到 `$/cancel_request`。另有一个用例确认直接丢弃 SDK 的 future 会发出 `$/cancel_request`。
  - **测试名**：都以用例编号的 snake_case 开头。另有两个用例不带编号：`acp_initialize_...` 和 `acp_extension_messages_...`。
- 其他核对：
  - 变异核对：临时去掉 K27 兜底，UNIT-acp-unhandled 立即失败；
  - 驱动层的测试连跑 30 次，没有偶发失败；
  - 没有运行真实的 CLI 或适配器，测试对端是内存双工上的最小替身；
  - 提交内容里没有本机路径和个人信息。
- 规模：约 +1,490 行（代码加测试，不含 lock），刚好在约 1,500 行的上限以内。其中测试 584 行。

## 提交列表

```
2149a40 docs(task): record the T050 pr CI summary
09d1282 docs(task): record the T050 working notes
a6cbd7d test(host): interrupt before closing in the UNIT-acp-detach flow
075e2d0 feat(host): add the ACP connection task on the SDK
8df504d feat(host): rewrite unknown values of the SDK's closed enums in the tap
c39b562 feat(host): add the wire tap with raw frames and structured redaction
5880764 feat(host): add AcpTransport with stdio and in-memory implementations
```

## 遗留问题 / 范围外发现

1. **license 作业失败（需要你决定）**：
   - `cargo deny` 拒绝 `foldhash 0.2.0`，许可证是 Zlib。
   - 来源：ACP SDK 和它的 schema crate 打开了 serde_json 的 `preserve_order`，这个特性引入 `foldhash`。特性是 SDK 自己打开的，绕不开。
   - Zlib 经 OSI 认证，也是 FSF 认定的自由许可证，和 T001 review 加进白名单的 Unicode-3.0 属于同一类宽松许可证。
   - 两种做法：在 `deny.toml` 的 allow 里加 `"Zlib"`，或者只给 `foldhash` 开一个单 crate 例外。改完在 main 上重跑 CI 即可。
2. **serde_json 的特性在工作区里统一**：`preserve_order` 对整个工作区生效，`cupper-protocol` 在工作区构建时也会拿到它。目前全部测试通过，`CLAUDE.md` 也早就要求导出物不依赖 map 键顺序。
3. **`initialize` 声明的能力和 SP-1 不一致**：
   - 设计 5.6.1 只列了 elicitation、`terminal_output` 和 `plan`。
   - SP-1 的客户端还声明了 `session.{configOptions.boolean, notices, compaction}`，以及 `auth.terminal: true`。
   - 5.6.2 有 `Compaction` 事件。不声明 `session.compaction` 时适配器还会不会推送，需要 T053 或 T055 用录制或真实 CLI 确认。
4. **给后续任务的交接**（详见 `notes.md`）：
   - **T053**：用 `tap::k19::original` 读原值。
   - **T055**：
     - 建议把回复投进同一个邮箱，保持线上顺序；
     - drop `AcpConnection` 就关闭适配器的 stdin（关闭顺序的第 2 步）；
     - `RawSink::bounded()` 只建一个，各实例共用；
     - `steering.supported` 从 `open` 返回的 `InitializeResponse` 顶层 `_meta` 读取。
   - **T028**：可以把 `tap/redact.rs` 里的凭据规则挪到 `diag`，避免两份规则各自演变。
   - **T025、T047**：`stderr` 和 `note` 两种方向由写它们的任务在 `Dir` 里补上。


## 第 1 轮审阅后的补改（2026-10-19）

按第 1 轮审阅的裁定，我在 `deny.toml` 的 allow 里加了 `"Zlib"`，并附注释说明它来自 SDK 打开 serde_json 的 `preserve_order` 后引入的 `foldhash`（提交 `fecf181`，同一提交删掉了旧摘要）；在干净的 worktree 里重跑 `cargo xtask ci pr`，rust、ts、license 三个作业都通过，现在只剩最终摘要 `docs/tasks/T050-driver-transport/ci/pr-fecf18126ede.json`（提交 `4fb3e57`）。
