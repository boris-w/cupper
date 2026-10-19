# T015 工作笔记

## 开工前的实测（2026-10-19）

在 scratch 目录里先导出一版 schema，用 `@hey-api/openapi-ts` 0.99.0（SP-4a 的版本，也是当前最新的正式版）生成，看清要做哪些处理：

- 导出：341 个定义，约 240 KB。`Event<EventBody>` 的定义名就是 `Event`：外壳属性加 27 个成员的 `anyOf`（26 种已知事件加 `UnknownEvent`）。
- **hey-api 会丢掉 `$ref` 旁边的属性**。schemars 把内部标签联合的 newtype 成员写成 `{"$ref": "#/$defs/ToolCall", "properties": {"kind": {"const": "tool_call"}}, "required": ["kind"]}`，hey-api 只认 `$ref`：TS 成员成了 `ToolCall`，没有 `kind` 字面量，无法收窄；zod 成员成了 `zToolCall`，解析时 `kind` 被剥掉，联合也不再按标签区分。受影响的有 `ItemStarted`、`InteractionRequest`、`InteractionOutcome`、`Auth` 等所有 `open_union!` 的 `in tag` 形式。转成 OpenAPI 时把这种写法改成 `allOf: [{$ref}, {其余}]`，实测得到 `ToolCall & {kind: 'tool_call'}` 和 `zToolCall.and(z.object({kind: z.literal('tool_call')}))`。
- `UnknownMember` 生成 `z.record(z.string(), z.unknown())`，本身就保留全部字段；`UnknownEvent` 生成普通 `z.object({data, type})`，要加 `.loose()`。
- 开放枚举生成 `'a' | 'b' | string`，要改成 `(string & {})`。
- hey-api 还会生成 `ClientOptions` 类型和 `index.ts`：前者是 SDK 用的，后处理删掉；后者用 `entryFile: false` 关掉，包入口自己写。

## 行数估计（开工前）

生成物（`protocol/schema/v1.json`、`packages/protocol/src/gen/`）和夹具（`protocol/fixtures/`）不计入（计划 1.1）。

| 模块 | 内容 | 代码 | 测试 |
|---|---|---|---|
| 1 Rust 导出 | `schema::document()`；`canonical::to_canonical_pretty`（入库文件要能审阅 diff，见"关键取舍"）；bin `export-schema` | 105 | 40 |
| 2 `tools/gen-protocol-ts` | 包配置；转 OpenAPI（含 `$ref` 旁属性改 `allOf`）、调用 hey-api、写文件、prettier；后处理三处（开放枚举、`.loose()`、已知事件字面量与守卫）并断言每处都命中 | 270 | — |
| 3 `packages/protocol` | 包配置、入口 `index.ts`（生成物不计） | 40 | — |
| 4 样例测试 | UNIT-proto-fixtures：Rust 逐个文件解析，事件按严格/宽容两种模式，方法按目录逐个往返（方法类型的分发表约 40 行）；UNIT-ts-fixtures：zod 解析同一批样例，已知事件收窄，未知值不报错 | — | 310 |
| 5 CI 与命令 | `xtask ci protocol`（自己的文件）；`rust` 作业加 iOS 模拟器构建；`jobs/mod.rs` 登记；workflow；根 `package.json` 的 `gen`；ESLint、Prettier 忽略生成物 | 115 | — |
| **合计** | | **530** | **350** |

**总计约 880 行**，在计划的 M（400–900 行）上沿。停止线：每完成一个模块统计一次，按实际偏差推算总数超过约 1,300 行时停下来汇报。

## 实际行数
