# T013 协议方法 II：git、文件、设置、认证、诊断与测试

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T013-protocol-methods-ext`
- 依赖：T012（已合并）。M1a 第 3 波，**在关键路径上**：T012 → T013 → T031 → T033 → …。T015、T031 都依赖它。

## 背景

宿主协议的方法只定义类型，不含实现（ADR-04）。T012 已经搭好方法的声明方式：
- `method!` 宏；
- `MILESTONE`、`ERRORS` 常量；
- 方法目录 `methods::catalog`；
- 跨组测试：往返、权限矩阵、`command_id` 与幂等、整数上界。

T012 自己做了握手与生命周期、流与内容两组。本任务做 5.3.4 的最后两组。与此同时，T018 在做项目、agent、会话和交互三组。

## 范围

**做**（计划 T013 一行；设计 5.3.4 后两组）：
1. **Git、工作区与文件**（M1b）：
   - `git/status`、`git/diff`（含 `caveats`）、`git/file_diff`；
   - `git/stage`、`git/unstage`、`git/revert`（来自 window 角色时必须带 `input`，与 `interaction/respond` 相同）；
   - `git/commit`、`git/push`、`git/branches`；
   - `worktree/check`、`files/search`、`workspace/open`。
2. **设置、认证、诊断、测试**：
   - `settings/get`、`settings/update`；
   - `auth/elevate`、`auth/elevation_get`、`auth/elevation_respond`；
   - `policy/describe`；
   - `diag/export`、`diag/recording_export`；
   - `test/*`，见 5.2.16、5.9.9。
3. **按构建特性门控的方法**：`diag/recording_export` 只在 `test-harness` 和 `dev-auth` 构建里有，`test/*` 只在 `test-harness` 构建里有。要在 schema 里标出这一点。注解的形式由你定，写进 `annotations.rs` 的文档表格。
4. **`worktree/check` 和 `session/archive` 共用的阻塞原因枚举**，由本任务定义：
   - 取值对应 ADR-10"清理规则"里的 7 条，一条一个取值；
   - 名字和放在哪个文件由你定，放在 git 组里即可，在汇报里写明；
   - T018 的 `session/archive.blockers` 会复用它（5.3.11 末尾"T012 开工前的裁定"）。

**不做**：
- 项目、agent、会话、交互（T018，并行进行中）；
- 任何方法的实现；
- schema 导出（T015）。

## 验收标准

- [ ] 新增 UNIT-proto-methods-ext：
  - 本任务每个方法的参数和结果都能往返序列化，取值用 5.3.4 里的典型值；
  - 方法级 scope 与 5.8.3 的权限矩阵逐格一致（往 `MATRIX` 加行，每行注明出处）；
  - 门控方法的标注能在 schema 里查到。
- [ ] T012 的跨组测试（`catalog.rs`）在加入本任务的方法后仍然通过。
- [ ] 仍能编译到 `aarch64-apple-ios-sim`。在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T013-protocol-methods-ext/ci/` 通过，摘要随任务提交。
- [ ] 本任务新增的测试，名字以 `unit_proto_methods_ext_` 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **接入方法的步骤**：详见 `docs/tasks/T012-protocol-methods-core/notes.md` 中"给 T018、T013 的说明"一节。
  - 新建组文件，用 `method!` 声明，并提供 `register`；
  - `methods/mod.rs` 加一行 `mod`，`catalog()` 加一行；
  - 在组文件的测试模块里写 `samples()`，并加进 `catalog.rs` 的覆盖测试；
  - 按 5.8.3 往 `MATRIX` 加行；
  - 参数必须是具名结构体；
  - 类型按完整的方法名命名，例如 `git/file_diff` 对应 `GitFileDiff`。（T012）
- **用例编号的归属**（编排者裁定）：
  - `catalog.rs` 里的跨组测试属于 T012 的 UNIT-proto-methods-core-stream，不改名，它们会自动检查你加的方法。
  - 你另写以 `unit_proto_methods_ext_` 开头的测试，用共享的测试辅助函数检查本组的样例和矩阵行。
- **`git/diff`** 复用 T017 的 `DiffStat`、`ExcludedPath`、`SnapshotStatus`。（T017）
- **惯例**：
  - `lib.rs` 只放 `mod` 声明；
  - 用 `ids` 里的前缀 ID 类型；
  - 开放枚举用 `open_enum!`，开放联合用 `open_union!`；
  - 整数不超过 2^53−1。

## 并行注意

T018 同时在往 `methods/mod.rs`、`catalog()`、`catalog.rs` 的覆盖测试和 `MATRIX` 里加行。你在这些地方只做附加，合并冲突由编排者处理。其余文件放在你自己的组文件里。

## 规模

计划估为 M（400–900 行）。**开工前先在 `notes.md` 里按方法组写行数估计，测试单独列一项。每写完一组就统计一次实际行数**，不要等全部写完（守则第 8 条；T012 在这一点上没做到）。预计会明显超出约 1,500 行时，先停下来汇报。

## 参考资料

- 设计：`03-protocol-v1.md` 5.3.4（后两组）、5.3.11（末尾"T012 开工前的裁定"）；`08-security.md` 5.8.2、5.8.3；`02-host.md` 5.2.10、5.2.16；`09-testing.md` 5.9.9；ADR-10（清理规则）。
- T012 的代码、`notes.md` 和汇报（`crates/cupper-protocol/src/methods/`）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料（`notes.md` 等）由执行者在任务分支上提交。
