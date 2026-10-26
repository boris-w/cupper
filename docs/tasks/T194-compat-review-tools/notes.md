# T194 工作笔记

## 开工前的核对

- 任务书；T188 的 `notes.md`（"打算的做法"第 3、4 条）；设计 5.9.5（"升级适配器时的评审"）、5.9.10；`handoff-notes.md` 第一列含 T188 或 T194 的行（T002：`--compat`；T188：报告的 schema 在 `testdata/compat/report.schema.json`）。
- 报告：`{v, commit, mode, agent, adapter, cli: {name, version}, scenarios: [{id, title, outcome, …}], …}`；`mode` 是 `fake`、`real_config`、`isolated_config`；结果现在是 `pass`、`fail`、`inconclusive`，T187 会加 `not_applicable`、`observed`、`real_only`（编排者：这三种和不认识的取值都不让编号完成，不认识的也不能当作通过）。
- 录制：首行是头，其余每行 `{seq, t, instance, dir, msg | data | note}`。
- `trace-ids`：登记表 `registry.ts`（字段白名单 `FIELDS`）、结果 `results.ts`（`pass`、`fail`、`skip`）、判定 `evaluate.ts`（只有跳过时是 problem）、命令行 `main.ts`（`--milestone` 只读已提交、未改动、同一提交的摘要）。
- `ts` 作业对 `tools/*` 下每个带 `test` 脚本的包跑 Vitest，新包照 `tools/sanitize` 建就会接上。

## 做法

1. **`tools/rec-shape-diff [--json] <旧录制> <新录制>`**：六类形态各是一个字符串集合：
   - 方法，带方向（`send initialize`、`recv session/update`）；
   - update 种类（`session/update` 的 `sessionUpdate`）；
   - `_meta` 键路径，带方法和所在的 params、result、error，数组下标记成 `[]`，只记键不记值；
   - `option_id`，带 kind；
   - `stopReason`；
   - 错误码，带方法。
   - 应答按 `instance`、方向和 `id` 对回请求的方法；`note`、stderr、非 JSON 行跳过。
   - 输出每类的增删；默认是给人看的摘要，`--json` 输出机器可读的报告。没有差异退出码 0，有差异 1，用法错误 2。
2. **`trace-ids --compat <报告>`**：
   - `ids.toml`：COMPAT 编号写成 `{ suite = "compat", scenario = "…", cli = "claude" | "codex" }`。规则：两个字段只能写在 COMPAT 编号上；套件为 compat 的 COMPAT 编号必须写；`(cli, scenario)` 不重复。
   - 读报告：`mode` 是 `fake` 时报错；不是两种真实模式之一也报错。每个剧本按 `(cli.name, id)` 找编号：`pass` 记通过、`fail` 记失败，其余（`inconclusive`、T187 的三种、不认识的取值）都记作跳过，跳过时判定为 problem；`inconclusive` 的说明提示重跑。
   - 没有登记的剧本单独列一节，不判失败。
   - `--milestone` 允许同时给 `--compat`，前提是报告已提交且未改动；报告里的提交不要求与摘要相同（兼容报告是在另一次运行里写的）。

## 规模估计（开工前）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `rec-shape-diff` 的包 | `package.json`、`tsconfig.json`、`vitest.config.ts` | 30 | — |
| 形态的提取与对比 | 六类、对回应答、增删 | 90 | 90 |
| 命令行 | 读两份录制、摘要与 `--json`、退出码 | 35 | — |
| `trace-ids` 的登记表 | `scenario`、`cli` 两个字段与三条规则 | 30 | 40 |
| 读报告 | 模式、结果映射、未登记的剧本 | 45 | 50 |
| 命令行 | `--compat`、`--milestone` 的放行、未登记一节 | 25 | — |
| `ids.toml` | 6 个 COMPAT 编号改成表 | 6 | — |
| 合计 | | 261 | 180 |

合计约 **440**，停止线 560，硬停止线 640。每写完一个模块跑一次 `cargo xtask size`，记在下面。

## 按模块统计
