# T013 工作笔记

## 行数估计（开工前）

### 估算方法

沿用 T012 笔记的密度（T012 实测：第 1 组估 305 行、实际 308 行；第 2 组估 300 行、实际 306 行，代码部分基本准确，测试偏多）：

| 写法 | 行数 |
|---|---|
| 结构体本身（derive、文档、括号、空行） | 约 4.5 行 |
| 字段（必填约 2 行，可选约 3 行） | 平均约 2.3 行 |
| `open_enum!` | 约 6 行，每个取值另加 1 行，带文档的取值再加 1 行 |
| `method!` 声明（带文档） | 约 8 行 |

### 按方法组估计

| 组 | 方法 | 类型数（结构体 / 联合 / 枚举，字段数） | 代码 | 测试 |
|---|---|---|---|---|
| 0 基础 | — | 构建门控：`rpc::Build`、`Method::BUILDS`（默认空，即所有构建都有）、`method!` 的可选 `builds:`、schema 写出 `x-builds`、`annotations.rs` 表格加一行 | 45 | — |
| 1 git、工作区与文件 | `git/status`、`diff`、`file_diff`、`stage`、`unstage`、`revert`、`commit`、`push`、`branches`，`worktree/check`、`files/search`、`workspace/open`（12 个） | 约 24 / 2（`git/diff` 的范围、`git/*` 的目标）/ 10（含 `WorktreeBlocker`），约 65 个字段 | 520 | 130 |
| 2 设置、认证、诊断、测试 | `settings/get`、`update`，`auth/elevate`、`elevation_get`、`elevation_respond`，`policy/describe`，`diag/export`、`recording_export`，`test/clock_advance`、`run_maintenance`、`seed`（11 个） | 约 22 / 0 / 6，约 60 个字段（`Settings` 和 `SettingsPatch` 各 11 个键） | 420 | 110 |
| 跨组测试 | — | `MATRIX` 加行（每行注明 5.8.3 出处）；格子加"策略拒绝""真实输入""仅特定构建"三种；矩阵检查抽成共享函数；`unit_proto_methods_ext_` 的样例覆盖、矩阵、门控标注、里程碑与错误码 | — | 130 |
| **合计** | 23 个方法 | | **约 985** | **约 370** |

**总计约 1,355 行**，按 ±10% 在 1,220 到 1,490 行之间。高于计划的 M（400–900 行），低于约 1,500 行的上限，所以先不停下来；但余量不大：

- **停止线**：每写完一组统计一次。按已完成组的实际偏差推算，总数预计超过约 1,450 行时，停下来汇报。
- 超出 M 的主要原因：本任务 23 个方法，比 T012 的 18 个多；git 组的结果结构多（状态、diff、hunk、检查结果各有嵌套）；`Settings` 和 `SettingsPatch` 两张键表。
