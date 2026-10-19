# T040 汇报

- 执行者：子 agent（Rust 工程师，宿主编排）
- 分支：`feat/T040-turn-machine-queue`（worktree `.claude/worktrees/T040`，工作区干净）
- 完成日期：2026-10-19

## 做了什么

按补充裁定，T040 收窄为驱动接口和 `QueueCore` 两部分。`TurnMachine` 和四条 PROP 留在 `feat/T048-turn-machine`，归 T048。

- **`crates/cupper-host/src/driver/api.rs`**：设计 5.6.2 的驱动接口，全部是纯数据，纯核心可以直接产出和接收。
  - 接口里只出现协议类型和宿主自己的类型，ACP 的类型不出 `driver` 模块。
  - **命令** `DriverCommand`：Attach、Prompt、Steer、Cancel、SetConfig、Close、Answer、Probe。带回复的请求，在文档里写明了回复类型。
  - **Attach 相关**：`Attach`、`AttachTarget`（New / Resume）、`ProjectConfigPolicy`；回复类型有 `AttachInfo`、`SteerOutcome`，回答类型是 `AskAnswer`。
  - **事件** `DriverEvent`：含 `NormUpdate`、`RunState`、`PromptDone` 和 `PromptFailed`（带 `PromptTag`）、`AdapterExited` 等。
  - **`AgentError`**：错误有三种形式（JSON-RPC、文本、上游重试），分 8 类。`AgentErrorClass::failure_code()` 把分类映射到 `FailureCode`。
  - `driver/mod.rs` 只加了一行 `pub mod api;`。
- **`crates/cupper-host/src/orchestrator/queue.rs`**：`QueueCore`，没有定时器的纯核心。
  - 排队消息先进先出。
  - 只有队列里有消息时才会暂停，`resume` 解除暂停。
  - 每次变更版本号加 1，并返回完整的 `queue.updated`。
  - 上限是 `max_queued_messages`（50）。
  - `queue_edit` 先核对 `expected_version`，再整体生效或整体不生效：
    - `replace`：改写内容和预览；
    - `steer`：只能单独出现，消息留在原位（T140 实现插话前的占位）；
    - `remove`、`move`：返回 unsupported（CP-8）。
  - 预览取消息里的文本块，按 `max_label_chars` 截断。
- **UNIT-queue-core**：6 个用例，测试名都以 `unit_queue_core_` 开头。
  - 按到达顺序出队，版本号逐次递增；
  - 只有队列里有消息时才暂停，暂停期间不出队；
  - 上限 50，满了拒收，状态不变；
  - `queue_edit` 的版本核对、`not_found`、`steer` 不能和其他操作同时出现、`remove` 和 `move` 返回 unsupported，失败时队列不变；
  - `steer` 占位时消息留在原位，出队顺序不变；
  - 预览的拼接和截断。
- **`notes.md`**：写了开工前的估计、每个模块的行数统计、超限停工和拆分经过、已裁定的事项，以及收窄后的范围和行数。

## 关键取舍

- **驱动接口用纯数据**：
  - prompt 带 `PromptTag`，用来分辨中断截止之后才到的旧结果；
  - agent 的请求用 `AskId` 指代，ACP 的 responder 留在 `AgentRuntime` 里，回答走 `DriverCommand::Answer`；
  - `Prompt` 的内容直接用协议的 `PromptContent`，由 T055 转成 ACP 的块；
  - `AttachInfo` 只带一个 `SessionCapabilities`，配置项和斜杠命令都在里面；
  - 事件发送端和回复通道由 actor 在交给 runtime 时补上。
  - 以上已经写进 5.3.11 末尾的"T040 实现时的裁定"。
- **`QueueCore` 用方法接口，没有实现 `Core` trait**：它没有定时器，唯一的调用方是回合状态机，在自己的一步里同步调用它。这样组合起来最直接，性质测试也通过回合状态机一起驱动。
- **队列为空时不暂停**：没有消息在等，暂停看不出来，还要多写一条事件（已裁定）。
- **预览的长度上限借用 `max_label_chars`**：设计里没有给出预览长度，这个上限是现成的。

## 验证方式

- **CI**：在干净的 worktree 里运行 `cargo xtask ci pr --record docs/tasks/T040-turn-machine-queue/ci/`，在提交 d2b8951 上通过：
  - rust 530 项通过，0 失败；四种特性组合（全特性、无特性、`dev-auth`、`test-harness`）的 clippy 和测试都通过；
  - ts 64 项通过，license 1 项通过；
  - protocol、testdata-scan、ui、e2e 作业还没有实现，跳过。
- **摘要**：`ci/pr-d2b895179b2a.json` 已经提交，UNIT-queue-core 记为 pass。我检查过，里面没有用户名和本机路径。

对照验收标准逐条说明：
- [x] UNIT-queue-core：6 个用例，覆盖了补充裁定要求的先进先出、上限 50、`remove`/`move` 返回 unsupported、`steer` 占位时留在原位。
- [x] 测试名以编号的 snake_case 开头，CI 已按编号识别。
- [x] 在干净的 worktree 里跑 CI 通过，摘要随任务提交。
- 四条 PROP 已按裁定转给 T048。

**规模**：相对 main 共 652 行，不含 `Cargo.lock` 和 `docs/`。实现 448 行，测试 204 行，在 M 的范围内。

| 文件 | 实现 | 测试 | 合计 |
|---|---|---|---|
| `driver/api.rs` | 285 | 26 | 311 |
| `orchestrator/queue.rs` | 157 | 178 | 335 |
| `orchestrator/mod.rs`、`driver/mod.rs`、`lib.rs` | 6 | 0 | 6 |
| 合计 | 448 | 204 | 652 |

## 提交列表

```
c0e02c8 docs(task): record the T040 pr CI summary
d2b8951 docs(task): T040 notes for the narrowed scope and its line counts
6e3e0cb test(host): cover QueueCore under UNIT-queue-core
a576725 Merge branch 'main' into feat/T040-turn-machine-queue
c50da70 docs(task): T040 line counts so far, the overrun and a split proposal
13bf686 feat(host): add QueueCore, the session's queue of messages
ec396ff feat(host): add driver::api, the commands and events between session and driver
f268f26 docs(task): T040 size estimate by module before coding
```

## 遗留问题 / 范围外发现

- **交给 T048**：
  - `TurnMachine` 的实现在 e03a930。之前有两个时长的登记，在重置时随那个提交一起移出了本分支，T048 要重新登记：`turn_prepare` 3 s，`interrupt_fallback` 取 `limits::V1`；
  - 四条 PROP；
  - 给 `ResolutionReason` 新增 `turn_ended`；
  - `Action` 的 large_enum_variant 目前用带 reason 的 `#[expect]` 放过了。
- **交给 T043 或 T048**：T018 定义会话方法的参数类型之后，把 `QueueOp`、`QueueEditError`、`QueueFull` 映射到协议的错误码。`QueueFull` 对应哪个错误码，设计里没有写：`session/send` 可能的错误只列了 `not_found`、`unavailable`、`too_large`。
- **交给 T055**：`driver::api` 的实现：
  - 用 `PromptTag` 回填结果；
  - 用 `AskId` 管理 responder；
  - 把 `PromptContent` 转成 ACP 的块：图片读 blob 转 base64，评论排成文本。
