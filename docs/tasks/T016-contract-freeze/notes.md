# T016 工作笔记

## 开工前的核对

- `handoff-notes.md` 第一列含 T016 的行：T015（可选字段的表示、`Settings.cli_path`、`v1.json` 是加缩进的 JCS、方法目录在 `x-methods` 的三张表、整数安全检查）、T032（`replayed: true`）、T042（`interaction/respond` 缺 `not_found`）；计划 P-1：冻结前改契约要单独提交、同一任务里重新生成、汇报里列出改了什么。
- 设计 5.12.5 的冻结机制与清单；5.1.6 的 `protocol` 作业。

## 冻结前的四件：现状与方案

1. **可选字段**：schemars 1.2 给每个 `Option<T>` 都加上 `null`，`v1.json` 里 222 个可选属性全是"可缺省且可为 `null`"，TS 生成 `?: T | null`、zod `.nullish()`。宿主序列化时这些字段都带 `skip_serializing_if`，只缺省、从不发 `null`；唯一的例外是 `Settings` 的两个 `cli_path`（没有 `skip`，`None` 时发 `null`）。
   - 方案：导出时加一个变换，非必填属性去掉 `null`（"可选就是缺省"），真正可为 `null` 的值用自己的类型声明（见第 2 件）。Rust 反序列化照旧接受 `null`，入方向宽松。
   - 试验（未提交）：生成后 `tsc --build` 全部 11 个 TS 项目 0 个错误；TS 单测只有 `settings/get`、`settings/update` 两条金样例失败（`cli_path: null`），由第 2 件解决。TS 端没有给协议可选字段传 `null` 或与 `null` 比较的调用点。
2. **`Settings.cli_path`**：`Settings` 每个键都在，`cli_path` 的值是路径或 `null`（自动发现）；`SettingsPatch` 里给 `cli_path` 传 `null` 表示清除，其余键的 `null` 在 Rust 反序列化时已被拒绝，但 schema 写着可为 `null`。
   - 方案：新类型 `CliPath`（路径或 `null`，具名的 `$defs`），`Settings` 里必填，`SettingsPatch` 里可选；第 1 件的变换去掉外层的 `null`，`CliPath` 自己的 `null` 留下。其余键在补丁里成为"可选、非 `null`"。TS 端没有读写设置的调用点（`settings/update` 的处理在 T036）。
3. **`replayed: true`**：账本回放时把它插进结果对象（`ledger::replayed`），25 个 `command` 幂等的方法都会；结果类型里有共用的 `Empty`，不能逐个结果类型加字段。
   - 方案：`$defs` 里加一个 `Replayed`（`{replayed?: true}`），方法目录里 `command` 类请求的 `result` 写成 `allOf: [<结果>, Replayed]`。这些结果都是开放对象，没有矛盾；TS 只多一个 `Replayed` 类型，客户端没有读它的地方。
4. **`interaction/respond` 的 `not_found`**：已由 T088 完成（89d1f34）：`x-errors` 里有 `not_found`，会话里不认识的请求 ID 回 `not_found`，`orchestrator/interaction/tests.rs` 有用例。本任务不再改。

## 规模估计（开工前，生成物不计）

| 模块 | 内容 | 估计 |
|---|---|---|
| 第 1 件 | 导出变换与单测 | 40 |
| 第 2 件 | `CliPath`、`Settings` 必填、补丁 | 35 |
| 第 3 件 | `Replayed` 与目录里的 `allOf` | 30 |
| `tools/schema-compat` | 包骨架；按方向分类型（参数入、结果和通知与事件出、两向都有的取严）；规则：删字段、出方向必填改可选、入方向新增必填或可选改必填、闭合枚举的取值、删方法和事件、类型改变 | 250 |
| UNIT-schema-compat | 各类情形 | 150 |
| 整数安全检查 | 导出文件里每个整数都有不超过 2^53−1 的界 | 40 |
| ShellAPI 快照 | `api-m1a.d.ts`（快照不计）与类型断言 | 25 |
| `xtask ci protocol` | 兼容比对、整数检查、ShellAPI 断言三步 | 35 |
| `xtask size` | 快照文件不计入 | 5 |
| 合计 | | 约 **610** |

停止线 850，硬停止线 950。

## 按模块统计
