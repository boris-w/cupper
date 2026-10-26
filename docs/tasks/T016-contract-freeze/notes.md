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

编排者裁定（2026-10-26）：第 1–3 件按方案做，第 4 件不改；`tools/schema-compat` 要把新增事件种类、视图里新增的成员判为附加式。

每次提交后跑 `cargo xtask size`（生成物与两个快照不计）：

| 提交 | 新增 | 删除 | 累计 |
|---|---|---|---|
| `CliPath` | 28 | 10 | +28 |
| 可选字段只缺省 | 69 | 1 | +97 |
| `replayed` 的声明 | 57 | 7 | +154 |
| `released/v1-m1a.json`（快照不计）、`xtask size` 不计两个快照 | 5 | 0 | +159 |
| `tools/schema-compat` 与 UNIT-schema-compat | 452 | 0 | +611 |
| ShellAPI 快照（不计）与 `frozen.ts` | 55 | 0 | +666 |
| `xtask ci protocol` 的三步 | 23 | 6 | +689 |

合计 +689 −24，停止线 850 以内。`schema-compat` 超出估计（估 435 行，实际 452），其余接近估计。

## 过程记录

- 事件目录是开放的：`EventBody` 和 `Event` 是 `anyOf`，最后一个分支是 `UnknownEvent`；开放联合以 `UnknownMember` 兜底，开放枚举以 `{type: string}` 兜底；schema 里没有一个闭合的枚举。`schema-compat` 据此把"有兜底的联合"当作开放的：加分支是附加式，所以新增事件种类通过。
- 方向：参数、客户端通知是入方向；结果、宿主通知，以及导出的 `Event`、`EventBody`、`IndexView`、`SessionView`、`RpcError` 是出方向；两边都用到的定义两条规则都适用；没有被引用到的定义按两个方向算（取严）。
- 演示（对 `released/v1-m1a.json` 的改动副本跑 `schema-compat`）：删 `SessionSummary.title`、`SessionCreateResult.session_id` 改可选、`SessionSendParams` 新增必填的 `token`，都以退出码 1 失败；`EventBody` 加 `settings.updated`、`IndexView` 加 `settings`、新增方法 `settings/watch`，退出码 0。
- ShellAPI：`frozen.ts` 逐项试过：删成员、删嵌套成员、改参数类型、删联合分支、收窄分支都让类型检查失败；加成员、给联合加分支通过。最初用"今天的接口可赋给冻结的接口"判定，联合加分支会经 `init()` 的返回值传到 `ShellInfo`，被误判为破坏，所以改成逐成员核对"还在、旧的调用还能通过类型检查"，联合单独核对"旧分支都还在"。
- 从冻结前的 schema（本分支开工时的 main）到现在，`schema-compat` 报 247 处破坏：都是本任务冻结前的三件改动（去掉 `null`、`CliPath`、结果包上 `allOf`），符合预期。

## 合并 main 之后

- 编排者把 main（含 T036 的设置）合进本分支。T036 的代码还在用旧的 `Option<WirePath>`，单独一个提交改成 `CliPath`：`settings.rs` 的默认值与绝对路径校验、它的单测、`launch.rs` 取 Claude 的 CLI 路径。`pnpm gen` 之后 `protocol/released/v1-m1a.json` 仍与导出的 schema 逐字节相同。
- 第一次 `ci pr --record` 只有 ts 作业的 `test-shell-api` 失败："Cannot allocate memory (os error 12)"，pnpm 拉不起 vitest，是机器当时的资源问题（单独跑 14 项全过）；丢掉那份摘要后重跑全部通过，摘要已提交。
