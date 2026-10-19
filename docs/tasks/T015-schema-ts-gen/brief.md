# T015 schema 导出与 TS 生成链

- 派发日期：2026-10-19
- 执行者：子 agent（全栈工程师，Rust 协议与 TS 工具链）
- 分支：`build/T015-schema-ts-gen`
- 依赖：T013、T014、T018（均已合并）。M1a 第 4 波，**零松弛**：桌面链 T015 → T071 → T075 → … 从这里开始。T016（契约冻结）也依赖它。

## 背景

协议以 Rust 类型为源（ADR-04）。T010–T018 已经定义了全部类型：
- 事件、领域类型、视图；
- 方法和方法目录（含 `SCOPE`、幂等、`x-milestone`、`x-errors`、`x-builds` 等注解）。

本任务建立导出与生成链（5.3.10）：

> Rust（serde + schemars）→ `protocol/schema/v1.json` → `tools/gen-protocol-ts`（hey-api 生成 TS 类型和 zod，非 strict）→ 后处理 → `packages/protocol`

从这个任务起，生成物一致性检查（`xtask ci protocol`）一直开着：Rust 类型一改，生成物就必须跟着重新生成并提交。

## 范围

**做**（计划 T015 一行；设计 5.3.10、5.1.5、5.1.6）：
1. **`export-schema`**：`cupper-protocol` 里的 bin，不进库，也不进 iOS 构建（5.1.1）。它完整输出 `protocol/schema/v1.json`，包括：
   - 事件和全部领域类型；
   - 视图 `IndexView`、`SessionView`（测试 #16 要比对两端的视图 JSON）；
   - 方法目录，三张表都要；
   - 各种注解（`x-scope`、`x-idempotent`、`x-milestone`、`x-errors`、`x-builds`、`x-id`、`x-time`、`x-path`、`x-mergeable` 等）。
   - 输出是规范化的 JSON（RFC 8785），不依赖 map 的键顺序。
2. **`tools/gen-protocol-ts`**：用 hey-api 生成 TS 类型和 zod（非 strict），再做三处后处理：
   - 已知事件类型的字面量列表和类型守卫；
   - 兜底成员用 `.loose()`；
   - 开放枚举写成 `(string & {})`。
3. **`packages/protocol`**：存放生成物，并提交入库。
4. **`protocol/fixtures/`**：两端共用的样例，包括带未知值的样例，用来验证开放性。
5. **`pnpm gen`**：一条命令完成导出和生成（`CLAUDE.md` 的常用命令）。
6. **`xtask ci protocol`**：
   - 重新导出、重新生成，与入库的生成物比对，有差异就失败；
   - 跑两端的样例测试。
   - 这个作业从此一直开着。
7. **补上 CI 漏掉的一步**：在 `xtask ci rust` 里加 `cargo build -p cupper-protocol --lib --target aarch64-apple-ios-sim`（5.1.6，T001 时漏掉）。

**不做**：
- 兼容比对与契约冻结（T016）；
- host-client（T071）。

## 验收标准

- [ ] `cargo xtask ci protocol` 通过。改动一个 Rust 类型而不重新生成，这个作业会失败：要验证这一点，验证完再改回来。
- [ ] 新增 UNIT-proto-fixtures：Rust 端逐个解析 `protocol/fixtures/` 的样例，往返一致；带未知值的样例按宽容模式降级，不报错。
- [ ] 新增 UNIT-ts-fixtures：TS 端用生成的 zod 解析同一批样例。已知值得到正确的类型；未知事件类型、开放枚举的未知值、兜底成员都不报错。
- [ ] `xtask ci rust` 包含 `aarch64-apple-ios-sim` 的构建步骤，并且通过。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T015-schema-ts-gen/ci/` 通过，`protocol` 作业不再是"not implemented"，摘要随任务提交。
- [ ] 测试名以用例编号开头：Rust 用 snake_case；TS 的标题以编号原文开头（`ids::owner`，见 T070 的交接）。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **TS 包的惯例**（T070）：
  - `exports` 直接指向 `src/*.ts`；
  - 类型检查走 project references；
  - Vitest 只收 `src/**/*.test.ts`；
  - 包的 `test` 脚本必须运行 Vitest，作业会在末尾补上 reporter 参数，不要另起脚本名。
- **schema 名**：
  - `Event<EventBody>` 和 `Raw<Event<EventBody>>` 的 schema 名就是 `Event`；
  - 泛型下 schemars 会自动生成名字，需要时用 `rename` 控制。（T010、T011、T012）
- **`UnknownMember`**：schema 是任意对象，后处理时用 `.loose()`。`Item`、`InteractionRequest`、`InteractionOutcome` 的兜底成员都是它。（T011、T017）
- **特殊形状**：
  - `ItemStarted` 是外壳属性加 9 个成员的 `anyOf`；
  - `ItemUpdated` 展开 `ItemPatch` 后共 21 个属性；
  - `AnswerValue` 是不带标签的联合（`anyOf`）。（T017）
- **方法目录**：用 `methods::catalog::catalog(&mut generator)` 导出，再 `take_definitions(true)`；方法条目可能带 `x-builds`，要保留。（T012、T013）
- **视图**：`IndexView`、`SessionView` 要导出进 `v1.json`。（T014）
- **两处 schema 不够精确**：
  - `Settings.cli_path` 实际总会出现，值可为 `null`，但 schema 里不是必填；
  - `SettingsPatch` 的非空键在 schema 里也允许 `null`。
  - 生成 TS 时可以考虑收紧；不收紧的话，在汇报里说明。（T013）
- **`cupper-protocol` 的惯例**：`lib.rs` 只放 `mod` 声明；用 ID 类型、`TimestampMs`、`WirePath`，注解会自动带上。（T010）

## 注意

- **依赖**：hey-api 等 npm 依赖锁精确版本。
  - 许可证必须在 ADR-11 的 allowlist 内，汇报里列出新增的依赖和许可证。
  - 这些依赖只在开发时使用，不进 App。
- **并行**：T026 正在改 `xtask ci rust`（加 shuttle 的步骤），可能会冲突，合并时由编排者处理。你只在 rust 作业里加 iOS 构建这一步，protocol 作业放在它自己的文件里（1.5："xtask ci 的每个作业一个文件"）。
- **生成物**：不计入规模（计划 1.1）。

## 规模

计划估为 M（400–900 行，不含生成物）。**开工前先在 `notes.md` 里按模块写行数估计，测试和测试辅助单独列一项**；动手后每完成一个模块统计一次（守则第 8 条）。预计会明显超出约 1,500 行时，先停下来汇报。

## 参考资料

- 设计：
  - `03-protocol-v1.md` 5.3.10（兼容与代码生成）、5.3.11；
  - `01-repo-build-ci.md` 5.1.1、5.1.5、5.1.6（`protocol`、`rust` 作业）；
  - `09-testing.md` 5.9.1（测试 #16）。
- ADR-04、ADR-11；SP-4a 的报告（后处理的三处）。
- T010–T018 的代码和汇报。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料（`notes.md` 等）由执行者在任务分支上提交。
