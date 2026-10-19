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

（每完成一组更新一次；口径同 T012：`git diff --numstat main -- crates/cupper-protocol`，测试指 `#[cfg(test)]` 里的行。）
