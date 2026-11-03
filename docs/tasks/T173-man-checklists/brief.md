# T173 S1–S4 逐步检查表与人工证据格式

- 派发日期：2026-11-03
- 执行者：子 agent（测试与文档）
- 分支：`docs/T173-man-checklists`
- 依赖：T099（M1a 已退出）。T179（M1b 退出验收）依赖本任务

## 背景

M1b 退出标准 ①（设计 5.12.3）：作者用两种 agent 走完 PRD 的 S1–S4 场景，按逐步检查表 MAN-S1 至 MAN-S4 记录步骤、通过条件、CLI 和适配器的版本、日期，结果存档。这四个编号已登记为 `man`。目前人工证据的格式只有一个先例：T099 的 `docs/tasks/T099-m1a-exit/man-keyboard.md`，以及 `tests/ids.toml` 里那条 `[[evidence]]`（id、date、record、versions）。T173 要在 M1b 开头把检查表写好，并定下全部 MAN 项统一的证据格式，和 trace-ids 的核对对齐。

## 范围

**做**（计划 T173 一行，5.12.3 退出标准 ①，5.9.10）：
1. **MAN-S1 至 MAN-S4 的检查表**，按 PRD 的 S1–S4（`docs/01-prd.md` 第 69–72 行）和 12-milestones 第 160 行的 E2E-S1 至 S4 写：
   - 每个场景按 Claude、Codex 两种 agent 各走一遍；
   - 每一步写明操作、预期、通过条件；
   - 记录 CLI 版本、适配器版本、cupper 的提交或构建、日期；
   - 写明用自用构建 Cupper Local，在真实项目上、用真实 agent 运行（消耗额度，由作者做）；
   - 哪些步骤依赖还没合并的任务（审查面板、worktree、Steer 等），检查表里标注依赖的任务号，届时作者按已有的部分走。
2. **人工证据的统一格式**：
   - 记录文件放在哪里、叫什么、写哪些字段，以 `man-keyboard.md` 为起点归纳；
   - `tests/ids.toml` 的 `[[evidence]]` 应有哪些字段（例如 `versions` 里写哪些组件），与 trace-ids 的 `verify` 怎么核对、什么情况判为缺；
   - 现有的 MAN 项（MAN-keyboard、MAN-ime、MAN-notification、MAN-tcc、MAN-ui6、MAN-upgrade 等，09 的 5.9.10 表）是否都套得上，套不上的写明理由。
   - 如果 trace-ids 要改（例如对 `versions` 做必填检查），改动小就在本任务做；大的话列进汇报，由 `main` 决定。
3. 检查表和格式说明放在合适的位置（例如 `docs/manual/`，由你提议）。

**不做**：走检查表（T179，由作者做）；改 E2E-S1 至 S4。

## 验收标准

- [ ] 四份检查表和证据格式说明写好，经编排者审阅。
- [ ] trace-ids 如有改动，`trace-ids check` 和相关用例通过。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：400；硬停止线：500（文档行数计入；如果 trace-ids 有改动，代码另计）

## 参考资料

- PRD：S1–S4 场景（`docs/01-prd.md` 第 69–72 行）。
- 设计：12-milestones 的 5.12.3 与 E2E-S1–S4（第 160 行），09 的 5.9.10（MAN 项表、trace-ids），07 的 5.7.9（自用构建）。
- 卷宗：T099（man-keyboard.md、record.md）、T160（自用构建与用户文档）、T002（trace-ids）。

## 注意

- 不运行真实的 Claude Code、Codex CLI，检查表由作者以后执行。
- 文档里不写个人信息（本机路径、账号等）。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
