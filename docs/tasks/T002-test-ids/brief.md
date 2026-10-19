# T002 用例编号登记与 trace-ids

- 派发日期：2026-10-19
- 执行者：子 agent（工程师，CI 与工具）
- 分支：`build/T002-test-ids`
- 依赖：T001（已合并）。T052（录制格式与回放）依赖它，T099（M1a 退出验收）用它。

## 背景

设计和计划给每个验收用例都定了编号（UNIT-、IT-、PROP-、GOLD-、E2E-、PERF-、MAN- 等），测试名以编号开头（Rust 用 snake_case，TS 用编号原文）。`xtask ci` 已经在摘要里按编号列出结果。

本任务做两件事：
- 把全部编号登记到一处；
- 写一个工具，核对每个编号是否在登记的套件里执行并通过，并能推导出待完成的清单（5.9.10、计划 1.6）。

## 范围

**做**（计划 T002 一行；设计 5.9.10；计划 1.6）：
1. **`tests/ids.toml`**：登记设计和计划里的全部编号。
   - 每个编号的字段：里程碑、产出任务、套件（rust、ts、e2e、man 等）、子编号、状态（含"不适用"、"缺口（用户已确认）"）。
   - 按任务分段，每个任务只追加自己那一段（计划 1.5）。
   - 已经合并的任务，按它们实际产出的编号登记，包括拆分后新增的编号，见交接事项。
2. **`tools/trace-ids`**：
   - 编号精确匹配；
   - 读测试报告，确认编号在登记的套件里执行并通过；
   - 读 cargo-nextest 的 JUnit 报告：目前 CI 用的是 libtest 输出，改用 nextest 时一并处理，见交接事项；
   - 读 Playwright 的 JSON 报告：e2e 还没有，先按格式实现并用样例测试；
   - 里程碑退出时，改读入库摘要里的逐编号结果清单；
   - 核对 MAN 证据的日期与版本；
   - 支持"缺口（用户已确认）"状态；
   - 自动推导待完成清单。
3. **`xtask ci testdata-scan`**：按 5.1.6 实现这个作业里属于本任务的部分，即编号登记的一致性检查。PII 扫描归 T052。

**不做**：
- PII 扫描（T052）；
- 兼容报告的校验。

## 验收标准

- [ ] `cargo xtask ci testdata-scan` 通过，不再是"not implemented"。
- [ ] 新增 UNIT-trace-ids，以下几种情况都要报错：
  - 有编号没有测试；
  - 测试被跳过；
  - 套件不符；
  - MAN 证据过期；
  - 子编号没有全部出现。
- [ ] 目前 main 上已有的编号全部登记；`tools/trace-ids` 对当前 main 的 CI 摘要能给出正确的已完成和待完成清单。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T002-test-ids/ci/` 通过，摘要随任务提交。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **结果解析**（T001、T070）：
  - 逐编号结果目前解析的是 libtest 的输出（`crates/xtask/src/ci/ctx.rs` 的 `parse_libtest`），改用 nextest 的 JUnit 报告时，替换这个函数；
  - ts 作业已经接上 Vitest 的 JSON reporter（`parse_vitest`）；按编号匹配时，JS 测试的标题要以编号原文开头（`ids::owner`）。
- **缺口编号**（T021）：IT-sys-peer-macos14、IT-sys-peer-macos15 登记为"缺口：本机替代不了，里程碑退出时由用户确认"；本机跑出的结果只记作 IT-sys-peer-current。
- **拆分和实现中新增的编号**：
  - UNIT-proto-events-index、UNIT-proto-events-session（T011 拆分）；
  - UNIT-proto-methods-core-stream、UNIT-proto-methods-core-session（T012 拆分）；
  - UNIT-queue-core（T040 拆分）；
  - UNIT-cli-client（T045）；
  - T014 的 IndexView 用例和订阅规则暂时归在 UNIT-view-window 下；
  - 以计划 v1.3.6 的任务表为准。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 并行注意

T033、T039、T041、T071 正在进行，它们会新增用例，但不会改 `tests/ids.toml`。本任务合并之后，后续任务各自往 `tests/ids.toml` 里追加自己那一段。xtask 的改动放在 `testdata-scan` 作业自己的文件里。

## 规模

计划估为 M（400–900 行），`tests/ids.toml` 是数据，不计入。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就统计一次**（守则第 8 条）。**预计会超出约 1,500 行时，必须先停下来汇报**。

## 参考资料

- 设计：`09-testing.md` 5.9.10（测试编号与人工验证清单）及各节的编号；`01-repo-build-ci.md` 5.1.6。
- 计划：1.5、1.6、1.8、第 2 节的任务表（各任务的验收编号）。
- `crates/xtask/src/ci/` 的现有代码。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
