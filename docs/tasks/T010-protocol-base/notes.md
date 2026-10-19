# T010 工作笔记

## 依赖

| crate | 版本 | 说明 |
|---|---|---|
| serde（derive） | 1.0.229 | 与工作区已锁定的版本相同 |
| schemars | 1.2.3 | SP-4a 实测版本 |
| sha2 | 0.10.9 | 同上；摘要的十六进制自己写，不引入 `hex` |
| thiserror | 2.0.21 | 同上 |
| proptest（dev） | 1.12.0，`default-features = false, features = ["std"]` | 去掉 `fork`、`timeout`，少拉 rusty-fork、tempfile 等 |

都加在根 `[workspace.dependencies]`。`cupper-protocol` 另外给 serde_json 打开 `float_roundtrip`（见下文"规范 JSON"）。`cargo deny --locked check` 通过。

## 模块划分

`lib.rs` 只放 `mod` 声明。T011–T014 各自新增文件，不需要改这里已有的模块：

| 模块 | 内容 | 来源 |
|---|---|---|
| `frame` | 分帧、blob 分片头 | SP-4a 原样，帧上限改取 `limits::V1` |
| `limits` | 5.3.9 的限额（`initialize` 下发的 `limits`）、`MAX_SAFE_INTEGER` | 新增 |
| `open_enum` | `open_enum!`（新增 `KNOWN`）、带 `type` 标签联合的兜底 `UnknownTagged` | SP-4a |
| `ids` | 5.4.4 的每种 ID 一个类型 | 新增 |
| `annotations` | 六种注解的键、`TimestampMs`、`DurationMs`、`WirePath` | 新增 |
| `rpc` | JSON-RPC 信封、`Method`（加 `SCOPE`、`IDEMPOTENCY`）、通知按方向分开 | SP-4a |
| `error` | 错误码（补 `git_error`）、reason、`ErrorData`、`RpcError` | SP-4a 拆出 |
| `envelope` | 事件信封 `Event<B>`、`Cause`、`Fallback`、`UnknownEvent` | SP-4a `events.rs` 的信封部分 |
| `sync` | 流 ID、连续性检查 | SP-4a |
| `text` | 转义后长度、切分、截断 | SP-4a |
| `coalesce` | 发送时合并；可合并的规则交给 `Mergeable` | SP-4a |
| `canonical` | RFC 8785 与 `d_` 摘要 | 新增 |
| `schema` | 生成器设置（整数上下界）、方法条目的 `x-scope`/`x-idempotent` | SP-4a `schema.rs` 的一部分 |
| `transport` | 套接字路径 | SP-4a，回退目录按设计 5.2.13 改 |

## 关键取舍

- **`Method::SCOPE` 用 `Option<Scope>`**：`Scope` 是线上类型（`data.scope`、以后 `principal.scopes`），按"枚举都是开放的"写成开放枚举；"无 scope"（`initialize`、`ping`、`auth/elevate`）不是一种 scope，用 `None` 表示，schema 里写 `x-scope: "none"`。`Idempotency` 只是注解，是封闭的 `Command`/`Natural`/`None`，对应表里的"命令"、"天然"、"—"。两个常量都没有默认值，漏写就编译不过。
- **通知按方向分开**：`HostNotification`（宿主 → 客户端，没有 scope）和 `ClientNotification`（客户端 → 宿主，和请求一样声明 `SCOPE`、`IDEMPOTENCY`，如 `client/presence`、`stream/ack`）。网关只分发后者。
- **JSON-RPC 的 id 叫 `RpcId`**：`RequestId` 留给交互请求的 `r_`，与字段名 `request_id` 一致。
- **reason 是一个扁平的开放枚举**：5.3.8 里没有两个码共用的 reason，`ErrorReason::code()` 给出它属于哪个码；`RpcError::with_reason` 在 debug 构建里断言两者相符。
- **`ErrorData`**：5.3.8 提到的 `scope`、`capability`、`agent_code`、`agent_message`、`retry_after_ms` 写成可选字段；引用以后才有的类型的 `supported`（T012 的版本区间）、`resolved_by`（T011）先放在展开的 `details` 里，T011/T012 可以改成具名字段（附加，不破坏）。SP-4a 的 `retryable` 设计里没有，去掉了。
- **ID 的校验**：只查前缀和正文（1–64 个 `[0-9A-Za-z_-]`），不查 UUID 的具体格式。宿主能把乱填的 ID 当参数错误拒掉，客户端也不依赖现在的格式（"客户端把 ID 当作不透明字符串"）。`p_` 与 `pr_`、`d_` 与 `dt_`、`h_` 与 `hk_` 互不混淆，有测试。
- **注解跟着类型走**：ID 类型带 `x-id: true`（principal 是稳定 ID，不带）；`TimestampMs`、`DurationMs` 带 `x-time: true`；`WirePath` 带 `x-path: true`。新字段只要用这些类型就自动带上注解，对应 5.9.3"新加的字段按注解自动纳入"。必须保留普通类型的字段可以用 `#[schemars(extend("x-path" = true))]`。`x-mergeable` 写在事件的变体上（schemars 的 `extend` 支持变体，有测试），合并规则由事件体实现 `Mergeable`。`x-id` 用布尔值，占位的前缀由规范化工具从值本身取（`s_`、`t_`……），没有前缀的 `command_id` 由它自己定。
- **事件信封对事件体泛型**：`Event<B>`，T011 定义事件体的开放联合后用 `Event<EventBody>`。`coalesce` 只管信封层的规则（seq 连续、`from`、相同的 `cause` 才保留、版本相同），哪些事件体能合并、按什么键合并（`item.delta` 按 `(item_id, field)`）由 `Mergeable` 决定。
- **限额按字节计**：5.3.9 写的是"16,000 / 32,000 / 16,000 字符（转义后）"，但帧上限是字节，32,000 个中文字符放不进 64,000 字节的帧，所以和 SP-4a 一样按转义后的 UTF-8 字节计，字段名写成 `max_delta_bytes` 等。标题截断仍按字符（`max_label_chars`）。写入时合并的 40 ms 是宿主内部的，不放进 `limits`；提权的"1 个未决、10 分钟内被拒 2 次后静默"是策略规则，留给 T031。
- **套接字路径**：SP-4a 退到 `/tmp/cupper-<uid>/`，设计 5.2.13 改成 `confstr(_CS_DARWIN_USER_TEMP_DIR)` 下的 `cupper-<数据目录哈希>/`。取这个目录要调用系统接口，属于 `cupper-sys`，所以 `socket_path` 把它作为参数传入。

## 规范 JSON（RFC 8785）

- 对象成员按名字的 UTF-16 码元排序（不是 Rust 字符串默认的码点序）；字符串只转义 `"`、`\` 和控制字符，`\u00xx` 用小写；数字按 ECMAScript 的 Number::toString。
- Rust 的 `{:e}` 给出能读回原值的最短数字串，但两个同样近的候选里会取进位的那个（`1424953923781206.25` 给出 `…206.3`），ECMAScript 取偶数（`…206.2`）。做法：按最短串的位数用 `{:.Ne}` 重新舍入（Rust 的定长格式是"四舍六入五成双"），能读回原值就用它。
- **serde_json 默认的浮点解析不是正确舍入的**：`333333333.33333329` 被解析成相邻的另一个 double，RFC 的例子因此算错。打开 `float_roundtrip` 后正确。客户端发来的数字（比如提问的 `number` 答案）会进入 `params_hash`，不打开的话宿主和客户端的摘要会对不上。
- 绝对值超过 2^53−1 的整数直接报错，不参与摘要：协议里不会有，JavaScript 也表示不了。NaN 和无穷大进不了 serde_json 的 `Value`（序列化时已变成 `null`），与 `JSON.stringify` 的行为一致。
- 核对：RFC 8785 附录 B 的数字样例、3.2.2 的例子和 3.2.3 的排序例子都按 rfc-editor.org 上的原文写进测试；另外在本机用一次性程序生成约 30 万个 double（随机位模式、随机小数、随机量级），与 Node 24 的 `JSON.stringify` 逐个比对，0 处不一致（程序没有入库）。

## 规模

`crates/cupper-protocol` 约 2,480 行（不含 Cargo.lock）：代码约 1,330 行，测试约 800 行，文档注释和空行约 350 行。超出了约 1,500 行的上限。

- 原因：一开始按模块估算约 1,600 行，低估了 rustfmt 默认风格下的行数（SP-4a 的代码重新格式化后也变长了）和测试的分量；没有在写完一半时重新统计。
- 守则要求明显超出时先停下汇报，这次是写完才统计出来的，在汇报里说明。
- 提交已经按模块切开（帧与限额、ID 与注解、RPC 与错误、信封与合并、规范 JSON、schema、套接字路径），需要拆开审阅时可以按提交分批看。

## 范围外的发现

- `xtask ci` 的 `rust` 作业还没有 `cargo build -p cupper-protocol --lib --target aarch64-apple-ios-sim`（设计 5.1.6 列了），本任务手动跑过；属于 C 区域，没有改。
- 5.3.9 的"字符"与实现的"转义后字节"不一致，建议设计改写法（见上）。
- ADR-04 的 `fallback{kind, summary}` 带的是界面文字，与"宿主只下发语义码"的原则有张力。信封照 ADR-04 保留，`summary` 怎么填由 T011 定。
- 5.3.4 里 `stream/ack`（客户端 → 宿主的通知）的幂等一栏是"—"，`client/presence` 是"天然"；按表照写即可，`Idempotency::None` 能表达"—"。
