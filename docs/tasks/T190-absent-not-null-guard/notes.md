# T190 工作笔记

## 开工前的核对

- 任务书；`handoff-notes.md` 第一列含 T190 的行（T016：变换在 `schema.rs` 的 `absent_not_null`，`CliPath` 是值本身可为 `null` 的具名定义）。
- 能拿到的样例值：`protocol/fixtures/` 的 `events.json`（事件，按 `Event` 解码再编码）、`methods.json`（每个方法的参数和结果，按各自的类型往返，`tests/fixtures.rs` 已有类型表）、`views.json`（Rust 视图写出的快照操作和视图，本身就是 Rust 的序列化结果）。
- 视图的 proptest 生成器（`view/props.rs`）是库内的 `pub(super)`，集成测试拿不到。

## 做法

- 新的集成测试 `tests/absent_not_null.rs`：样例按 Rust 类型往返后，沿导出的 schema（`schema::document()`，与 `v1.json` 同源）逐层走：遇到 `$ref` 进入定义，`anyOf` 选第一个对得上的分支（按类型、标签常量），对象逐个属性、数组逐个元素、映射逐个值。非必填属性的值是 `null`、而属性自己的类型不允许 `null` 时，记一次违反。`Unknown…` 定义不往下走。JSON-RPC 信封不在范围内（只检查 `params` 和 `result`）。
- 覆盖面：用不带 `absent_not_null` 的生成器再导出一次定义，"带 `Option` 的属性"就是其中非必填且可为 `null` 的属性；样例里出现过非 `null` 值的算覆盖到。

## 规模估计（开工前）

| 模块 | 估计 |
|---|---|
| 测试：读样例、往返（复用 `fixtures.rs` 的类型表要另写一份，集成测试之间不能共用） | 70 |
| 沿 schema 走、判 `null`、记覆盖 | 70 |
| 覆盖面：未变换的定义里找 `Option` 属性、输出未覆盖的 | 35 |
| 合计 | 约 **175** |

停止线 200，硬停止线 240。

## 按提交统计
