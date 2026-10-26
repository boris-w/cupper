# T016 汇报

- 执行者：子 agent（工程师）
- 分支：`chore/T016-contract-freeze`
- 完成日期：2026-10-26

T016 已完成。冻结前定下的三件各自单独提交，每次都跑了 `pnpm gen`；第 4 件由 T088 做过，没有再改。合并 main 后，T036 的代码单独一个提交改成 `CliPath`。冻结本身包括：
- 方向敏感的 `tools/schema-compat`，含 UNIT-schema-compat；
- `protocol/released/v1-m1a.json`；
- `packages/shell-api/api-m1a.d.ts` 和类型断言 `frozen.ts`；
- 导出文件的整数安全检查；
- `xtask ci protocol` 里新增的三步。

在干净的 worktree 里跑 `cargo xtask ci pr --record` 全部通过，摘要已提交。`cargo xtask size` 为 +699 −31（生成物和两个快照不计），停止线 850。

## 做了什么

### 冻结前的改动（逐条）

1. **`CliPath`**（f1d480b）
   - `$defs` 新增 `CliPath`，值是一个路径或 `null`（`null` 表示自动发现）。
   - `Settings` 的两个 `agents.*.cli_path` 改为**必填**，类型为 `CliPath`。
   - `SettingsPatch` 里这两个键可选，`null` 表示清除；其余补丁键随第 2 条变成"可选、不为 null"。
   - TS：`Settings` 的这两个键不再是可选；`CliPath = WirePath | null`。
   - **Rust 端用法**（T036 等照此写）：
     - 构造：`CliPath(Some(WirePath(path)))`；自动发现用 `CliPath(None)`，或 `CliPath::default()`。
     - 读取：`settings.claude_cli_path.0` 是 `Option<WirePath>`，判空用 `.0.is_none()`，取路径用 `.0.as_ref()`。
     - 补丁：`Some(CliPath(None))` 清除路径，`None` 保持不变。
     - JSON：`#[serde(transparent)]`，序列化成路径字符串或 `null`。
2. **可选字段只缺省、不为 `null`**（733bba6）
   - 导出时加了一个变换 `absent_not_null`：222 个非必填属性上的 `null` 去掉。TS 从 `?: T | null` 变成 `?: T`，zod 从 `.nullish()` 变成 `.optional()`。
   - `CliPath` 自己的 `null` 保留；Rust 反序列化仍把 `null` 读作缺省，上行保持宽容。
   - TS 调用点：试验时 11 个 TS 项目 `tsc --build` 为 0 个错误，不需要改代码。
3. **`replayed: true` 的声明**（7fd9d6f）
   - `$defs` 新增 `Replayed`（`{replayed?: true}`）。
   - 方法目录里 25 个 `command` 请求的 `result` 写成 `allOf: [<结果类型>, Replayed]`。
   - 成员名改为 `cupper_protocol::rpc::REPLAYED`，账本引用它。
   - TS 多一个 `Replayed` 类型；`packages/protocol` 的金样例测试改成能读两种写法。
4. **`interaction/respond` 的 `not_found`**：T088 已经做过（89d1f34），有用例，本任务没有改。
5. **合并 main 后改用 `CliPath`**（629a96d）：T036 的 `settings.rs`（默认值、绝对路径校验）和它的单测，以及 `launch.rs` 读 `claude_cli_path` 的那一行。编译器只指出了这些地方。之后 `pnpm gen`，`v1-m1a.json` 仍与导出的 schema 逐字节相同。

**建议补进设计 5.3 的约定**（由你改设计）：
- 可选字段一律只缺省、不为 `null`：schema 里非必填属性不带 `null`，宿主不发 `null`。值本身可以为 `null` 的，在它自己的类型里声明（如 `CliPath`）。上行方向，宿主把 `null` 读作缺省，保持宽容。
- `command` 方法的结果在账本回放时多一个 `replayed: true`。方法目录把这类方法的结果写成 `allOf: [<结果类型>, Replayed]`（5.2.5）。

### 冻结本身

**`tools/schema-compat`（TS，方向敏感）**
- 定义的方向：
  - 入方向（客户端到宿主）：参数、客户端通知；
  - 出方向（宿主到客户端）：结果、宿主通知，以及导出的 `Event`、`EventBody`、`IndexView`、`SessionView`、`RpcError`；
  - 两边都用到的，两套规则都适用；没有被引用到的定义按两个方向算（取严）。
- 判为破坏：
  - 删方法、删定义、删字段；
  - 改方法的 scope 或幂等性；改类型；
  - 出方向：必填改可选；没有兜底的枚举或联合加了取值或分支；
  - 入方向：新增必填字段、可选改必填；删取值或分支。
- 其余算附加：加方法、加可选字段、开放枚举加取值、开放联合加分支，包括**新增一种事件**和**视图里新增成员**。
- 另有 `--integers`：检查导出文件里每个整数都有界、不超过 ±(2^53−1)，并且不带 `format`。

**事件目录是开放的（你的补充要求）**
- `EventBody` 和 `Event` 都是 `anyOf`，最后一个分支是 `UnknownEvent`；开放联合以 `UnknownMember` 兜底，开放枚举以 `{type: string}` 兜底。schema 里没有闭合的枚举。
- `schema-compat` 把"有兜底的联合"当作开放的，所以新增事件种类是附加式。

**快照与断言**
- `protocol/released/v1-m1a.json`：冻结时导出 schema 的逐字节副本，已加进 `.prettierignore`。
- `packages/shell-api/api-m1a.d.ts`：tsc 在冻结时为 `src/shell-api.ts` 生成的声明。
- `packages/shell-api/src/frozen.ts` 让 shell-api 的构建核对两件事：
  - 每个冻结成员在任何层级都还在，并且还接受原来的参数；
  - 每个冻结联合的每个分支都还是今天联合的值。

**接入与配套**
- `xtask ci protocol` 新增三步：`schema-compat`、`integers`、`shell-api-m1a`（shell-api 的构建）。
- `cargo xtask size` 不计 `protocol/released/` 和 `api-m1a.d.ts`，并加了单测。

## 关键取舍

- **为什么在导出时变换**：schemars 1.2 对 `Option` 一律加 `null`，没有设置项可以关掉。在导出时对非必填属性统一去掉 `null`，比逐个字段写属性稳。
- **为什么 `CliPath` 是具名定义**：真正可以为 `null` 的值做成具名定义，外层的 `null` 和值自己的 `null` 才分得开。如果直接用 `Option<Option<_>>`，两层在 schema 里看不出区别。
- **`replayed` 为什么声明在方法目录里**：结果类型里有好几个方法共用的 `Empty`，所以不能逐个结果类型加字段。这些结果都是开放对象，`allOf` 不会矛盾。
- **ShellAPI 断言怎么判**：最初用"今天的接口可以赋给冻结的接口"来判。但联合加分支后，新分支会经 `init()` 的返回值传进 `ShellInfo`，被误判为破坏。改成逐成员核对"还在、旧的调用仍能通过类型检查"，联合单独核对"旧分支都还在"。数组和协议类型（`Handshake`）当作叶子，由协议冻结那边负责。

## 验证方式

- **UNIT-schema-compat**（6 条）覆盖：
  - 方向的划分；
  - 自己比自己、附加式改动通过；
  - 新增事件种类和视图成员通过；
  - 删字段、出方向必填改可选、入方向新增必填或可选改必填，判为破坏；
  - 改类型、闭合枚举加值、开放枚举删值、删方法、改 scope、删定义，判为破坏；
  - 冻结的 schema 自比，以及整数检查。
- **Rust**：`unit_proto_base_schema_optional_fields_are_absent_not_null`；方法注解测试断言 `command` 类的结果带 `Replayed`；设置补丁测试；`xtask size` 测试。
- **用改过的冻结 schema 副本演示 `schema-compat`**：

  | 改动 | 结果 |
  |---|---|
  | 删 `SessionSummary.title` | 破坏，退出码 1 |
  | `SessionCreateResult.session_id` 改为可选 | 破坏，退出码 1 |
  | `SessionSendParams` 新增必填的 `token` | 破坏，退出码 1 |
  | `EventBody` 加 `settings.updated`，`IndexView` 加 `settings`，新增方法 `settings/watch` | 附加，退出码 0 |

- **ShellAPI**：逐项试过。删成员、删嵌套成员、改参数类型、删联合分支、收窄分支，都让 `frozen.ts` 报类型错误；加成员、给联合加分支，通过。
- **从冻结前的 schema 到现在**：`schema-compat` 报 247 处破坏，全部来自本任务冻结前的三件改动，符合预期。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T016-contract-freeze/ci/`，摘要 `pr-629a96d7ed26.json` 已提交。跑的期间没有往工作区写文件。

  | 作业 | 结果 |
  |---|---|
  | rust | 2,159 通过 |
  | protocol | 188 通过（含新增三步） |
  | ts | 483 通过 |
  | license | 5 通过 |
  | testdata-scan | 3 通过（本机没有敏感词表，那一步跳过） |
  | ui | 35 通过 |
  | e2e | 30 通过 |

  - 合并 main 之前那次，失败的是 prettier 检查冻结副本，改法是把 `protocol/released/` 加进 `.prettierignore`。
  - 合并之后第一次，只有 `test-shell-api` 因为"Cannot allocate memory (os error 12)"没能启动 vitest，是机器当时的资源问题，单独跑 14 项全过。重跑全部通过。
- **试合并**：用 `git merge-tree` 试合并最新的 main，没有冲突。
- **测试编号**：UNIT-schema-compat 在 `ids.toml` 的 T016 段里，测试名以它开头。

## 提交列表

```
9aa4fdc docs(T016): record the ci pr summary and the CliPath follow-up
629a96d fix(host): the settings and the Claude launch read the CLI paths as CliPath
0782442 Merge branch 'main' into chore/T016-contract-freeze（编排者合并）
4070317 chore(protocol): keep prettier off the frozen schema
a059d65 docs(T016): record the ruling, the size per commit and the checks shown
e283f4c ci(protocol): compare with the frozen contract and check the integers
9664161 feat(shell-api): freeze the ShellAPI as api-m1a.d.ts, checked by its types
0bfa85f feat(tools): schema-compat checks the schema against the released one
2095c67 chore(protocol): freeze the schema as released/v1-m1a.json
7fd9d6f feat(protocol): declare replayed: true on the results of command methods
733bba6 feat(protocol): an optional field is absent, never null
f1d480b feat(protocol): Settings names each CLI by a CliPath, always present
f2ff5ae docs(T016): record the checks, the four decisions proposed and the estimate
```

## 遗留问题 / 范围外发现

- **`Option` 字段要带 `skip_serializing_if`**："可选字段只缺省"依赖这一点。现在只是约定，没有自动检查：漏写的字段会发出 `null`，而 schema 说它不能为 `null`，TS 的 zod 会拒收。可以加一个守卫测试，例如序列化每个金样例后断言没有 `null` 落在非必填属性上。
- **`schema-compat` 的边界**：
  - 不比较描述、数值界的放宽、`x-errors` 的增减，也不比较 `x-milestone`；
  - 没有被引用到的定义按两个方向从严判。
- **ShellAPI 断言的边界**：回调的参数类型不核对，因为回调参数是双变的；回调里收到的联合由联合检查负责。数组和协议类型当作叶子。
- **规则落档**："冻结后改契约只能附加，破坏性改动按 ADR-04 升主版本"，需要你写进 `docs/00-process.md` 或交接。
