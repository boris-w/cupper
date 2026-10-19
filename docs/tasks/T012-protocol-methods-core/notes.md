# T012 工作笔记

## 行数估计（开工前）

### 估算方法

按 T017 笔记里由 T011 `index.rs` 实测得到的密度折算（`index.rs` 342 行：15 个结构体、16 个开放枚举、约 60 个字段，已用当前代码核对）：

| 写法 | 行数 |
|---|---|
| 结构体本身（derive、文档、括号、空行） | 约 4.5 行 |
| 字段（必填约 2 行，可选约 3 行） | 平均约 2.3 行 |
| `open_enum!` / `open_union!` | 约 6 行，每个取值另加 1 行，带文档的取值再加 1 行 |
| 方法声明（用一个声明宏，每个方法一个单元类型加 `Method` 实现） | 请求和客户端通知约 7 行，宿主通知约 5 行 |

测试按往例上调：T011 估 150 行、实际 475 行；T017 估 330 行、实际 567 行。

### 按方法组估计

| 组 | 方法 | 类型数（结构体 / 联合 / 枚举，字段数） | 代码 | 测试 |
|---|---|---|---|---|
| 0 基础设施 | — | `Milestone`、`MethodError`，`Method`/`Notification` 加 `MILESTONE`、`ERRORS` 常量；schema 写出 `x-milestone`、`x-errors`；`ErrorData.supported` 具名；声明宏与 `Empty`；按名字排序的方法目录（供 T015 导出和本任务的表驱动测试） | 240 | 60 |
| 1 握手、生命周期、在场 | `host/hello`、`initialize`、`ping`、`client/presence`、`host/status`、`host/prepare_upgrade`、`host/exit`、`host/shutdown`、`ui/navigate`（9 个） | 15 / 1 / 4，约 49 个字段 | 305 | 110 |
| 2 流与内容 | `stream/subscribe`、`unsubscribe`、`events`、`snapshot`、`reset`、`ack`，`history/page`、`blob/get`、`blob/put`（9 个） | 14 / 0 / 3，约 41 个字段；另有宽容解码用的 `Raw<T>` | 300 | 140 |
| 3 项目与 agent | `project/add`、`trust`、`trust_info`、`remove`，`agents/capabilities`、`refresh`、`restart`（7 个） | 13 / 0 / 1，约 28 个字段 | 200 | 60 |
| 4 会话 | `session/create`、`send`、`interrupt`、`queue_resume`、`queue_edit`、`set_config`、`confirm_config`、`rename`、`archive`、`restore`、`mark_read`、`get`、`capabilities`、`list_archived`（14 个） | 29 / 2 / 7，约 71 个字段 | 510 | 170 |
| 5 交互请求 | `interaction/respond`、`interaction/get`（2 个） | 5 / 0 / 2，约 17 个字段 | 110 | 40 |
| 跨组测试 | — | 5.8.3 权限矩阵逐格比对（表里每行注明出处）、`command_id` 与"命令"幂等互为充要、每个方法的错误码可在 schema 查到且 reason 属于其 code、整数上界、方法清单与 5.3.4 前五组一致 | — | 已摊进各组 |
| **合计** | 41 个方法和通知 | | **约 1,665** | **约 580** |

**总计约 2,250 行，按 ±10% 在 2,000 到 2,500 行之间，明显超过约 1,500 行的上限。** 按守则第 8 条和 brief 的"规模"一节，开工前停下来汇报，等编排者决定是否拆分。

即使把 5.12.5 允许冻结后再加的 M1b、M3 方法（`host/prepare_upgrade`、`project/remove`、`agents/restart`、`session/set_config`、`rename`、`archive`、`restore`、`list_archived`、`stream/ack`，约 270 行）挪走，也还有约 1,980 行；而 brief 第 7 条要求这些方法一并定义，所以不建议这样压缩。

### 拆分建议

按 brief 举的例子分成两个任务，A 先做、先合并：

| 任务 | 内容 | 估计 | 解锁 |
|---|---|---|---|
| **A：方法基础与握手、流**（沿用本分支） | 第 0、1、2 组；权限矩阵、`command_id`、错误码、整数上界等测试的框架，只填本组的行 | 约 1,155 | T030（网关只需要握手）、T045（客户端库需要握手和流） |
| **B：项目、agent、会话与交互** | 第 3、4、5 组；往 A 的方法目录和测试表里加行 | 约 1,090 | T031（还需要 T013）；T015 需要 A、B、T013 都合并 |

- B 依赖 A 的声明宏和目录，A 合并后再开工，或者从 A 的基础设施提交之后开分支。
- 关键路径：T015 等 A、B、T013 都合并。B 的规模和 T013 相当，A 合并后 B 与 T013 并行，不比一个 2,250 行的任务慢；审阅也分成两批。
- 备选：不拆，按组分提交，停止线定在约 2,400 行，审阅时按提交分批看。

## 开工前定下的形状（供编排者一并确认）

拆不拆都按下面做；有不同意见请在回复里指出。

1. **里程碑和错误码作为 trait 常量**：`Method` 加 `MILESTONE: Milestone`、`ERRORS: &'static [MethodError]`，`Notification` 加 `MILESTONE`，都不给默认值，漏写编译不过（与 T010 对 `SCOPE`、`IDEMPOTENCY` 的做法一致）。`schema::method_entry` 等写出 `x-milestone`（`M1a`、`M1b`、`M2`、`M3`，取 5.3.4 里程碑一栏的最早值）和 `x-errors`（`["not_found", "conflict/digest_mismatch"]`，即 5.3.4"可能的错误"一栏，`code/reason` 写法）。所有方法都可能返回的错误（`not_initialized`、`forbidden/scope`、`invalid params`、`rate_limited`、`shutting_down`、`internal`，命令另有 `idempotency_mismatch`）写在模块文档里，不逐个列。
   - 要改的共享文件：`rpc.rs`（加类型和两个常量，T010 测试里的两个实现补常量）、`schema.rs`（三个 entry 函数各加一两行，测试补常量）、`annotations.rs`（加两个键名和表格行）、`error.rs`（`ErrorData.supported: Option<VersionRange>`，附加）、`lib.rs`（一行 `mod methods`）。都不是 T014 会动的文件。
2. **宽容解码**：`stream/events` 的 `events`、`stream/snapshot` 和 `history/page` 的 `ops` 是 `Vec<Raw<T>>`。`Raw<T>` 在线上就是原样的 JSON（`serde(transparent)`），schema 转发成 `T` 的 schema，所以生成的 TS 类型不变；客户端逐个调用 `raw.decode(Strictness::Tolerant)`，内部就是 `Event::decode` / `EventBody::decode`。一条残缺事件只影响它自己。
3. **方法目录**：`methods` 模块提供按名字排序（`BTreeMap`）的请求、客户端通知、宿主通知三张 schema entry 表，T015 的 `export-schema` 直接用；本任务的测试也从这里遍历，保证测试覆盖每个方法。方法清单另由测试按 5.3.4 前五组写定，两者比对。
4. **nonce、proof 用字符串**（64 位十六进制），字段上标 `x-id`（每次运行都不同，金样例要归一化）；格式校验和转字节留给 T030 的网关。
5. **命令参数直接带 `command_id: CommandId`**，不做 `flatten`。`attestation`（M3 预留）以后作为可选字段加入，不破坏兼容。
6. **设计没写死、需要我补的取值或形状**（实现时写进笔记和汇报，请编排者裁定是否写进 5.3.4）：
   - `project/add`、`trust_info` 的 `findings` 条目：`{kind, path, target?, outside_workspace, detail}`，`kind` 为开放枚举（`hook`、`env`、`api_key_helper`、`status_line`、`mcp_server`、`skill`、`agent`、`command`、`codex_config`）。
   - `diff_from_trusted`：按文件列出 `{path, change: add|update|delete}`（复用 `DiffChange`）。宿主目前只存整体摘要，要算这个差异，T035 需要在信任时另存每个文件的哈希清单。
   - `session/send` 的 `fallback_reason`（插话退回排队的原因）：`unsupported`、`turn_mismatch`、`not_prompting`、`prompt_required`、`timeout`、`agent_error`。
   - `session/archive` 的 `blockers`：ADR-10 的 7 条规则各一个取值；T013 的 `worktree/check` 复用同一个枚举。
   - `host/status.clients` 用 `clients.updated` 里的 `ClientInfo` 列表，不只是个数。
   - 错误码补充：`project/trust_info` 加 `unavailable/scan_failed`（重新扫描可能失败）；`session/create` 加 `forbidden/scope`（与 `set_config` 同样受档位规则约束）。
   - `session/queue_edit` 在 5.8.3 里没有单列，矩阵测试按 `session/send` 那一行比对，并在表里注明。

## 拆分后的计划（A：方法基础、握手与流）

编排者裁定拆分（brief 末尾"补充裁定"，计划 v1.3.2）：本任务只做第 0、1、2 组，第 3–5 组归 T018。验收编号改为 UNIT-proto-methods-core-stream。

| 组 | 代码 | 测试 | 小计 |
|---|---|---|---|
| 0 方法基础 | 240 | 60 | 300 |
| 1 握手与生命周期 | 305 | 110 | 415 |
| 2 流与内容 | 300 | 140 | 440 |
| **合计** | **845** | **310** | **约 1,155** |

- 停止线：按已完成组的实际偏差推算，总数预计超过约 1,450 行时停下来汇报。
- `stream/subscribe` 的结果复用 T014 的 `sync::SubscribeMode`、`sync::ResyncReason`。
- 给 T018、T013 留的扩展点：每个方法组一个文件，文件里用 `method!` 声明，并提供 `register(&mut Catalog, &mut SchemaGenerator)`；`catalog.rs` 为每个组调用一行；测试样例、权限矩阵各加自己的行。
