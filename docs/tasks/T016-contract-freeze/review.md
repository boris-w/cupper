# T016 评审

- 评审人：编排者
- 日期：2026-10-26
- 结论：**通过，合并**

## 核对

- **规模**：+699 −31（生成物和两份冻结快照不计），停止线 850 以内。
- **CI**：执行者在干净的 worktree 里跑的摘要 `pr-629a96d7ed26.json` 全部通过（rust 2,159、protocol 188、ts 483、license 5、testdata-scan 3、ui 35、e2e 30）。合并后第一次 `test-shell-api` 因"Cannot allocate memory"没能启动，是当时机器资源不够，重跑全部通过。之后 main 只多了一个文档提交，直接合并。
- **schema-compat 的演示**：用改过的冻结副本试了删字段、出方向必填改可选、入方向新增必填（都判为破坏，退出码 1），以及新增事件种类、视图成员、方法（判为附加，退出码 0）。新增 `settings.updated` 和 `IndexView.settings` 按附加通过，与 T036 的裁定一致。
- **`skip_serializing_if` 的覆盖**：编排者查了 `cupper-protocol` 里的 `Option` 字段，线上类型都带 `skip_serializing_if`；没带的只有不上线的 `Decoded.degraded`、`MethodError.reason`，读入用的 `RawMessage`，以及 JSON-RPC 错误信封的 `id`（按 JSON-RPC 本来就可以为 `null`）。

## 逐项意见

1. **`CliPath` 做成具名定义**：外层"缺省"与值本身的 `null`（自动发现）分得开，`Option<Option<_>>` 在 schema 里看不出两层。补丁里 `Some(CliPath(None))` 清除、`None` 不变，语义清楚。
2. **在导出时去掉 `null`**：schemars 1.2 没有开关，导出时统一变换比逐个字段写属性稳；Rust 读入仍把 `null` 当缺省，上行保持宽容。
3. **`replayed` 声明在方法目录**：结果类型里有多个方法共用的 `Empty`，在方法目录里写成 `allOf: [<结果类型>, Replayed]` 是对的。
4. **schema-compat 按方向判**：方向由引用推出，没有被引用到的定义两个方向都算、从严判；有兜底分支的联合当作开放，新增事件是附加式。
5. **ShellAPI 断言**：从"整体可赋值"改成逐成员核对"还在、旧调用仍能通过类型检查"，联合单独核对"旧分支都还在"，避开了联合加分支被误判为破坏。
6. **整数检查**：导出文件里每个整数都有界、不超过 ±(2^53−1)、不带 `format`，落实了代码约束。

## 记下的点

- **守卫测试**："可选字段只缺省"现在靠约定。哪天漏写了 `skip_serializing_if`，宿主就会发出 `null`，zod 会拒收。需要一个守卫：序列化金样例后断言非必填属性上没有 `null`。另立小任务。
- **设计与流程**：编排者把两条约定写进设计 5.3.1、5.3.10：可选字段只缺省；`command` 方法的结果在回放时带 `replayed: true`。冻结规则写进 5.12.5 和 `docs/00-process.md`：冻结后改契约只能附加，破坏性改动按 ADR-04 升主版本。
- **schema-compat 的边界**：不比较描述、数值界的放宽、`x-errors` 的增减、`x-milestone`。记进交接，以后有需要再加。
