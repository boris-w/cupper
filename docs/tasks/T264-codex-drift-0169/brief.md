# T264 审阅 codex 0.169.0 的漂移

- 派发日期：2026-11-16
- 执行者：子 agent（CI 与工具；T259、T261 的执行者）
- 分支：`chore/T264-codex-drift-0169`
- 依赖：T259、T261（已合并）。T179 依赖本任务

## 范围

**做**（计划 T264 一行）：照 T259 的流程（`docs/tasks/T259-codex-drift-1111/`）审阅。

1. 11-13 起 nightly 的 `codex-drift` 每晚报"not reviewed yet"（例如 run 43770475159），npm 的 `latest` 是 0.169.0；漂移与 0.168.0 的记录不同，T261 的沿用不适用。先确认 0.168.0 之后有几个版本、新变化出自哪一版（这次有 `AgentMessageModeration`、`AgentMessageDisplay`、`CodexErrorInfo` 的新枚举、`responsesapiClientMetadataPatch` 等）。
2. 和上一条已审的记录逐行比，分清这次新出现的变化；必要时像 T246 那样另取上一版做两两比较。
3. 逐条查对 codex-acp 2.2.2 有没有影响，只读源码。
4. 用 `review <dir> T113 <结论>` 写进 `testdata/codex-drift/reviewed.json`；有影响的单列给 T113。
5. 分支上的 nightly `codex-drift` 判通过，由 `main` 触发。

**隔离**（同 T246）：
- codex 只运行 `app-server generate-json-schema`，`HOME`、`CODEX_HOME` 指向工作目录里的空目录；
- 不登录、不发 prompt、不读写 `~/.codex`；
- 外发请求只到 npm，User-Agent 不带个人信息。

**不做**：升级适配器、改基线（`write`）、改 codex-drift 工具。

## 验收标准

- [ ] 记录写入 `reviewed.json`，结论逐条说明对 codex-acp 2.2.2 的影响。
- [ ] 分支上的 nightly `codex-drift` 判通过。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

记录数据和文档不计入；代码改动应为 0。

## 参考资料

- 卷宗：T259、T261、T246、T209、T114（流程、工具的用法、已知盲点）。
- 交接事项：维护 codex-drift 工具、T113 的几行。

## 注意

- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令。

## 工作方式

worktree 是 `.claude/worktrees/T052`，编排者已切到新分支 `chore/T264-codex-drift-0169`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
