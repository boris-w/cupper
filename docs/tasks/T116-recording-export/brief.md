# T116 录制导出

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师）。T043、T055、T008 的执行者接手，因为要改的原始帧写入在它写的运行时里
- 分支：`feat/T116-recording-export`
- 依赖：T047（合并中）。T064（兼容套件框架）依赖本任务。**本任务在剩余关键路径上，没有松弛**：T116 → T064 → T066 → T099

## 背景

T047 开工前拆成三份（计划 v1.3.21）：T047 做了静止屏障和 `test/*`；本任务做 `diag/recording_export`，把宿主自己驱动产生的原始帧导出成录制格式 v1，供兼容套件（T064、T066）产出 B 层录制（设计 5.9.3 的流程图：`raw.db` → `diag/recording_export` → `tools/sanitize` → `testdata/`）。

## 范围

**做**（T047 补充裁定第 2、3 条；设计 5.9.3、5.9.9）：
1. **`diag/recording_export`**（只在 test-harness、dev-auth 构建里登记）：从 `raw.db` 导出录制格式 v1，逐行写出用 `Header::to_line`、`Frame::to_line`（`cupper-protocol::recording`，T052）；`raw.db` 的 `ts` 换算成段内相对时间；按适配器实例分段，每段以 `instance_started` 开头；参数形状按 T013 已定的类型（实现时可细化）。**宿主导出未脱敏的录制**，由调用方入库前跑 `tools/sanitize`（不在宿主里移植规则）。登记后删掉 UNIT-unsupported-set 里的 `Until("T116")`，并在 `supervisor/harness.rs` 旁登记（T047 的交接）。
2. **`stderr` 和 `note` 写进原始帧**：适配器的 stderr 逐行写一帧（`dir: stderr`，结构化脱敏）；实例启动和退出各写一条 `note`（`instance_started`、`instance_exited`，带退出状态）。stderr 仍同时写进 `AdapterLog`（T028）。原始帧的 `Dir` 目前只有 `send`、`recv`、`recv_nonjson`（T050 的交接）。
3. **静止屏障**：原始帧不持有 `Hold`（T047 的交接）；IT 需要等原始帧落盘时，用事件或轮询 `raw.db` 的方式做屏障，写进 `notes.md`。

**不做**：兼容套件本身（T064）；`tools/sanitize` 的规则改动（T052 的工具已够用）。

## 验收标准

- [ ] IT-recording-export：真实宿主进程加假 agent 跑一个回合（含一次 stderr 输出），经 `diag/recording_export` 导出；导出的文件能被格式库 `Recording::parse` 读回、分段正确、`instance_started`/`instance_exited` 齐全；用 node 跑 `tools/sanitize` 的脱敏和扫描通过。
- [ ] 发布构建不登记这个方法（UNIT-release-features 照常通过）。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T116 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T116-recording-export/ci/` 通过，摘要随任务提交。

## 注意

- 不运行真实的 Claude Code CLI；用例用假 agent。
- 导出的录制是未脱敏的，用例产生的临时文件用完即删，不入库。
- 代码约束：每条事件流只有一个写者；原始帧写线程照常是 `raw.db` 唯一的写者。

## 规模

计划估为 M，T047 的执行者估约 370–500 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 700 行（包括贴线）时，必须先停下来汇报**。

## 参考资料

- 设计：`09-testing.md` 5.9.3、5.9.9；`03-protocol-v1.md`（`diag/recording_export`）；`06-agent-driver.md` 5.6.10（线路分接）。
- `docs/tasks/` 下 T047、T052、T050、T028 的 `notes.md`；`crates/cupper-protocol/src/recording.rs`；`tools/sanitize/`。

## 工作方式

T047 合并后，T008 的分支也会再合一次 main。T008 合并之后，在原来的 T043 worktree 里从最新的 main 开新分支：`git checkout -b feat/T116-recording-export main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
