# T269 审阅 codex 0.170.0、0.171.0 的漂移

- 派发日期：2026-11-23
- 执行者：子 agent（CI 与工具；T259、T261、T264 的执行者）
- 分支：`chore/T269-codex-drift-0171`
- 依赖：T264（已合并）。T179 依赖本任务

## 范围

**做**（计划 T269 一行）：照 T259、T264 的流程（`docs/tasks/T259-codex-drift-1111/`、`docs/tasks/T264-codex-drift-0169/`）审阅。

1. 11-19 起 nightly 的 `codex-drift` 每晚报"not reviewed yet"（例如 run 45083301404），npm 的 `latest` 是 0.171.0（11-21 发布）；`reviewed.json` 最后一条是 0.169.0。先确认 0.169.0 之后有几个版本、新变化出自哪一版。
2. 和 0.169.0 的记录逐行比，分清这次新出现的变化；必要时另取上一版做两两比较。日志里能看到的有 `Thread.attachedToThreadId`、`ThreadItem` 的 `moderation` 和 `imageGeneration` 新属性、`TrustedContacts`、`UserInput.audio` 去掉 `url`、`AdditionalContextEntry.name` 等，以日志全文为准。
3. 逐条查对 codex-acp 2.2.2 有没有影响，只读源码。去掉的字段（如 `audio.url`、`input_audio.audio_url`）要特别看适配器有没有用到。
4. 用 `review <dir> T113 <结论>` 写进 `testdata/codex-drift/reviewed.json`；有影响的单列给 T113。
5. 分支上的 nightly `codex-drift` 判通过，由 `main` 触发。

**隔离**（同 T246、T264）：
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

- 卷宗：T264、T259、T261、T246、T209、T114（流程、工具的用法、已知盲点）。
- 交接事项：维护 codex-drift 工具、T113 的几行。

## 注意

- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令。

## 工作方式

worktree 是 `.claude/worktrees/T052`，编排者已切到新分支 `chore/T269-codex-drift-0171`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
