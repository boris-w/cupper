# T015 汇报

- 执行者：子 agent（全栈工程师，Rust 协议与 TS 工具链）
- 分支：`build/T015-schema-ts-gen`
- 完成日期：2026-10-19

## 做了什么

T015 已完成，验收标准全部满足。在干净 worktree 里跑 `cargo xtask ci pr`，全部作业通过：

| 作业 | 结果 |
|---|---|
| rust | 581 通过 |
| protocol | 176 通过 |
| ts | 232 通过 |
| license | 1 通过 |

`protocol` 作业不再是"not implemented"。有一点要注意：同一个提交的**第一次** pr 运行失败了，是我自己的验证方式造成的，代码本身没有问题，原因见"遗留问题"第 1 条。

改动共 940 行新增、21 行删除，不含生成物和夹具，比开工前估计的 880 行多约 7%，远低于约 1,300 行的停止线。逐模块的估计与实际见 `notes.md`。

1. **`export-schema`**
   - 位置：`crates/cupper-protocol/src/bin/export-schema.rs`。它调用库里新增的纯函数 `schema::document()`，写出 `protocol/schema/v1.json`。
   - `$defs` 共 341 个定义，包括：事件外壳 `Event`、`EventBody`、全部领域类型、`IndexView`、`SessionView`、`RpcError`。
   - `x-methods` 是三张表：`requests`、`client_notifications`、`host_notifications`。
     - 每个条目带 `x-scope`、`x-idempotent`、`x-milestone`、`x-errors`；门控方法带 `x-builds`。
     - `x-id`、`x-time`、`x-path`、`x-mergeable` 原样保留。
     - 顶层另有 `x-protocol-version: 1`。
   - 写出用新增的 `canonical::to_canonical_pretty`：RFC 8785 的键序和数值、字符串写法，加缩进（见取舍 1）。超过 2^53−1 的整数会让导出直接失败。
   - 这个 bin 是 crate 里唯一做 I/O 的地方；iOS 构建只编库，不含它。
2. **`tools/gen-protocol-ts`**
   - 流程：转成 OpenAPI → hey-api 0.99.0（`@hey-api/typescript` 加 `zod`，非 strict）→ 后处理 → Prettier。
   - 三处后处理：
     - 85 个开放枚举写成 `| (string & {})`。
     - 兜底成员和"接受其他字段"的对象保留未知字段：`UnknownEvent`、`ErrorData` 加 `.loose()`；`UnknownMember` 本身已是 record，不用改。
     - 生成 `events.gen.ts`，内容是：
       - `KNOWN_EVENT_TYPES`（26 个字面量）；
       - 类型 `KnownEventType`、`KnownEventBody`、`KnownEvent`；
       - `isKnownEventType`；
       - 类型守卫 `isKnownEvent`，它按该类型的 zod 校验 `data`，已知类型但数据残缺时返回 false。
   - 每处改写都检查"该命中的地方都命中了"。hey-api 升级后输出变了，生成会直接失败，不会悄悄出错。
   - 连跑两次，生成结果逐字节相同。
3. **`packages/protocol`**（`@cupper/protocol`）
   - `src/gen/` 下三个生成文件入库，`src/index.ts` 负责重导出。
   - 按 T070 的惯例写。生成物由 Prettier 格式化；ESLint 跳过它们，tsc 照常检查。
4. **`protocol/fixtures/`**
   - `events.json` 共 74 例：
     - 26 种事件全覆盖，可选字段缺省的另有一例；
     - 8 例是已知类型里带未知值：开放枚举、未知条目类型、未知请求类型、未知结果类型、未知 prompt 块、未知字段；
     - 3 例未知事件类型；9 例数据残缺；2 例没有外壳或没有标签。
   - `methods.json` 共 92 例，覆盖方法目录里全部 64 个方法和通知。
   - 取值是用临时插桩（没有提交）从各模块现有的 Rust 样例一次性导出的，再补上开放性用例。
5. **测试**
   - UNIT-proto-fixtures，共 5 个：
     - `tests/fixtures.rs` 3 个：事件分别走严格和宽容两种解码，再序列化回来要等于原样例或 `normalized`；只带 `type`、`data` 的事件体也单独解析一遍；方法用一个宏分发到各自的 Params、Result 类型做往返，并核对样例覆盖整个目录。
     - 库内 2 个：检查导出文档完整（所有 `$ref` 都能解析、注解都在），以及带缩进的写法去掉空白后就是 JCS。
   - UNIT-ts-fixtures：`packages/protocol/src/fixtures.test.ts`，168 例：
     - 用 zod 解析同一批样例，结果等于原样例或 `normalized`；
     - `isKnownEvent` 的判定和预期一致；
     - 方法按 `v1.json` 的 `x-methods` 找到对应 zod，逐个解析；
     - 编译期检查：已知事件能按 `type` 收窄；开放枚举的字面量没有被 `string` 吞掉；未知类型赋给 `KnownEventType` 会报错（`@ts-expect-error`）。
6. **`pnpm gen`**：先 `cargo run … export-schema -- protocol/schema/v1.json`，再 `node tools/gen-protocol-ts/src/main.ts`，不依赖平台。
7. **`xtask ci protocol`**
   - 作业在自己的文件 `jobs/protocol.rs` 里，另配 workflow `protocol.yml`（ubuntu，暂不启用）。
   - 步骤：`pnpm install` → `pnpm gen` → 工作区必须仍然干净 → 带 `unit_proto_fixtures` 过滤的 `cargo test -p cupper-protocol`（即 UNIT-proto-fixtures）→ `packages/protocol` 的 Vitest（即 UNIT-ts-fixtures）。
   - 每一步都会执行，一份报告里能同时看到"契约过期"和由此失败的样例。
8. **`xtask ci rust`**：只加了一步 `ios-sim`，即 `cargo build -p cupper-protocol --lib --target aarch64-apple-ios-sim --locked`。

**新增 npm 依赖**（都锁精确版本，前四个只在开发时用）：

| 包 | 版本 | 许可证 | 用在哪里 |
|---|---|---|---|
| `@hey-api/openapi-ts` | 0.99.0 | MIT | gen 工具 |
| `prettier` | 3.9.9 | MIT | gen 工具（与根目录同版本） |
| `typescript` | 6.0.3 | Apache-2.0 | gen 工具（hey-api 的 peer，与根目录同版本） |
| `@types/node` | 24.19.1 | MIT | gen 工具、protocol 包的测试（与 apps/desktop 同版本） |
| `zod` | 4.6.7 | MIT | **`@cupper/protocol` 的运行时依赖**，以后会随 host-client 进 App |
| `vitest` 5.0.3、`vite` 8.3.3 | — | MIT | protocol 包的测试（与其他包同版本） |

- 锁文件新增 43 个包：40 个 MIT；`color-support`、`semver@7.8.4` 是 ISC；`dotenv` 是 BSD-2-Clause。都在 ADR-11 白名单内。
- 依赖闭包里还有 `argparse@2.0.1`，许可证是 Python-2.0，**不在白名单**。它经 hey-api → `js-yaml@4.2.0` 引入；main 上原本就有它（electron-builder 经 `js-yaml@4.3.2` 引入），只在开发时用。

## 关键取舍

1. **`v1.json` 是"加了缩进的 JCS"，不是单行的 RFC 8785 原文。**
   - 键序、数值和字符串的写法都按 RFC 8785，只多了空白；去掉空白就是 JCS 原文，测试核对了这一点。
   - 单行 JCS 约 240 KB，契约每次改动在 diff 里都是一整行，没法审阅，冻结后更是如此。
   - 代价是这个文件由 Rust 自己排版，所以加进了 `.prettierignore`。
   - 如果要求字节级的 JCS 原文，改写出函数一行即可。
2. **后处理用文本改写，并检查每处都命中**，没有用 hey-api 的插件或 `~resolvers`。后者带 `~` 前缀，是不稳定的 API。版本锁死以后，这种文本改写可靠，升级时也会响亮地失败。
3. **hey-api 有四个问题**，前三个在转 OpenAPI 或后处理时修正：
   - 它忽略 `$ref` 旁边的关键字。`open_union!` 内部标签联合的成员因此丢了标签，TS 无法收窄，zod 会剥掉 `kind`、`type`，联合也不再按标签区分。涉及 `ItemStarted`、`InteractionRequest`、`InteractionOutcome`、`Auth`、`Content`、`PromptBlock`、`QueueOp`、`WorkspaceSpec`。改写成 `allOf` 后解决。
   - `true` 子 schema 被丢掉：`policy/describe.params` 这个字段整个不见了。改成 `{}` 后生成 `z.unknown()`。
   - 带 `additionalProperties` 的 `ErrorData`，TS 类型有索引签名，zod 却剥掉了 flatten 进来的细节字段。加 `.loose()` 后保留。
   - 它从自己的安装目录向上查找 tsconfig。设 `tsConfigPath: null` 关掉查找，输出不再随所在目录变化。
4. **兜底成员分两层处理，与 Rust 一致。**
   - 事件以下的嵌套联合：兜底成员只接受联合不认识的标签。已知成员的数据残缺时联合直接失败，对应 `open_union!` 的严格已知标签。
   - 事件层：`zUnknownEvent` 什么都接，于是整个事件降级为未知事件，由 `isKnownEvent` 返回 false，等价于 Rust 的 `Tolerant`。
   - 如果不这样做，"tool_call 缺 `output_state`"这类数据会被当成已知事件、配上一个未知条目放过去，界面会读到缺字段的对象。拿修正前的生成物跑测试，这 4 个样例会失败，`policy/describe` 也会失败。
5. **`Settings.cli_path` 和 `SettingsPatch` 没有收紧。** 两者都在客户端→宿主方向，宿主会拒绝；在 TS 端单独收紧，就要对特定类型做特例后处理，而 schema 本身仍然不精确。更合适的做法是冻结前在 Rust 里改（例如给 `cli_path` 标必填），这样两端一起变。

## 验证方式

- [x] **`cargo xtask ci protocol` 通过**（176 通过）。
  - 反向验证：在临时的分离 worktree 里给 `FallbackKind` 加一个取值并提交，不跑 `pnpm gen`，然后跑作业。作业失败（175 通过、1 失败），并列出 `v1.json`、`types.gen.ts`、`zod.gen.ts` 三处改动。
  - 验证完已删除临时 worktree，本分支上没有那个提交。
- [x] **UNIT-proto-fixtures**：逐个文件解析，往返一致；带未知值的样例按宽容模式降级，不报错。故意把一条残缺样例的预期改成 known，测试会失败。
- [x] **UNIT-ts-fixtures**：已知值得到正确类型，收窄能编译；未知事件类型、开放枚举的未知值、兜底成员都不报错。
- [x] **`xtask ci rust`** 包含 `ios-sim` 一步，并且通过。
- [x] **pr 运行**：在干净的 worktree 里，对提交 `b375346` 跑 `cargo xtask ci pr --record docs/tasks/T015-schema-ts-gen/ci/`。
  - 结果：rust 581、protocol 176、ts 232、license 1，全部通过；testdata-scan、ui、e2e 尚未实现。
  - 摘要是 `docs/tasks/T015-schema-ts-gen/ci/pr-b37534601f10.json`，UNIT-proto-fixtures 和 UNIT-ts-fixtures 两个编号都是 pass，随提交 `d149f5f` 入库。
- [x] 测试名都以编号开头：Rust 用 `unit_proto_fixtures_`；TS 用 `describe("UNIT-ts-fixtures")`。
- 另外，`cargo fmt`、`clippy -D warnings`、Prettier、ESLint、`tsc --build` 都通过。

## 提交列表

```
d149f5f docs(task): T015 pr CI summary and the shared target directory pitfall
b375346 docs(task): T015 actual line counts and the hey-api gaps the fixtures found
115a610 ci: build the cupper-protocol library for the iOS simulator
471f006 ci: add the protocol job
97ca695 test(protocol): shared fixtures checked in Rust and TS
22d9012 fix(protocol): make the generated zod match Rust on malformed and open data
a328977 build(protocol): generate packages/protocol with tools/gen-protocol-ts
4a4eb0c feat(protocol): export the whole schema with the export-schema bin
1f03df8 docs(task): T015 notes with the pre-start probe and line estimates
```

## 遗留问题 / 范围外发现

1. **共用 `CARGO_TARGET_DIR` 会测到别的 worktree 的旧产物。**
   - 经过：反向验证时，临时 worktree 共用了本 worktree 的 `target/`。之后第一次跑 pr，`protocol` 作业用的竟是临时 worktree 编出的 `export-schema`（带着 `card`）、fixtures 测试和旧版 xtask，因而失败。
   - 原因：cargo 对工作区成员的元数据哈希不含绝对路径，依赖信息里记的又是相对路径，于是另一个 worktree 编出的、时间更新的产物在这里被当成最新的。
   - 处理：`cargo clean -p cupper-protocol -p xtask` 之后重跑，全部通过。
   - 建议：计划 1.8 写着"构建缓存可以在 worktree 之间共享"，若用同一个 `CARGO_TARGET_DIR`，会出同样的问题，建议改成只共享 registry 和 sccache 这一类缓存。
2. **残缺事件在两端的行为不完全一致。** `zStreamEventsParams`、`zStreamSnapshotParams`、`zHistoryPageResult` 对数组里每个事件做完整校验：
   - 数据残缺的事件会被接住；
   - 但没有合法外壳的事件（例如缺 `seq`）会让整条消息失败；
   - Rust 的 `Raw<T>` 只让那一条失败。
   - 建议 T071 先把 `events`、`ops` 当作 `unknown[]` 取出，再逐个 `zEvent.safeParse`。
3. **ID 在 schema 里没有 pattern**，所以 zod 会放过前缀错误的 ID，Rust 则会拒绝。这符合"客户端把 ID 当不透明字符串"的约定，样例里没有放这种用例。
4. **其他开放联合在 TS 里无法按标签收窄。** `Item` 的 `kind` 等，因为 `UnknownMember` 带索引签名。要不要给它们也生成类似 `isKnownEvent` 的守卫，可在 T072、T082 定。
5. **可选字段在 TS 里是 `T | null`**（zod 用 `.nullish()`），而宿主从不发 `null`。要不要在冻结前统一改成只是缺省，由 T016 决定。
6. **`UnknownEvent` 加了 `.loose()`**，未知类型事件上的陌生外壳字段 TS 会保留，Rust 会丢掉。没有实际影响，样例避开了这种情况。
7. **设计 5.3.10 只写了三处后处理。** 实际还有四处：`$ref` 旁属性、`true` 子 schema、开放对象 `.loose()`、嵌套兜底只收未知标签（见取舍 3、4）。建议编排者补进设计。
8. **`protocol` 作业还没有对导出文件单独做整数安全检查**（5.1.6 列了这一项）。目前导出时超过 2^53−1 就失败，rust 作业里的单元测试也覆盖了生成器的输出。兼容比对归 T016。
9. **与 T026 的冲突**：`rust.rs` 里我只改了模块文档，并在 clippy 和测试之间插了 `ios-sim` 一步。
10. **给后续任务的交接**
    - T071：从 `@cupper/protocol` 用 `zEvent`、`zEventBody`、`isKnownEvent`、`KnownEvent`。`isKnownEvent` 为 false 时记日志并显示占位。`protocol/fixtures/methods.json` 可以直接给 mock 宿主用。
    - T072：`zIndexView`、`zSessionView` 及对应类型已经生成，两端视图按深度相等比较。
    - T016：`v1.json` 是加了缩进的 JCS，方法目录在 `x-methods` 的三张表里。
    - 所有后续任务：改了协议类型就要跑 `pnpm gen` 并提交生成物，`protocol` 作业会卡住漏提交的情况。
