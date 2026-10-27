# T193 工作笔记

## 开工前的核对

- 任务书；交接（`handoff-notes.md` 第一列含 T193 的行，T190）：守卫在 `cupper-protocol/tests/fixtures.rs` 的 `unit_proto_absent_not_null_samples_send_no_null_on_optional_fields`，沿 `schema::document()` 走样例；覆盖面用不带 `absent_not_null` 的生成器再导出一次算出（220 个里 135 个）。
- 导出的 schema 里没有 `pattern`、`format`（只有一处 `double`）、长度限制；ID 的格式在 Rust 里查（`ids.rs`：前缀 + 1 到 64 个字母、数字、`_`、`-`），前缀只写在定义的说明里（如 "Session: `s_` + UUIDv7."）。`StreamId`（`index` 或 `session/<id>`）不是 `x-id`，要单独给值。
- 可以经 Rust 类型往返的根：事件（`Event::decode`，严格）、方法目录里每个方法的参数和结果（复用 `each_method!` 类型表）、两个视图、`RpcError`。

## 想清楚的一点：只"全填"抓不到漏写的 `skip_serializing_if`

守卫抓的是 `None` 被写成 `null`。一个可选字段全填时是 `Some`，往返后不会出 `null`，去掉它的 `skip_serializing_if` 守卫也看不出来。所以分两步：

1. **全填**：每个可选字段都填上，每个 `anyOf` 分支至少出现一次。这样每个定义都能走到，覆盖面（照旧按"出现过有值"算）接近全部。
2. **逐个去掉**：在全填的实例上每次只去掉一个可选字段，再往返、过守卫。这时这个字段是 `None`，它的上一层照样走得到，漏写 `skip_serializing_if` 就会出 `null`。

覆盖面另加一项"去掉时检查过"：可选字段的上一层走到了、这个字段不在，说明它是 `None` 而且没写成 `null`。

## 规模估计（开工前）

| 模块 | 估计 |
|---|---|
| 全填的生成器：沿导出的 schema，`$ref`、`allOf`、`anyOf` 每个分支、数组、映射、ID 前缀、少数按名字给的值 | 100 |
| 逐个去掉一个可选字段的变体 | 35 |
| 根：事件、方法类型表、视图、`RpcError`；往返、读不进时报出 | 45 |
| 覆盖面：加"去掉时检查过"一项，输出 | 15 |
| 合计 | 约 **195** |

停止线 300，硬停止线 350。
