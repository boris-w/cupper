# T010 汇报

- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T010-protocol-base`
- 完成日期：2026-10-19

**先说规模**：`crates/cupper-protocol` 约 2,480 行（不含 Cargo.lock），其中代码约 1,330 行，测试约 800 行，文档注释和空行约 350 行，明显超过约 1,500 行的上限。

- 原因：开工时按模块估算是 1,600 行左右，低估了 rustfmt 默认风格下的行数和测试的分量。中途没有重新统计，写完才发现。
- 守则要求先停下来汇报，这次没有做到，是我的失误。
- 提交是按模块切开的。如果要拆开审阅，可以按提交分批看，也可以把"规范 JSON + schema"作为第二批。

## 做了什么

新建 `crates/cupper-protocol`，库部分不做 I/O，能编译到 iOS 模拟器。`lib.rs` 只有 `mod` 声明，T011–T014 新增各自的文件即可，不需要改这里的模块。

- **从 SP-4a 迁移的部分**（对照设计 5.2.15）
  - `frame`：原样沿用，帧上限改为取自 `limits::V1`。
  - `rpc`：`Method` 新增 `SCOPE` 和 `IDEMPOTENCY` 两个常量，都没有默认值，漏写就编译不过。通知按方向拆成 `HostNotification` 和 `ClientNotification`，后者和请求一样要声明这两个常量（用于 `client/presence`、`stream/ack`）。JSON-RPC 的 id 改名为 `RpcId`。
  - `open_enum`：宏新增 `KNOWN` 列表；另加 `UnknownTagged`，作为带 `type` 标签的联合类型的兜底成员。
  - `text`、`sync`、`coalesce`：`sync` 里会话流的 ID 改用 `SessionId`。`coalesce` 只管信封层的规则，哪些事件能合并由新加的 `Mergeable` trait 决定。
  - `transport`：只保留不做 I/O 的套接字路径计算。路径过长时的回退目录，按设计 5.2.13 改成用户临时目录下的 `cupper-<数据目录哈希>/`。
- **事件信封与流 ID**：`envelope` 里定义 `Event<B>`（对事件体泛型）、`Cause`、`Fallback`、`UnknownEvent`；流 ID 是 `sync::StreamId`。
- **带前缀的 ID**：`ids` 为 5.4.4 的每一种 ID 定义一个类型，含 `CommandId`、`Digest`、`HunkId`、`DiffToken`、`ConnId`。按 UUID 生成的那几类提供 `from_uuid(u128)`。
- **错误码与 reason（5.3.8）**：`error` 补上 `git_error`；全部 reason 是一个开放枚举，`code()` 给出每个 reason 属于哪个错误码。`ErrorData` 把 5.3.8 提到的附加字段写成可选字段，其余放在展开的 `details` 里。
- **限额（5.3.9）**：`limits::V1`，是 `initialize` 结果里要下发的 `Limits`；另有 `MAX_SAFE_INTEGER`。
- **规范 JSON**：`canonical` 按 RFC 8785 实现，`digest()` 返回 `d_` 加完整的 64 位十六进制。
- **六种注解的机制**
  - `x-scope`、`x-idempotent`：由 `schema::method_entry`、`client_notification_entry` 从 trait 常量写出。
  - `x-mergeable`：写在事件变体上，用 `#[schemars(extend("x-mergeable" = true))]`，已测试过对变体有效。
  - `x-id`、`x-time`、`x-path`：跟着类型走。ID 类型自带 `x-id`（principal 是稳定 ID，不带），`TimestampMs`/`DurationMs` 自带 `x-time`，`WirePath` 自带 `x-path`。新字段只要用这些类型，就会被金样例的规范化自动纳入。
- **整数上界**：`schema::generator()` 把所有整数的 `format` 换成不超过 ±(2^53−1) 的上下界；规范 JSON 遇到超出这个范围的整数直接报错。

## 关键取舍

- **`SCOPE` 的类型是 `Option<Scope>`**：`Scope` 会出现在线上（`data.scope`，以后还有 `principal.scopes`），所以做成开放枚举。"不需要 scope"（`initialize`、`ping`、`auth/elevate`）用 `None` 表示，schema 里写 `x-scope: "none"`。`Idempotency` 只用于注解，是封闭枚举 `Command` / `Natural` / `None`，对应方法表里的"命令"、"天然"、"—"。
- **ID 只做宽松校验**：只查前缀和正文（1–64 个 `[0-9A-Za-z_-]`），不查 UUID 的具体格式。宿主能把乱填的 ID 当参数错误拒掉，客户端也仍然把 ID 当作不透明字符串。相邻的前缀不会混淆（`p_` 和 `pr_`、`d_` 和 `dt_`、`h_` 和 `hk_`），有测试覆盖。
- **限额按转义后的字节计**：5.3.9 写的是"字符"，但 32,000 个中文字符放不进 64,000 字节的帧，所以和 SP-4a 一样按字节计，字段名是 `max_*_bytes`。宿主内部的 40 ms 写入合并窗口、提权的冷却规则不放进 `limits`。
- **JCS 的两处坑**
  1. Rust 的 `{:e}` 在两个同样近的最短数字串之间会取进位的那个，ECMAScript 取偶数。现在按同样的位数再做一次"四舍六入五成双"，能读回原值就用它。
  2. serde_json 默认的浮点解析会差 1 个 ULP（RFC 例子里的 `333333333.33333329` 就会算错）。所以 `cupper-protocol` 给 serde_json 打开了 `float_roundtrip`。不打开的话，客户端发来的数字进入 `params_hash` 后，宿主和客户端算出的摘要会对不上。
- **`ErrorData` 的两个字段暂不具名**：`supported` 依赖 T012 的版本区间类型，`resolved_by` 依赖 T011 的类型，先放在 `details` 里。T011、T012 可以以附加的方式改成具名字段。SP-4a 里的 `retryable` 设计中没有，去掉了。
- 其他取舍和依赖版本见 `docs/tasks/T010-protocol-base/notes.md`。

## 验证方式

- **UNIT-digest-jcs**：通过。
  - RFC 8785 的 3.2.2 示例、3.2.3 的排序示例（U+1F600 排在 U+FB33 前面）、附录 B 的全部数字样例，都按 rfc-editor.org 的原文写进了测试。
  - 另有字符串转义测试、完整 256 位摘要的金样例（`{"a":"x","b":1}`）、double 读回原值的性质测试。
  - 另外做了一次本机交叉核对：约 30 万个 double 和 Node 24 的 `JSON.stringify` 逐个比对，0 处不一致。比对程序没有入库。
- **UNIT-proto-base**：通过，覆盖以下各项。
  - 分帧往返：任意切分字节流的性质测试，含 blob 分片。
  - 超过上限的帧被拒：编码和解码两侧都测。
  - 开放枚举和开放联合遇到未知值时落到 `Unknown`，并能原样往返。
  - 错误码往返：所有已知的 code 和 reason，未知的 code、reason 和附加字段也能原样保留。
  - 整数上界：schema 里每个整数都有界、不带 `format`；规范 JSON 拒绝 2^53 及以上的整数。
  - 另外还测了 ID、注解、方法注解、信封、流 ID、合并（性质测试）、套接字路径。
- **iOS 模拟器**：`cargo build -p cupper-protocol --target aarch64-apple-ios-sim` 的 debug 和 release 都通过。
- **本机 CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T010-protocol-base/ci/` 通过。
  - rust 41 项（其中本 crate 26 项）、ts 3 项、license 1 项全部通过，其余作业是"尚未实现"。
  - 摘要是 `ci/pr-aba3d39d7720.json`，逐编号结果里有 UNIT-digest-jcs pass、UNIT-proto-base pass。
- **其他检查**：`RUSTDOCFLAGS="-D warnings" cargo doc -p cupper-protocol` 通过；`cargo deny --locked check` 通过。
- **测试命名**：所有测试函数名都以 `unit_proto_base_` 或 `unit_digest_jcs_` 开头。

## 提交列表

```
0c83ee1 docs(task): record the T010 pr CI summary
aba3d39 docs(task): add T010 working notes
12c39fd docs(protocol): disambiguate the open_enum! doc link
8fd5f0e feat(protocol): compute the host socket path
7eecc59 feat(protocol): bound schema integers and annotate method entries
fdbb39c feat(protocol): add RFC 8785 canonical JSON and d_ digests
d3c42d7 feat(protocol): add the event envelope, stream IDs and send-time coalescing
890a320 feat(protocol): add the JSON-RPC envelope, method traits and error codes
7b462fc feat(protocol): add open enums, prefixed IDs and annotated scalar types
578654b feat(protocol): migrate framing from SP-4a and add the v1 limits
```

## 遗留问题 / 范围外发现

1. **规模超限**：见开头。需要编排者决定是直接接受，还是按提交分批审阅。
2. **根配置的改动**
   - 根 `Cargo.toml` 的 `[workspace.dependencies]` 新增：`proptest = { version = "1.12.0", default-features = false, features = ["std"] }`、`schemars = "1.2.3"`、`serde = { version = "1.0.229", features = ["derive"] }`、`sha2 = "0.10.9"`、`thiserror = "2.0.21"`。
   - crate 自己给 serde_json 打开了 `float_roundtrip`。因为 feature 会统一，整个工作区的 serde_json 浮点解析都会变成精确的。
   - main 合并 T020/T022 后 `Cargo.lock` 也有新增，合并时这个文件预计会有文本冲突，重新生成 lock 即可。
3. **iOS 构建还不在 CI 里**：`xtask ci` 的 `rust` 作业还没有 `cargo build -p cupper-protocol --lib --target aarch64-apple-ios-sim`（设计 5.1.6 列了）。这次是手动跑的；这属于 C 区域，我没有改。
4. **设计文档的写法**：5.3.9 的"字符"建议改成"转义后的字节"（理由见上）。另外 5.2.15 写 `transport.rs` "沿用"，但 5.2.13 已经改了回退目录，实现按 5.2.13。
5. **`fallback.summary` 是界面文字**：ADR-04 的 `fallback{kind, summary}` 带的是界面文字，和"宿主只下发语义码"的原则有冲突。信封照 ADR-04 保留，`summary` 怎么填留给 T011 定。
6. **留给后续任务的事**：`Event<B>` 的 schema 名在泛型下是自动生成的，T015 导出时可能要加 `rename`。`ErrorData.supported`、`resolved_by` 改成具名字段留给 T011/T012。
