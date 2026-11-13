# T261 codex-drift：与已审记录相同的漂移视为已审

- 派发日期：2026-11-13
- 执行者：子 agent（CI 与工具；T259 的执行者）
- 分支：`chore/T261-codex-drift-same`
- 依赖：T259（已合并）

## 范围

**做**（计划 T261 一行）：

1. 现在 `tools/codex-drift` 的 `check` 只按 npm `latest` 的版本号查 `testdata/codex-drift/reviewed.json`。协议没变的补丁版本（T259 里的 0.167.1、0.167.2）也会让 nightly 的 `codex-drift` 报"not reviewed yet"，每次都要人工记录。
2. 改为：`latest` 没有自己的记录时，如果它的漂移与某条已审记录（同一基线）逐行相同，就判通过，并打印沿用的是哪个版本的结论（版本、日期、任务、结论）；有任何不同仍报未审，退出码不变。
3. `latest` 有自己的记录时，行为不变：漂移与它的记录不同仍要求重审。
4. 工具的单元测试覆盖：与某条记录相同（通过并指明沿用哪条）、多一处、少一处（都报未审）、基线不同的记录不沿用。

**不做**：改基线（`write`）、改 `review` 的写法、补 T246 记下的 `anyOf` 盲点、改 nightly 的 workflow。

## 验收标准

- [ ] `tools/codex-drift` 的单元测试通过，覆盖上面四种情况。
- [ ] 本机 `cargo xtask ci codex-drift --in nightly` 通过；分支上的 nightly `codex-drift` 判通过，由 `main` 触发。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

预计约 40 行（含测试），停止线 80。

## 参考资料

- 卷宗：T259、T246、T209、T114；交接事项里维护 codex-drift 工具的几行。
- 设计 5.1.6（`codex-drift` 一行）、ADR-08。

## 注意

- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令。
- 只在本机用已有的 schema 和记录测试；需要跑 codex 时同 T259 的隔离做法。

## 工作方式

worktree 是 `.claude/worktrees/T052`，编排者已切到新分支 `chore/T261-codex-drift-same`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
