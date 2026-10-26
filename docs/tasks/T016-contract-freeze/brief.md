# T016 契约冻结

- 派发日期：2026-10-26
- 执行者：子 agent（工程师）。T052、T053、T095、T076、T009、T165、T168、T067、T176、T178、T078 的执行者接手（窗口桥与 ShellAPI 的数据连接、录制格式都是它做的）
- 分支：`chore/T016-contract-freeze`
- 依赖：T077、T134、T135、T078、T082、T166、T083、T085（都已合并；K2 已达标，不再等 T079）。T099 依赖本任务

## 背景

协议和 ShellAPI 的契约在 M1a 冻结（设计 5.12.5，计划 P-1）。冻结之后 M1b 只能附加：加方法、事件、可选字段和开放枚举的取值，不改已有的契约。冻结靠一个方向敏感的兼容检查守住，接进 `xtask ci protocol`：破坏性改动让 CI 失败，附加式改动通过。

## 范围

**做**（计划 T016 一行；设计 5.12.5）：
1. **冻结前要定的几件**（交接里点名给本任务）：
   - **可选字段的表示**：TS 里现在是 `T | null`（zod `.nullish()`），宿主从不发 `null`。要不要统一改成只是缺省（T015 的交接）。
   - **`Settings.cli_path`** 标必填；`SettingsPatch` 的非空键不接受 `null`。在 Rust 里改，两端一起变（T015 的交接）。
   - **`replayed: true`**：账本回放的结果带这个字段（5.2.5），schema 里还没有声明，要定声明方式（T032 的交接）。
   - **`interaction/respond` 的 `x-errors`** 补上 `not_found`：会话里不存在的请求 ID 目前回 `invalid_params`，而 `interaction/get` 回 `not_found`（T042 的交接）。
   
   每一件给出方案和理由。会影响 TS 端用法的（例如 `null` 改成缺省），列出要改的调用点，先汇报给编排者定，再动手。定下来的改动各自单独提交，在本任务里重新生成 `packages/protocol`（`pnpm gen`），在汇报里逐条列出改了什么。
2. **`tools/schema-compat`**（方向敏感）：
   - 比较 `protocol/released/v1-m1a.json` 与当前导出的 schema；
   - 按方向判定：下发方向（宿主到客户端）的必填改可选、删字段算破坏；上行方向新增必填算破坏；开放枚举加取值、加可选字段、加方法算附加。
3. **`protocol/released/v1-m1a.json`**：冻结时的 schema 快照，入库。
4. **`packages/shell-api/api-m1a.d.ts`** 与类型断言：ShellAPI 的冻结快照，删成员或改签名让类型检查失败。
5. **接入 `xtask ci protocol`**。
6. **整数安全检查**：`protocol` 作业还没有对导出文件单独做整数安全检查（5.1.6），导出时超界会直接失败。冻结时一并补上（T015 的交接）。

**不做**：上面第 1 条之外的契约内容修改。

## 验收标准

- [ ] 破坏性改动的样例让 `xtask ci protocol` 失败：删字段、下发方向必填改可选、上行新增必填、ShellAPI 删成员。附加式改动通过。
- [ ] UNIT-schema-compat：方向判定的各类情形。
- [ ] 冻结前改动的清单写在汇报里，每条单独提交。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T016 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T016-contract-freeze/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T016 的行；开工前逐行读一遍）

- T015：可选字段的表示；`Settings.cli_path`；`v1.json` 是加了缩进的 JCS，方法目录在 `x-methods` 的三张表里；整数安全检查。
- T032：`replayed: true` 的声明。
- T042：`interaction/respond` 缺 `not_found`。
- 计划 P-1 与 handoff 第 68 行一带：冻结前修改契约要单独提交、在同一任务里重新生成、汇报里列出改了什么。

## 注意

- 导出给其他语言的产物不依赖 serde_json 的 map 键顺序；协议里的整数不超过 2^53−1。
- 冻结之后，后续任务改契约只能附加。规则写进 `docs/00-process.md` 或交接，由编排者合并时补。

## 规模

- 停止线：850；硬停止线：950

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计（生成物不计）；每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b chore/T016-contract-freeze main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
