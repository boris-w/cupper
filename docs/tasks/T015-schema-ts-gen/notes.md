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

按 `git diff --numstat 79fcec4 HEAD` 统计，不含 `pnpm-lock.yaml`、生成物、夹具和 `docs/`。测试指 Rust 的 `#[cfg(test)]` 和 `tests/`，以及 TS 的 `*.test.ts`。

### 模块 1 之后（`4a4eb0c`）

| 模块 | 代码 | 测试 | 估计 |
|---|---|---|---|
| 1 Rust 导出 | 117 | 63 | 105 + 40 |

比估计多约 35 行：`canonical.rs` 的写出函数改成可选缩进，拆出 `separate`、`close` 两个小函数；文档测试顺带核对了全部 `$ref` 都能解析、注解都在。推算总数约 950 行，低于停止线，继续。

### 全部完成之后（`115a610`）

| 模块 | 文件 | 代码 | 测试 | 估计 |
|---|---|---|---|---|
| 1 Rust 导出 | `schema.rs`、`canonical.rs`、bin、`.prettierignore` | 117 | 63 | 145 |
| 2 `tools/gen-protocol-ts` | `main.ts`、`postprocess.ts`、包配置 | 342 | — | 270 |
| 3 `packages/protocol` | 包配置、`index.ts`、Vitest 配置 | 42 | — | 40 |
| 4 样例测试 | `tests/fixtures.rs`、`fixtures.test.ts` | — | 268 | 310 |
| 5 CI 与命令 | `jobs/protocol.rs`、`rust.rs`、`mod.rs`、workflow、`gen` 脚本、ESLint 忽略 | 108 | — | 115 |
| **合计** | | **609** | **331** | **880** |

**总计 940 行新增、21 行删除**，比估计多约 7%，在 M 的上沿之外一点，远低于停止线。生成物（`v1.json` 约 10,300 行，`src/gen/` 约 6,700 行）和夹具（约 2,600 行）不计。

- 模块 2 多出约 70 行：写样例测试时又发现 hey-api 的三处问题（见下节），后处理相应加了三段，每段都带"必须命中"的检查。
- 模块 4 少约 40 行：样例的取值放在 JSON 夹具里，测试只写判定规则；方法按目录逐个分发的表用一个宏写成，比估计紧凑。

## 样例测试发现的 hey-api 问题

开工前的实测只发现了 `$ref` 旁属性被丢掉一处。两端样例跑起来以后又发现三处，都在 `postprocess.ts` 里处理，并由样例覆盖：

1. **嵌套联合的兜底成员什么都收**。`z.union` 按顺序尝试，已知标签的成员数据残缺时会落进 `zUnknownMember`，被当成未知成员；Rust 的 `open_union!` 对已知标签严格解析，这种情况整个事件降级。改为：事件以下的兜底成员只接受联合不认识的标签（`.refine`），已知成员残缺时联合失败，事件层的 `zUnknownEvent` 接住整个事件，`isKnownEvent` 返回 false，与 Rust 的宽容模式一致。事件层的兜底仍然什么都收，否则残缺事件会让 `zEvent` 直接报错。
2. **`true` 子 schema 被丢掉**：`policy/describe.params`（`serde_json::Value`）在 schema 里是 `true`，hey-api 生成的对象里没有这个字段。转 OpenAPI 时改成 `{}`，生成 `z.unknown()`。
3. **带 `additionalProperties: true` 的对象被剥掉其他字段**：`ErrorData` 的 `details` 是 flatten 进来的任意键，TS 类型有索引签名，zod 却是普通 `z.object`。和兜底成员一样加 `.loose()`。

另外，hey-api 会从**自己的安装目录**向上找 tsconfig（实测在 scratch 目录里找到了一个无关的 tsconfig 并报错）。仓库里它会找到根目录的 `tsconfig.base.json`，结果碰巧正确，但输出不该依赖安装位置，所以设 `tsConfigPath: null` 关掉查找。

## 验证"改了类型不重新生成就失败"

在临时的分离 worktree 里给 `FallbackKind` 加一个取值并提交（不跑 `pnpm gen`），共用本 worktree 的构建目录跑 `cargo xtask ci protocol`：`gen` 步骤之后工作区出现 `protocol/schema/v1.json`、`types.gen.ts`、`zod.gen.ts` 三处改动，作业失败（175 通过、1 失败），提示提交重新生成的文件。验证完删除了临时 worktree，本分支没有这个提交。

**教训：临时 worktree 不要和别的 worktree 共用构建目录。** 上面的验证用 `CARGO_TARGET_DIR` 指向本 worktree 的 `target/`。随后在本 worktree 跑 `xtask ci pr`，`protocol` 作业用的竟是临时 worktree 编出的 `export-schema` 和 `fixtures` 测试：`v1.json` 多出了 `card`，测试按临时 worktree 已删除的路径找样例而失败，xtask 也是改名前的旧版本。原因是 cargo 对工作区成员的元数据哈希不含绝对路径，依赖信息里记的又是相对工作区根的路径，于是另一个 worktree 编出的、时间更新的产物在这里被当成最新的。`cargo clean -p cupper-protocol -p xtask` 之后重跑，全部通过。计划 1.8 写着"构建缓存可以在 worktree 之间共享"，用同一个 `CARGO_TARGET_DIR` 时会遇到同样的问题，见汇报的范围外发现。
