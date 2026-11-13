# T259 审阅 codex 新版本的漂移（11-11 起）

- 派发日期：2026-11-13
- 执行者：子 agent（CI 与工具；T151、T248、T253、T255、T252 的执行者）
- 分支：`chore/T259-codex-drift`
- 依赖：T246（已合并）。T179 依赖本任务

## 范围

**做**（计划 T259 一行）：照 T246 的流程（`docs/tasks/T246-codex-0167-drift/`）审阅。

1. 11-11 起 nightly 的 `codex-drift` 每晚报"not reviewed yet"（例如 run 43328452471）。先确认 npm 上的新版本号，以及有几个版本没审。
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

- 卷宗：T246、T209、T114（流程、工具的用法、已知盲点）。
- 交接事项：维护 codex-drift 工具、T113 的几行。

## 注意

- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令。

## 工作方式

worktree 是 `.claude/worktrees/T052`，编排者已切到新分支 `chore/T259-codex-drift`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
