# T018 工作笔记

## 行数估计（开工前）

### 估算方法

沿用 T012 笔记里的密度（结构体约 4.5 行，字段平均约 2.3 行，`open_enum!` 约 6 行加每个取值 1 行），方法声明按 `mod.rs` 文档里的紧凑写法（`scope`、`idempotency`、`milestone` 写一行，`errors` 一行），每个约 7 行。T012 第 1、2 组的估计与实际基本一致（305 → 308、300 → 306），超出都在测试上，所以测试按 T012 第 1、2 组的实际（每组约 100–160 行）估。

参数一律按方法单独定义（`AgentsCapabilitiesParams`、`AgentsRefreshParams` 形状相同也各写一个），以便以后各自加可选字段；这比 T012 的估计多出约 40 行。

### 按组估计

| 组 | 方法 | 类型（结构体 / 联合 / 枚举，字段数） | 代码 | 测试 |
|---|---|---|---|---|
| 3 项目与 agent（`methods/project.rs`） | `project/add`、`trust`、`trust_info`、`remove`，`agents/capabilities`、`refresh`、`restart`（7 个） | 16 / 0 / 1，约 32 个字段 | 240 | 60 |
| 4 会话（`methods/session.rs`，不含 `archive`） | `session/create`、`send`、`interrupt`、`queue_resume`、`queue_edit`、`set_config`、`confirm_config`、`rename`、`restore`、`mark_read`、`get`、`capabilities`、`list_archived`（13 个） | 28 / 2 / 5，约 70 个字段 | 500 | 150 |
| 5 交互请求（`methods/interaction.rs`） | `interaction/respond`、`interaction/get`（2 个） | 6 / 0 / 2，约 18 个字段；`digit` 的 1–9 新类型 | 140 | 50 |
| 共享文件 | `mod.rs`、`catalog()` 各 3 行；`MATRIX` 7 行（每行注明出处）；SEC-9 格子；抽出矩阵逐格比对的辅助函数 | — | 70 | — |
| 本任务编号的测试（`catalog.rs`） | 各组样例恰好覆盖本组方法；本组矩阵行逐格比对；SEC-9 格子；里程碑与补充错误码 | — | — | 80 |
| `session/archive`（等 T013） | 1 个方法，参数和结果各一个枚举，`blockers` 复用 T013 的枚举 | 2 / 0 / 2 | 35 | 10 |
| **合计** | 23 个方法 | | **约 985** | **约 350** |

**总计约 1,335 行**，在约 1,500 行的上限以内，比 T012 拆分时的估计（1,090 行）多约 250 行：每个方法单独的参数类型，以及测试按 T012 的实际上调。

停止线：每完成一组，按已完成组的实际偏差推算总数，预计超过约 1,450 行时停下来汇报。

## 实际行数

口径同 T012：相对分支起点（`git merge-base main HEAD`）的 `git diff --numstat -- crates/cupper-protocol`，测试指 `#[cfg(test)]` 里的行和 `testkit.rs` 的新增。每完成一组统计一次：

| 组 | 提交 | 组文件（代码 / 测试） | 共享文件（`catalog.rs` 等）的净增 | 提交后累计新增 | 当时推算的总数 |
|---|---|---|---|---|---|
| 3 项目与 agent | `a00b881` | `project.rs` 216 / 66 | 154：SEC-9 格子、抽出的 `check_rows`、矩阵 4 行、本任务的测试、`testkit` 的两个样例函数 | 436 | 约 1,360 |
| 4 会话（不含 `archive`） | `a400a35` | `session.rs` 447 / 138 | 45：矩阵 3 行、测试里的方法清单 | 1,066 | 约 1,330 |
| 5 交互请求 | `82864b8` | `interaction.rs` 151 / 87 | 18：矩阵 1 行、测试 | 1,322 | 约 1,380 |
| 队列满（编排者补充裁定） | `03c0b7c` | `session.rs` 测试 +15 | 18：`error.rs` 加 `queue_full`，测试 | 1,355 | 约 1,400 |
| **合计（不含 `archive`）** | | | | **1,355 新增 / 12 删除**：代码约 825，测试约 530 | 估计 1,290 |

- 代码比估计少约 120 行，测试多约 180 行。测试超出主要在 `catalog.rs`：rustfmt 把矩阵行的方法清单、断言里的数组逐项展开，矩阵 7 行就有约 70 行；另有 `testkit` 里两个组共用的能力描述样例（22 行）。
- `session/archive` 估 45 行，加上之后预计约 1,400 行，在 1,450 行的停止线以内。

## 实现时的取舍

形状没有写死、由我补上的地方，请编排者裁定是否写进 5.3.4：

- **参数一律按方法单独定义**：`agents/capabilities` 和 `agents/refresh` 的参数都是 `{agent}`，`session/get` 和 `session/capabilities` 的参数都是 `{session_id}`，也各写一个类型，以后各自加可选字段时互不影响。
- **`project/add.trust` 的类型叫 `TrustScan`**：`ProjectTrust` 按命名规则是 `project/trust` 的方法类型。`project/trust_info` 的结果把 `TrustScan` 的三个字段再写一遍，加上 `diff_from_trusted?`，不用 `flatten`（与 T012 一致）。
- **`trust_digest` 用 `Digest` 类型**（`d_` 加 64 位十六进制）。5.2.6 的信任摘要不是对 JSON 做 JCS，但线上形式一样，客户端只原样回显。T035 按这个形式输出。
- **`findings` 条目**：`path` 是配置文件相对项目目录的路径，项目之上直到 git 根目录的各级以 `../` 开头；`target` 是链接解析后的 realpath；`detail` 是文件里写的内容，原样显示。
- **`diff_from_trusted`** 只在 `stale` 时出现。
- **`agents/capabilities`** 的结果是 `{cached?: {capabilities, captured_at}}`：缓存为空时不出现，客户端据此调用 `agents/refresh` 探测（5.6.7）。5.3.4 只写了"最近一次的能力描述及其采集时间"，没写缓存为空的情况。
- **`agents/refresh`** 的结果是 `{status: AgentStatus}`，复用 T011 的类型。
- **`session/create`**：`workspace` 是按 `kind` 区分的开放联合 `WorkspaceSpec`（`local`；`worktree {base_ref, branch_hint?}`）；`config` 是 `InitialConfig {permission_mode?, model?, reasoning_effort?}`，值是字符串，与 `session.configured` 一致。
- **`Disposition`**（`started`、`queued`、`steered`）由 `session/send` 的结果和 `session/queue_edit` 的 `steer` 结果共用。
- **`QueueOp`** 是按 `op` 区分的开放联合；`steer` 和 `remove` 都只带 `queue_id`，共用 `QueueTarget`。
- **`session/set_config.option`** 用能力描述里的 `ConfigOptionId`（开放枚举，其他 ID 原样透传）；结果的 `config` 与 `session.configured.config` 同为字符串到字符串的映射。
- **`session/get`** 的"完整摘要和工作区信息"写成 `{summary: SessionSummary, workspace: SessionWorkspace, archived_at?}`。已归档的会话也能查，所以带 `archived_at`。
- **`session/list_archived`**：游标是 `{archived_at, session_id}`（与 `history/page` 一样用结构化游标），结果是 `{sessions: [{summary, archived_at}], next?}`，最近归档的在前；没有页大小参数。
- **`session/capabilities`** 的结果直接是 `SessionCapabilities`。方法类型按规则也叫 `SessionCapabilities`（在 `methods::session` 里），组文件里用 `capabilities::SessionCapabilities` 指结果类型。
- **`interaction/respond.input`** 是 `InputEvidence {kind, key?, digit?}`。`digit` 是只接受 1–9 的 `KeyDigit`（serde `try_from`），schema 写成 `minimum: 1, maximum: 9` 的整数。`key` 为 `digit` 时必须有 `digit`、`digit` 与所选选项的序号相符，这些都归 T031 的策略检查。
- **`interaction/get`** 的结果是 `{requested, status, outcome?, resolved_by?, reason?}`：`requested` 是 `interaction.requested` 的数据，后三项与 `interaction.resolved` 相同。
- **错误码的写法**：
  - `project/remove`、`session/archive` 的 `conflict`（有运行中的回合）写成 `conflict/turn_running`，与 T012 的 `host/exit` 一致；
  - `interaction/respond` 的 `forbidden`（含 `user_activation_required`）只写 `forbidden`，因为原因不止一种；`session/create` 的 `unavailable` 同理。
- **队列满**（编排者补充裁定）：`ErrorReason` 加 `queue_full`，归在 `too_large` 下；`session/send` 和 `session/queue_edit` 的 `ERRORS` 加 `too_large/queue_full`。

权限矩阵：

- 新增格子 `Sec9`：remote_device 持有这个方法的 scope，由策略模块按 SEC-9 拒绝或收窄。用在 `project/add`、`project/remove`，以及 `session/set_config`（只允许模型和推理强度）、`rename`、`restore`、`agents/refresh`、`agents/restart` 那一行（`archive` 加入后也在这一行）。
- 5.8.3 里"按档位规则"那一行、`interaction/respond` 的 privileged 选项那一行，都是资源级规则（T031），不进 `MATRIX`：每个方法只能出现在一行里，方法级的 scope 分别由 `session/create`、`session/set_config` 所在的行和 `interaction/respond`（approve 选项）那一行决定。
- `session/queue_edit` 按裁定放在 `session/send` 那一行。本任务的只读方法单独一行，名字注明属于 5.8.3 的"订阅……"那一行。
- T012 的逐格比对抽成 `check_rows`，T012 的全表测试和本任务的测试共用；本任务的测试要求本组的方法单独成行。

## 给 T013、T031 的说明

- T013：`git/revert` 的 `input` 请复用 `methods::interaction::InputEvidence`，不要另定义。
- T031：`InputEvidence` 里 `key` 与 `digit` 的一致性、`digit` 与选项序号的核对，以及所有 `Sec9` 格子，都要在策略模块里实现。

## 进度

- 第 3、4、5 组和队列满已完成并提交。在 `03c0b7c` 上不带 `--record` 跑过 `cargo xtask ci pr`，全部通过；`cupper-protocol` 能编译到 `aarch64-apple-ios-sim`。带 `--record` 的正式运行留到 `session/archive` 完成之后。
- `session/archive` 的 `blockers` 要复用 T013 定义的枚举。T013 还没有合并，按 brief 停在这里，等编排者把 main 合入本分支后再做。
