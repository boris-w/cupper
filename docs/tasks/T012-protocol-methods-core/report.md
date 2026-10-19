# T012 汇报

- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T012-protocol-methods-core`
- 完成日期：2026-10-19

**结论**：拆分后 A 范围的工作都做完了，验收标准逐条满足。我在干净的 worktree 里跑了 `cargo xtask ci pr --record`，结果通过，UNIT-proto-methods-core-stream 为 pass。

**规模**：`crates/cupper-protocol` 新增 1,412 行，删除 33 行，在 1,500 行的上限以内。拆分后的估计是 1,155 行。

| 组 | 代码 | 测试 | 小计 | 估计 |
|---|---|---|---|---|
| 0 方法基础 | 约 255 | 约 290 | 约 545 | 300 |
| 1 握手与生命周期 | 308 | 97 | 405 | 415 |
| 2 流与内容 | 306 | 157 | 463 | 440 |

- 超出估计的部分全在第 0 组的跨组测试：`catalog.rs` 的测试约 210 行，原来只估了 60 行。
- **守则第 8 条这次没有做到**：三组全部写完之后才第一次统计行数，当时是 1,426 行，离 notes 里定的 1,450 行停止线很近。之后压缩了测试，最终是 1,412 行。

## 做了什么

**第 0 组：方法基础**
- `rpc.rs`：新增 `Milestone`（M1a、M1b、M2、M3）和 `MethodError { code, reason? }`。
  - `Method` 加 `MILESTONE`、`ERRORS` 两个常量，`Notification` 加 `MILESTONE`，都没有默认值。
- `schema.rs`：三个 entry 函数写出 `x-milestone`；请求另写出 `x-errors`，写法是 `["not_found", "conflict/turn_running"]`。`annotations.rs` 新增这两个键名，文档表格里也补了对应的行。
- `error.rs`：`ErrorData.supported: Option<VersionRange>` 改为具名字段。
- `methods/mod.rs`：
  - 声明宏 `method!`，有三种写法：`request`、`client_notification`、`host_notification`；
  - `Empty`（`{}`）；
  - 模块文档写了声明方式和命名规则：`host/exit` 对应 `HostExit`、`HostExitParams`；
  - 模块文档还列出所有方法都可能返回的错误。
- `methods/catalog.rs`：`Catalog` 是请求、客户端通知、宿主通知三张按名字排序的 `BTreeMap`。每组提供一个 `register`，`catalog()` 里为每组调用一行。T015 直接用它导出。
- 跨组测试的框架：权限矩阵、`command_id` 与幂等、错误码与里程碑、整数上界、样例覆盖。

**第 1 组：握手与生命周期**（`methods/handshake.rs`）
- `host/hello`；
- `initialize`：
  - `auth` 是开放联合，变体有 `codesign`、`peer`、`dev_key {c_nonce, proof}`、`harness {c_nonce, proof}`；
  - 另有 `role`、`features`（开放枚举 `Feature`）、`coalesce_ms`；
  - 结果包含 `host {host_id, version, weak_auth, data_epoch}`、`host_proof?`、`principal {id, kind, role?, scopes}`、`features`、`limits`；
- `ping`、`client/presence`、`host/status`（`clients` 是 `ClientInfo` 列表，测试构建另带 `pid?`）；
- `host/prepare_upgrade`、`host/exit`、`host/shutdown`、`ui/navigate`。
- 另加两个小工具：`PROTOCOL`（本 crate 支持的主版本区间）、`VersionRange::negotiate`（取双方交集中的最大值）。

**第 2 组：流与内容**（`methods/stream.rs`）
- `stream/subscribe`：结果复用 T014 的 `SubscribeMode`、`ResyncReason`；另有 `StreamSubscribeParams::resume()`，转成 T014 的 `Resume`。
- `stream/unsubscribe`、`stream/events`、`stream/snapshot`、`stream/reset`、`stream/ack`（M3）。
- `history/page`：游标 `{turn_no, op_index?}`。
- `blob/get`、`blob/put`。
- **`Raw<T>`**：
  - 线上是原样的 JSON（`serde(transparent)`），schema 完全转发成 `T` 的 schema；
  - 为 `Raw<Event<EventBody>>` 和 `Raw<EventBody>` 提供 `decode(Strictness)`，用于逐条解码。

## 关键取舍

- **按开工前的裁定做**：trait 常量没有默认值；`nonce`、`proof` 用字符串并标 `x-id`；命令方法的参数直接带 `command_id`；方法目录是三张排序表。
- **类型按完整的方法名命名**：例如 `StreamSubscribe`、`HostPrepareUpgrade`。为此单独做了一次改名提交，生成的 TS 名字和后续各组都按这一条规则。
- **权限矩阵的格子只有 4 种**：`Y`、`N`、`Native`（原生确认）、`ByMain`（仅由主进程代发）。
  - T018 的 `project/add` 等行里，remote_device 那一格是"scope 允许、按 SEC-9 由策略拒绝"。到时需要加一种格子。本任务用不到，我就没加，避免留下没人用的代码。
  - 表里每一行的第一列写 5.8.3 的行名。`stream/ack` 在 5.8.3 里没有单列，归到"订阅"那一行，并在行名里注明。
- **`InitializeParams` 没有实现 `Eq`**：开放联合 `Auth` 的兜底成员是 JSON 对象，只能实现 `PartialEq`。
- **改了 T010 的两处测试**：
  - `error.rs` 里"未知的附加字段能往返"的用例，原来用 `supported` 当作未知字段。它现在是具名字段了，改用 `quota`，保持原来的测试意图。
  - `schema.rs` 测试里遍历整数 schema 的函数挪进 `testutil`，与本任务的测试共用。

## 验证方式

| 验收标准 | 结果 |
|---|---|
| 每个方法的参数和结果都能往返序列化，取值用 5.3.4 的典型值 | 通过。`unit_proto_methods_core_stream_every_method_roundtrips` 对每个样例做 JSON→Rust→JSON 相等、Rust→JSON→Rust 相等、规范 JSON 可序列化三项检查，并断言样例覆盖的方法名集合与目录完全一致。可选字段有无两种写法都覆盖到了。 |
| 方法级 scope 与 5.8.3 逐格一致，表里每行注明出处 | 通过。`..._scopes_match_the_matrix` 按 5.8.2 的 7 个 principal 的 scope 集合，与每个方法的 `x-scope` 逐格比对；目录里每个带 scope 的方法必须恰好出现在一行里。 |
| 需要 `command_id` 的方法一律是"命令"幂等，反之亦然 | 通过。`..._commands_carry_a_command_id` 从 schema 定义里读 `required`，两个方向都检查，并确认字段类型是 `CommandId`。 |
| 宽容解码，有用例 | 通过。`..._one_malformed_event_spares_the_rest`：一条 `stream/events` 里有正常事件、类型已知但数据残缺的事件、缺 `seq` 的事件，整条消息仍能解析；逐条解码时，残缺事件降级成 `Unknown` 并带上原因，缺 `seq` 的那条单独失败，其余照常。`stream/snapshot` 和 `history/page` 的 `ops` 也做了同样的检查。 |
| 整数不超过 2^53−1，导出物不依赖 map 键顺序 | 通过。`..._integers_and_order_are_portable` 遍历目录和全部 schema 定义里的整数 schema，确认都有上下界且没有 `format`；目录按名字排序，规范化后的文本稳定；每个样例都能通过规范 JSON（它会拒绝超界的整数）。 |
| 错误码能在 schema 里查到 | 通过。`..._milestones_and_errors_are_in_the_schema` 核对几个方法的 `x-errors`、`x-milestone`，并确认每个 reason 都属于它前面的 code。 |
| 仍能编译到 `aarch64-apple-ios-sim` | 通过：`cargo build -p cupper-protocol --lib --target aarch64-apple-ios-sim`。 |
| 本机 CI | 通过。在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T012-protocol-methods-core/ci/`：rust 414 项、ts 64 项、license 1 项全部通过；摘要是 `ci/pr-e308d214fdfd.json`，其中 UNIT-proto-methods-core-stream 为 pass。 |
| 测试命名 | 新增测试都以 `unit_proto_methods_core_stream_` 开头。 |

其他检查：
- `clippy --all-targets -D warnings` 通过；
- `RUSTDOCFLAGS="-D warnings" cargo doc -p cupper-protocol` 通过；
- 每个功能提交都单独跑过测试，第一个提交是在临时 worktree 里单独验证的；
- 改坏代码确认测试会失败：把 `host/status` 的 scope 改成 write、把 `host/exit` 的幂等改成天然，对应的两个测试都会失败。

## 提交列表

```
3a29cb8 docs(task): record the T012 pr CI summary
e308d21 docs(task): note the T012 naming rule for the groups that follow
d7eab12 refactor(protocol): name method types after their full method names
06817aa docs(task): record the T012 line counts and implementation notes
5186e71 feat(protocol): add the stream and content methods
bd80dfc feat(protocol): add the method catalog and the handshake methods
3a55bbe feat(protocol): declare milestones and errors on every method
5923f56 docs(task): plan the narrowed T012 after the split
3b13d01 docs(task): add the T012 group plan and line estimate
```

## 遗留问题 / 范围外发现

1. **改了这些共享文件**：
   - `rpc.rs`：加类型和常量，T010 测试里的 `Ping` 补上常量；
   - `schema.rs`：entry 函数加注解，测试里的实现补上常量，并改用 `testutil::integers`；
   - `annotations.rs`：加两个键名和文档表格行；
   - `error.rs`：加 `supported` 字段，并调整上面说的那个测试用例；
   - `testutil.rs`：加 `integers`；
   - `lib.rs`：加一行 `pub mod methods`。

   `sync.rs`、`session.rs`、`index.rs` 和 `view/` 都没有改。
2. **给 T018、T013 的接入说明**（`notes.md` 的"给 T018、T013 的说明"一节写了具体步骤）：
   - 新建组文件，用 `method!` 声明，并提供 `register`；
   - `mod.rs` 加一行，`catalog()` 加一行；
   - 在组文件的测试模块里写 `samples()`，并加进 `catalog.rs` 的覆盖测试；
   - 按 5.8.3 往 `MATRIX` 加行，T018 需要新增 SEC-9 格子；
   - 参数必须是具名结构体。

   覆盖测试的函数名目前是 `unit_proto_methods_core_stream_every_method_roundtrips`。T018 的样例也会在这个测试里检查，如果要按 T018 的编号单独归属，T018 可以把它改成通用的名字。
3. **给 T030、T045**：可以直接用 `handshake::PROTOCOL`、`VersionRange::negotiate` 和 `StreamSubscribeParams::resume()`。`nonce`、`proof` 的格式校验和转字节仍归 T030。
4. **给 T015**：
   - `Raw<Event<EventBody>>` 的 schema 名就是 `Event`，和 T011 交接里说的一致；
   - 方法目录用 `methods::catalog::catalog(&mut generator)`，之后再 `take_definitions(true)`。
