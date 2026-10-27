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

## 开工后发现：守卫的走法漏掉了带标签的分支

带标签的联合（如 `ItemStarted` 的各种条目）每个分支是"`$ref` 加旁边的标签常量"。`fits` 只看 `$ref` 指向的定义，不看旁边的标签，于是第一个分支（`user_message`）对谁都合适，`tool_call`、`plan`、`late_tool_output`、`error` 条目全被当成 `user_message` 走，`ToolCall`、`Plan`、`LateToolOutput`、`ErrorItem` 的属性从没被检查过。另外走法和覆盖面都不进 `oneOf`（`DiffScope`、`Entry`、`GitTarget`、`WorkspaceView` 用它）。

修正后（e0a9905），现有样例就覆盖到 161/220（原来 135），也没有出 `null`。

## 做法

- `tests/fixtures/full.rs`：沿导出的 schema 生成"全填"的实例。`$ref`、`allOf` 合并，`anyOf`/`oneOf` 每个分支（`Unknown…` 和 `null` 除外）各出一个实例，对象的各属性的实例并排组合，数组每个实例放一个元素（`minItems` 要求更多时放够），`const`、`enum` 取第一个值，整数取 1（夹在上下限内）。ID：标了 `x-id` 或定义名以 `Id` 结尾的字符串，取说明里第一个反引号内容到 `_` 为前缀（"Session: `s_` + UUIDv7" 得 `s_01`）；说明里没有反引号、提到 hex 的给 64 位十六进制；`StreamId` 给 `index`；路径给 `/w/a`。
- 根：`Event`（严格解码）、方法目录三张表里每个方法的参数和结果（复用 `each_method!` 类型表）、`RpcError`。视图不做：两个视图里的映射以各种 ID 为键，schema 不说键是哪种 ID，生成不出合法的键；视图是 Rust 自己写出的，`views.json` 里有。
- 守卫（`guard`）：读入、写出、走一遍；再把走到的每个有值的可选属性逐个去掉（按 JSON 指针删），各自再读入、写出、走一遍。对现有样例也这样做。读不进的实例记成错误，和 `null` 一起断言为空。
- 覆盖面：照旧输出"出现过有值"的数目，另加"去掉时检查过"（上一层走到了、这个属性不在）。全部 `Option` 属性都出现过，现在由测试断言，以后加的属性生成不到会直接失败。
- 方法目录在 `Walk::new` 里读一次，不再每个样例重新导出一次 schema：这个测试从约 19 s 降到 1 s 以内（调试构建）。

## 覆盖面

| | 原来（T190） | 只修走法 | 本任务完成后 |
|---|---|---|---|
| 出现过有值 | 135 / 220 | 161 / 220 | **220 / 220** |
| 去掉时检查过 | — | — | **216 / 220** |

"去掉时检查过"差的 4 个都在视图里：`IndexView.agents`、`IndexView.clients`、`IndexView.host`、`RequestView.resolved`。原因同上，视图不生成；`views.json` 的样例里它们都有值，所以"出现过有值"算到了，但没有在 `None` 时被检查过。

## 负面样例（不提交）

| 临时去掉 `skip_serializing_if` 的字段 | 本任务的守卫 | T190 的守卫（main 上的测试） |
|---|---|---|
| `ToolCall.cwd` | 失败，8 处 `null on optional ToolCall.cwd`：6 处来自现有样例（走法修正后才走得到），2 处来自全填实例 | 通过（没抓到） |
| `ErrorData.retry_after_ms` | 失败，只来自全填实例：`full error /data/retry_after_ms: null on optional ErrorData.retry_after_ms` | 通过（没抓到） |
| `ToolDiff.hunks` | 失败，`item.started`、`item.updated`、`item.completed` 的全填实例和一条现有样例 | — |

改回后通过。

## 按模块统计

| 模块 | 提交 | `cargo xtask size` 累计 |
|---|---|---|
| 走法：带标签的分支、`oneOf` | e0a9905 | +12 −7 |
| 全填生成器、逐个去掉、根、覆盖面 | b03b351 | +293 −55（净 +238） |

估计约 195，实际 293，离停止线 300 只差 7。超出估计的原因：走法的修正（估计里没有）；守卫要记下 JSON 指针才能逐个去掉；新文件 `full.rs` 在写完第一个模块时还没加进 git，那次 `cargo xtask size` 没算进去，没能及早发现。
