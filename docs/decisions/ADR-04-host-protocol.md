# ADR-04 宿主与客户端之间的协议

- 状态：提议
- 日期：2026-10-16
- 来源：`docs/meetings/2026-10-16-tech-selection/`（`minutes.md` C1–C3；`architect.md` ADR-04；三份第二轮回应）

## 背景

协议需要同时服务这几类客户端：桌面 UI、cupper-cli，以及 M3 起的 iOS（经由端到端加密通道和中继）。有几个硬性前提：

- NFR-1 要求宿主协议从第一版起就具备：版本与能力协商、客户端身份和逐条命令的授权、按游标续传、命令幂等、大内容按需拉取。
- NFR-10：iOS 由用户自己构建，版本必然和桌面错开。
- M3 要在 Noise 通道上逐条加密，Noise 单条消息最多 65535 字节。

## 可选方案

1. 自有协议：JSON-RPC 2.0 信封，JSON 编码
2. 对外直接讲 ACP
3. 沿用 Codex app-server 协议
4. gRPC/Protobuf
5. 自定义二进制格式（CBOR、MessagePack）

schema 的来源也有几种选择：Rust 类型、手写 JSON Schema、TypeSpec、`.proto`。

## 决策

**采用方案 1。schema 以 Rust 类型为源，生成 JSON Schema 作为对外契约。**

**分帧**
- 帧格式：`u32 长度（大端）| u8 帧类型 | 负载`。
- **本地和远程统一用一个上限**：单帧不超过 64,000 字节。
- 帧类型：
  - `0x01`：JSON 消息；
  - `0x02`：blob 分片（原始字节，带一个小头部：hash、偏移、是否最后一片）；
  - `0x03`：预留给二进制编码。
- JSON 消息不跨帧分片。超过上限的内容一律改为 blob 引用，用 `blob/get` 拉取，用 `blob/put` 上传。

**消息类别**（完整清单见 `architect.md` 的 ADR-04 一节）

| 类别 | 消息 |
|---|---|
| 握手 | `initialize` |
| 订阅与事件 | `stream/subscribe`、`stream/events`、`stream/reset`；`stream/ack` 在 M3 启用 |
| 历史 | `history/page` |
| 查询 | `session/capabilities`、`agents/status`、`git/status`、`git/diff`、`files/search`、会话内搜索等 |
| 命令 | `session/create`、`session/send`、`session/interrupt`、`interaction/respond`、`git/stage`、`git/commit` 等 |
| 内容 | `blob/get`、`blob/put` |
| 认证 | `auth/elevate` |
| 在场 | `client/presence` |
| 生命周期 | `host/shutdown` |
| 心跳 | `ping` |

宿主不会主动向客户端发请求。审批和提问都是事件流里的状态对象，客户端用命令来回答。

**版本与兼容**
- 主版本是一个整数，只有破坏性变更才加 1。可选能力通过 `features` 协商。宿主同时支持当前和上一个主版本（N 和 N−1）。
- 客户端按"宽容读取"处理：
  - 不认识的字段直接忽略；
  - 枚举和联合类型默认是开放的，遇到不认识的值，落到生成器产出的 `Unknown` 兜底分支；
  - 事件信封带 `fallback{kind, summary}`，给不认识的条目显示占位。
- 事件信封保持开放：`type` 是普通字符串，每种事件的 `data` 单独定义 schema。
- 新的交互请求类型必须用 `features` 门控。宿主通过 `actionable_by` 声明哪些客户端可以处理，处理时宿主自己也会再检查。
- 兼容性检查区分消息方向：宿主发往客户端的消息，删除字段、把必填改成可选，都算破坏性变更；客户端发往宿主的消息，新增必填字段算破坏性变更。

**seq 与合并**
- 日志里同一条流的 seq 连续。
- **写入时合并**：同一条目的文本片段按 33–50 ms 合并。条目的第一个片段立即发出；条目结束时，或者发出其他事件之前，先把缓存的片段刷出去。
- **发送时合并**：对慢连接，可以把一段片段合并成区间事件，用 `from` 字段标出它覆盖的 seq 区间。合并窗口在握手时以 `coalesce_ms` 协商。
- 客户端检查：本事件的 `from`（没有 `from` 时取它的 `seq`）必须等于上一个事件的 `seq` 加 1；不满足就按游标重新订阅。
- schema 里标出哪些事件类型可以合并。加一条性质测试：合并后应用和逐条应用，结果必须完全一样。

**命令**
- 每条命令带 `command_id`（UUIDv7）。宿主至少保留 24 小时的记录，同一个 id 重复到达时直接回放上次的结果。
- 前置条件：
  - `expected_turn_id`：用于 Steer；
  - `request_digest`：用于审批和回答提问。摘要由宿主计算并下发，客户端原样回显，客户端不重新计算；
  - `hunk_id` 加 diff 基线：用于暂存和还原；
  - `expected_version`：用于编辑队列。
- 命令信封里预留一个可选的 `attestation` 字段，供 M3 和 M4 放设备签名。
- 错误：应用错误统一用 `-32000`，客户端按 `error.data.code` 分支处理。错误码见 `architect.md` 的错误码表。界面上的文案由客户端负责本地化。

**标识与时间**
- ID 一律是带前缀的字符串；host_id 随机生成。
- 时间用 UTC 毫秒整数；seq 不超过 2^53。

**代码生成与 CI**
- 生成链路：`crates/cupper-protocol`（serde + schemars）→ 提交到仓库的 `protocol/schema/v<N>.json` → 用 @hey-api/openapi-ts 生成 TS 类型和 zod 校验（不用 strict 模式）。生成出的 TS 也提交到仓库。
- CI 检查：
  - 生成结果与仓库中的一致；
  - 与上一个发布版本做兼容性比对（区分方向）；
  - `protocol/fixtures/` 里的样例（包括未知值）在 Rust 和 TS 中往返；
  - 录制的会话经假 agent 回放后，结果与金样例一致；
  - 每个帧都不超过上限（性质测试）；
  - 共享的 crate 能编译到 `aarch64-apple-ios-sim`。
- **iOS 怎样使用协议，由 ADR-15 在 M3 前决定**：要么通过 UniFFI 共用一个不做 I/O 的客户端 crate，要么由 schema 生成原生 Swift 类型。M1 只需保证这两条路都还能走。

**其他**
- 远程链路上，加密之前不做压缩，避免长度侧信道。
- 事件模型的细节（条目类型、upsert、流的划分、落盘、保留策略、"大字段改为 blob 引用"的阈值）留给 ADR-06。这个阈值对所有连接都一样。

## 后果

- **好处**：
  - 和 ACP、Codex app-server 的做法一致，AI 和贡献者都熟悉；
  - 每条消息可以单独加密，M3 不需要返工；
  - 帧格式和上限从 M1 起就与 Noise 兼容。
- **代价**：
  - 需要自己维护兼容性检查，以及 Swift 的生成链（如果 ADR-15 选择原生 Swift 类型）；
  - 64 KB 的上限要求快照分页、大内容一律走 blob，宿主和客户端都要多写一些代码。
- **改变条件**：M3 实测 JSON 的带宽太大时，在握手中协商 `0x03` 二进制编码，schema 不变。

## SP-4a 实测修订（2026-10-16）

1. **整数**：协议里所有整数都不超过 2^53−1。导出 schema 时用上下界代替 `format: uint64/int64`，CI 用 `integers_are_js_safe` 检查。
2. **消息**：
   - 补上 `stream/snapshot`：快照分页下发，内容是一串事件；
   - 事件信封去掉 `id`，与 ADR-06 一致。
3. **TS 生成链**：需要补一个后处理步骤或 hey-api 插件，工作量约 0.5–1 人日，做三件事：
   - 生成已知事件类型的字面量列表和类型守卫；
   - 兜底成员用 `.loose()`，保留未知字段；
   - 开放枚举写成 `(string & {})`。
4. **"类型已知但数据残缺"的处理**：客户端把它降级成未知事件，并记日志；CI 对金样例做严格校验，不允许降级。
5. **`blob/put` 在 M1 实现**：64 KB 的帧上限，对用户粘贴的长 prompt 会立刻起作用。
6. **命令参数的摘要**：基于键排好序的规范 JSON 计算。

## 设计评审修订（2026-10-16）

来源：设计评审会（`docs/meetings/2026-10-16-design-review/minutes.md`）、用户决定（`docs/meetings/2026-10-16-gate-design-decisions/minutes.md`）、详细设计 v0.7 的 5.13.2。本节是细化，不推翻上文；上文中被替换的写法已就地改正并注明。

- **握手**：增加 `host/hello`（下发 nonce）；`initialize` 每条连接只接受一次，角色确定后不可变，增加 `nonce`、`role`；退化模式的认证改为绑定信道的双向证明（方向标签、双方 nonce、各自看到的对端 pid），结果带 `host_proof` 和 `data_epoch`（5.3.2）。
- **命令**："受理"语义，命令结果和它产生的事件在同一事务里落盘；账本按 principal 的稳定 ID 记；`session/create` 必须带第一条消息，并随受理事务落盘；window 角色的 `interaction/respond`、`git/revert` 必须带 `input` 证据。
- **同步与历史**：`history/page` 读回合快照，游标细到回合内的位置 `{turn_no, op_index?}`；`stream/subscribe` 带 `epoch`，`epoch_changed` 或 `cursor_ahead` 时走快照。
- **事件**：`item.delta` 带 `field: text|output`，合并键为 `(item_id, field)`；各条目带时间字段；`turn.completed.final_item_ids`；`late_tool_output` 条目；`ToolCall` 的必填字段按 `tool_kind` 列表。
- **其他**：`blob/put` 先发分片再发请求；hunk 的前置条件具体化为 `diff_token`；错误码增加 `git_error` 并给出 reason 清单；规范 JSON 用 RFC 8785，摘要保留完整的 256 位；要批准的内容不截断，超长改为 blob。
- **v1 契约在 M1a 冻结**：冻结清单和机制见详细设计 5.12.5；之后 M1b 只加方法、事件、可选字段和开放枚举的取值。方法清单以 5.3.4 为准。
