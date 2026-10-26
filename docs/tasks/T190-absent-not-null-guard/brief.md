# T190 可选字段不发 `null` 的守卫

- 派发日期：2026-10-26
- 执行者：子 agent（工程师）。T016 的执行者接着做
- 分支：`test/T190-absent-not-null-guard`
- 依赖：T016（已合并）。T099 依赖本任务

## 背景

T016 冻结契约时定下"可选字段只缺省、不为 `null`"（设计 5.3.1）：导出时去掉非必填属性上的 `null`，TS 的 zod 从 `.nullish()` 变成 `.optional()`。这一点靠每个 `Option` 字段都写了 `skip_serializing_if`。漏写一个，宿主就会发出 `null`，界面的 zod 会拒收整条消息，而现在没有任何检查能发现。编排者合并前查过，线上类型都写了，所以这是防以后出错的守卫（计划 v1.3.58）。

## 范围

**做**：
1. **UNIT-proto-absent-not-null**：
   - 把 `cupper-protocol` 的金样例与夹具（`protocol/fixtures/` 的事件、方法、视图，以及其他能拿到的样例值）按 Rust 类型读入、再序列化；
   - 按导出的 schema 逐层找到每个属性的定义，断言非必填属性上没有 `null`；
   - 值本身可以为 `null` 的具名定义（如 `CliPath`）不算违反；
   - JSON-RPC 信封本身不在检查范围内（错误响应的 `id` 按 JSON-RPC 可以为 `null`）。
2. **负面样例**：临时去掉一个字段的 `skip_serializing_if`，确认守卫会失败（不提交，写进 `notes.md`）。
3. **覆盖面**：说明样例值覆盖了 schema 里多少个带 `Option` 的属性；没覆盖到的，看能不能用现有的生成方法补上（例如 proptest 的生成器，或者每个类型的默认值加上所有可选字段都填满的值），补不上的列出来。

**不做**：改契约；改导出时的变换。

## 验收标准

- [ ] UNIT-proto-absent-not-null 通过；负面样例让它失败。
- [ ] 覆盖面的数字与没覆盖到的属性写进 `notes.md` 和汇报。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T190 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T190-absent-not-null-guard/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T190 的行）

- T016：导出时的变换是 `cupper-protocol/src/schema.rs` 的 `absent_not_null`；`CliPath` 是值本身可以为 `null` 的具名定义。

## 注意

- 共享的 crate（`cupper-protocol` 的库部分）不做 I/O；读夹具文件的代码放在测试里。

## 规模

- 停止线：200；硬停止线：240

<!-- 执行者开工前在 `notes.md` 里写估计；每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b test/T190-absent-not-null-guard main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-26，编排者）

执行者写完守卫后才统计行数，结果是 +287 −14，越过了硬停止线 240。守则第 8 条要求每完成一个模块就统计一次，这次没做到，记在评审里。守卫本身已完成，负面样例能让它失败：去掉 `AuthElevateResult.expires_at` 的 `skip_serializing_if`，测试就报错。

**裁定（计划 v1.3.61）**：
- 采用方案一，不压缩：守卫和覆盖面的统计留在同一个测试里，以后加样例时覆盖面自动增长。
- 补上覆盖面统计的两处修正：允许为 `null` 的属性在样例里取 `null` 时算作覆盖到；把 `RpcError` 的可选字段算进来。
- 样例只覆盖到 211 个可选属性中的 134 个，其余 77 个由新增的 T193 补：按 schema 为每个方法的类型和每种事件生成"可选字段全填"的合法实例。T193 排在 M1b 第一次附加改契约（T151）之前。

- 停止线：310；硬停止线：330
