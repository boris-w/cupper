## 第 1 轮：开工前的拆分汇报（2026-10-19）

# T011 汇报（中途停下：预计规模超限，请编排者决定是否拆分）

- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T011-protocol-events`
- 完成日期：2026-10-19（只完成了拆分计划，还没有写代码）

**先说结论**：我按模块估算了规模，约 **2,150 行**（代码约 1,845 行，测试约 310 行；按 ±10% 算，在 1,950 到 2,400 行之间），明显超过 1,500 行的上限。按任务书的要求和守则第 8 条，我写完拆分计划就停下来汇报，没有开始写代码。

## 做了什么

- 读了任务书，设计 5.3.1–5.3.7、5.12.5，以及 5.2.6–5.2.9、5.4.4、5.6.2–5.6.6、5.7.4、5.8.2，ADR-04、ADR-06，T010 的代码、汇报、笔记和 review，还有 SP-4a 的 `events.rs`。
- 写了 `docs/tasks/T011-protocol-events/notes.md` 并提交，内容有三部分：
  - 拆分计划：按模块列出类型数量和行数估计；
  - 两个子任务共用的技术方案；
  - 规划中发现的 11 处设计空缺，以及我打算怎么填。
- 做了一次实测（一次性的小程序，放在会话的临时目录里，没有入库），用来确定技术方案，结果见下一节。

## 关键取舍

**1. 开放联合要区分"未知"和"残缺"，需要新增 `open_union!` 宏。**

- 实测（serde 1.0.229、schemars 1.2.3）发现：派生的 `#[serde(tag = "type")]` 加上末尾的 `#[serde(untagged)] Unknown` 时，类型已知但数据残缺的值，例如 `{"type":"permission","title":1}`，会悄悄落进 `Unknown`，不报错。照 T010 现有的写法，严格模式做不出来。
- 方案：`Serialize` 和 `JsonSchema` 照旧派生。实测 schemars 产出的是 `anyOf`，不会出现 `oneOf` 同时匹配两个成员的问题。`Deserialize` 由宏生成：标签已知时按变体严格解析，标签未知时才落进 `Unknown`。

**2. 严格模式开关是 `Strictness::{Strict, Tolerant}`，作用于 `Event<EventBody>` 和 `EventBody`。**

- 严格模式下直接报错，供 CI 的金样例使用。
- 宽容模式下，`type` 已知但解析失败的，整个事件降级成 `Unknown`，同时把错误交给调用方记日志（ADR-04 修订第 4 条）。

**3. 一个 `EventBody` 覆盖两条流**，用 `stream_kind()` 区分事件属于哪条流。

**4. `fallback.summary`（交接事项）：建议定为"最后兜底的英文短句"，不下发语义码。**

- 理由：只有不认识这个 `type` 的旧客户端才会用到 `fallback`。新类型带来的新语义码，旧客户端也没有对应的翻译，下发了也没用。
- 规则：
  - `summary` 是给开发者看的英文，最多 120 个字符，不含 agent 给的文本和路径；
  - 界面显示本地化的通用占位，`summary` 只放在 tooltip 或诊断信息里；
  - `kind` 取客户端已经会画的占位形状，如 `item`、`hidden`。

## 验证方式

- 这次没有写代码，也没有跑 CI。验收标准中的各项都留给拆分后的任务。
- 实测程序验证了上面第 1 条的判断。
- 根配置没有改动。

## 提交列表

```
540a0bb docs(task): add the T011 module plan and line estimate
```

## 遗留问题 / 范围外发现

**1. 建议拆成两个依次进行的任务**（明细见 notes.md 的表格）：

| 任务 | 范围 | 估计 |
|---|---|---|
| **T011a 事件基础、领域类型与索引流** | `open_union!` 和严格/宽容解码；`EventBody` 的骨架（先放索引流的变体）；公共领域类型（`Content`、`PromptContent`、`AgentKind`、`PrincipalKind`、`ClientKind`、`ClientRole`、`Tier`、`ResolvedBy` 等）；索引流的全部 10 种事件；`SessionCapabilities`；`fallback.summary` 的填法；`ErrorData.resolved_by` 改为具名字段 | 约 1,055 行 |
| **T011b 会话流：条目、ToolCall 与交互请求** | 会话流的 14 种事件；8 种条目、`ToolCall` 及按 `tool_kind` 的必填字段表、`ItemPatch`；交互请求、`Option.effect`、回答；`request_digest`；`item.delta` 的 `Mergeable` 和 `x-mergeable` | 约 1,100 行 |

- T011b 依赖 T011a 的宏、解码函数和公共类型。
- T014 的 `IndexView` 在 T011a 合并后就能开工，`SessionView` 要等 T011b。
- 如果按这个方案拆，当前分支上的 notes.md 可以直接给 T011a 用。

**2. 设计空缺**：notes.md 里列了 11 条，每条都附了建议。下面三条会改动设计写定的形状，请编排者确认或改写设计：

- `late_tool_output.output`：设计写的是必填，但条目开始时还没有输出，建议改为可选；
- `Option.effect.ends_turn`：实际有"会、不会、未知"三种状态，建议用 `Option<bool>`，不出现表示未知；
- 新增 ID 前缀：`IntentId` 用 `n_`，`GroupId` 用 `g_`；另新增 `BlobHash` 类型（不带前缀，带 `x-id`）。这需要在 5.4.4 里补上。

其余几条是设计没有定的地方，按建议补上即可：`ItemPatch` 的形状；`workspace.failed`、`error_code` 的类型；`AgentStatus` 的结构；`cost` 写成 `{amount, currency}`；配置项的值和提问回答的值的类型；排队条目的 `created_by`。

## 第 2 轮：按收窄后的范围实现（2026-10-19）

# T011 汇报

- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T011-protocol-events`
- 完成日期：2026-10-19

**结论**：按收窄后的范围（原 T011a）全部完成。在干净的 worktree 里跑 `cargo xtask ci pr` 通过，UNIT-proto-events-index 为 pass。

**规模**：`crates/cupper-protocol` 新增 1,491 行，删除 60 行（代码约 1,015 行，测试约 475 行）。

- 比批准时估计的约 1,055 行多出约 40%，没有超过 1,500 行的上限。
- 写完 `event.rs` 时累计正好 1,500 行，之后只精简了写法，没有再加代码。
- 多出来的原因主要有两个：
  - 契约类型的文档注释，T015 会把它们导出成 schema 和 TS 的说明，所以没有省；
  - 索引流的开放枚举有 16 个，比估计的多。
- 每个模块的行数见 `notes.md` 的"实际行数"。

## 做了什么

- **`open_union!`**（新文件 `open_union.rs`，设计 5.3.11）
  - `Serialize` 和 `JsonSchema` 照旧派生，schema 是 `anyOf`。`Deserialize` 由宏生成：标签已知时严格解析，错误原样传出，错误信息带上标签；标签未知时才落进 `Unknown`，原样保留数据。
  - 有两种形式：
    - 内部标签：字段名任意，T017 的条目可以用 `kind`；`Unknown` 是 `UnknownMember`，保存整个对象；
    - 邻接标签：只接受 `type`/`data`，供事件体使用；`Unknown` 是 `UnknownEvent`。
  - 生成 `TAGS` 列表和 `tag()` 方法。
- **严格/宽容解码**（`event.rs`）：入口是 `Event::<EventBody>::decode(&Value, Strictness)` 和 `EventBody::decode`，返回 `Decoded {value, degraded}`。
  - `Strict`：任何一层出现"类型已知、数据残缺"都直接报错。
  - `Tolerant`：把整个事件降级成 `Unknown`，并把错误交给调用方记日志。
  - 信封本身残缺、或事件体缺 `type` 时，两种模式都报错。
- **`EventBody`**：包含索引流的 10 种事件。`stream_kind()` 用穷尽的 `match` 判定所属的流，T017 加变体时漏了分类会编译不过。
- **公共领域类型**（`domain.rs`）：
  - `Content`、`BlobRef`；
  - `PromptContent` 的 5 种块，含 `review_comment`、`text_blob`；
  - `AgentKind`、`PrincipalKind`、`ClientKind`、`ClientRole`、`Tier`、`FailureCode`、`ResolvedBy`、`Cost`。
- **新增 ID**（`ids`）：`IntentId`（`n_`）、`GroupId`（`g_`）、`BlobHash`（不带前缀，带 `x-id`）。
- **索引流的全部事件**（`index.rs`）：
  - `project.upserted`（含 `trust_state`）、`project.removed`；
  - `session.upserted`（`SessionSummary`，含 `pending_kind`、`error_code`）、`session.removed`；
  - `agents.updated`：`AgentStatus` 按 5.3.11 第 8 条写成结构，git 和 git-lfs 版本另放在 `git` 里；
  - `host.updated`、`clients.updated`、`elevation.requested`/`resolved`、`notification.intent`。
- **`SessionCapabilities`**（`capabilities.rs`）：含 `turn.interrupt_stops_commands`、配置项及其档位、斜杠命令、提取层能力。`CliCompat` 与 `AgentStatus` 共用。
- **`fallback.summary` 的填法**（`envelope.rs`）：
  - `kind` 改为开放枚举 `FallbackKind`，取值 `item`、`hidden`；
  - `summary` 是给开发者看的英文，最多 120 个字符，`Fallback::new` 在 debug 构建里断言长度。
- **`ErrorData.resolved_by`** 改为具名的可选字段，属于附加改动。

## 关键取舍

- **删掉了 T010 的 `UnknownTagged` 及其测试** `unit_proto_base_open_union_unknown_members_roundtrip`。按 5.3.11，不再用派生的 `untagged` 兜底，这个类型已经没有使用方。开放联合的覆盖移到了 UNIT-proto-events-index。UNIT-proto-base 的其余测试不变，CI 中仍为 pass。
- **内部标签联合的 `Unknown` 保存整个对象**：任意标签名都能用，往返时一字不差。代价是它的 schema 是任意对象，T015 后处理时用 `.loose()`。
- **降级的单位是一个事件**：输入是 `&Value`，不需要先复制。客户端要逐个事件解码，见下文"遗留问题"中给 T012 的一条。
- **设计只列了字段、没有列取值的地方**（`notes.md` 有完整的表）：
  - `AgentState`：`unavailable`、`idle`、`starting`、`running`、`crashed`；
  - `CliInstall`：`native`、`npm_shim`、`unsupported_install`；
  - `ConfigDirState`：`default`、`ok`、`rejected`；
  - `AuthState`：`logged_in`、`logged_out`、`unchecked`。不用 `unknown`，因为 `open_enum!` 已经占用了 `Unknown` 这个变体名；
  - `DiffSide`：`old`、`new`；
  - `FailureCode` 在 5.3.11 列出的取值之外，另加 `adapter_crashed`、`cli_missing`、`runtime_failed`；
  - `CliCompat` 不加新取值，读不到版本号时记为 `unsupported`。
- **5.3.5 中没有标可选的字段，都写成必填**：`SessionSummary.title` 在 `title_source: none` 时为空字符串；`BlobRef.media_type` 必填。`last_exit_abnormal?` 写成只在为 true 时出现的布尔值。

## 验证方式

| 验收标准 | 结果 |
|---|---|
| UNIT-proto-events-index | 通过。15 个测试覆盖：<br>- 索引流每种事件的往返，带信封和不带信封各一次；<br>- 未知类型；<br>- 未知枚举值和多余字段（不算残缺）；<br>- 6 类残缺数据（缺字段、负数、ID 前缀错、嵌套缺字段、没有 `data`、`data` 类型错），严格模式下报错，宽容模式下降级、错误交出、数据原样保留；<br>- 信封或标签残缺时两种模式都报错；<br>- `open_union!` 本身：已知/未知成员、无字段的成员、残缺被拒、schema；<br>- 领域类型、能力描述、新 ID、`fallback`、`resolved_by` |
| 第 2 组对照表 | 见下表 |
| 编译到 `aarch64-apple-ios-sim` | debug 和 release 都通过 |
| `cargo xtask ci pr --record` | 在干净的 worktree 里于 `affa9f4` 运行，通过：rust 93 项、ts 26 项、license 1 项；UNIT-proto-events-index 和 UNIT-proto-base 均为 pass。摘要是 `docs/tasks/T011-protocol-events/ci/pr-affa9f47509f.json`，不含路径和用户名 |
| 测试命名 | 本任务新增的测试都以 `unit_proto_events_index_` 开头 |

另外：`RUSTDOCFLAGS="-D warnings" cargo doc -p cupper-protocol` 通过，clippy 的 `-D warnings` 通过。我还临时加了一个测试，确认 `Event<EventBody>` 能正常生成 schema（结构是信封的字段加 11 个成员的 `anyOf`），用完已撤掉，没有提交。

**5.12.5 第 2 组：本任务负责的部分**

| 第 2 组的项 | 定义在 |
|---|---|
| 能力里的 `turn.interrupt_stops_commands` | `capabilities::TurnCapabilities.interrupt_stops_commands` |
| `project.upserted.trust_state` | `index::ProjectUpserted.trust_state`、`TrustState {untrusted, trusted, stale}` |
| v1.4 `SessionSummary.pending_kind` | `index::SessionSummary.pending_kind`、`PendingKind {approval, input}` |
| `queue.updated` 条目的 `content` 用到的类型 | `domain::PromptContent`、`PromptBlock`（含 `review_comment`、`text_blob`）；事件本身属于 T017 |
| 注解 `x-id`、`x-time`、`x-path` | 新增的 `IntentId`、`GroupId`、`BlobHash` 都带 `x-id`；索引流的时间字段用 `TimestampMs`，路径字段用 `WirePath` |
| 活动组 `group_id` 的 ID 类型 | `ids::GroupId`；规则和字段属于 T017 |

时间字段、`final_item_ids`、`ToolCall`、`late_tool_output`、`may_still_run`、`session.configured` 的两个字段、`snapshot.recorded`、`Option.effect`、`reply_to` 都属于 T017；`project/trust` 等方法属于 T012、T013。

## 提交列表

```
e204451 docs(task): record the T011 pr CI summary
affa9f4 docs(task): record the T011 implementation notes
f648d72 feat(protocol): add EventBody with strict and tolerant decoding
f635621 feat(protocol): add the index stream event data
2a61d88 feat(protocol): add the session capabilities
e1bfb8f feat(protocol): name ErrorData.resolved_by
ac34009 feat(protocol): add the shared domain types
9dfc25d feat(protocol): add the IntentId, GroupId and BlobHash IDs
048a39f feat(protocol): define the fallback placeholder rules
ad1fd0f feat(protocol): add open_union! with strict known members
9881ec0 docs(task): add the T011 module plan and line estimate
```

## 遗留问题 / 范围外发现

1. **根配置没有改动**，也没有新增依赖。T010 的代码有以下改动：
   - 删除了 `UnknownTagged` 和它的一个测试；
   - `Fallback.kind` 从字符串改为开放枚举，线上的格式不变；
   - `ErrorData` 新增一个可选字段；
   - 改了 `envelope` 和 `open_enum` 的模块文档。
2. **给 T012**：`stream/events`、`stream/snapshot`、`history/page` 的参数如果整体用派生的 `Deserialize` 解析，其中一个残缺事件会让整条通知解析失败。参数类型要让客户端能逐个事件调用 `Event::decode` / `EventBody::decode`（宽容模式）。
3. **给 T017**：
   - 在 `EventBody` 里加会话流的变体，`stream_kind()` 会强制你给新变体分类；
   - `item.delta` 变体上加 `#[schemars(extend("x-mergeable" = true))]`，并为 `EventBody` 实现 `Mergeable`；
   - 条目联合用 `open_union!` 的 `in tag "kind"` 形式；
   - 测试可以复用 `testutil::{roundtrip, open_values}`。
4. **给 T015**：
   - `Event<EventBody>` 的 schema 名是 `Event`；
   - `UnknownMember` 的 schema 是任意对象，后处理时用 `.loose()`。
5. **事件的 `v` 常量**（`EVENT_VERSION`）这次没有定义，由第一个写事件的宿主任务按需加上。
6. **建议补进设计**：上面"关键取舍"里那些设计只列了字段的取值（`AgentState`、`CliInstall`、`ConfigDirState`、`AuthState` 的 `unchecked`、`DiffSide`、`FailureCode` 新增的三个值），建议编排者写进 5.3.11 或 5.3.5。
