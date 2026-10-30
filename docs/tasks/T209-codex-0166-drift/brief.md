# T209 审阅 codex 0.166.0 的漂移

- 派发日期：2026-10-30
- 执行者：子 agent（工程师）
- 分支：`chore/T209-codex-0166-drift`
- 依赖：T114（已合并）。T099 依赖本任务：nightly 要在退出候选上通过

## 背景

10-29 的 nightly（https://github.com/boris-w/cupper/actions/runs/40967230840 ）里，`codex-drift` 发现 npm 上最新的 codex 已经是 0.166.0，按 T114 的规则"每个新版本报警一次"，作业失败，提示 "not reviewed yet: once it is, record it with `review`"。0.165.0 的那条记录已经在 `testdata/codex-drift/reviewed.json` 里（跟进任务 T113）。

## 范围

**做**：
1. 在本机照 T114 的流程跑 `tools/codex-drift` 的 `fetch`、`check`：只运行 `app-server generate-json-schema`，`HOME`、`CODEX_HOME` 指向隔离的空目录。
2. 逐条审阅 0.166.0 相对基线（0.163.0）的漂移，和 0.165.0 那条记录比较：
   - 哪些是 0.166.0 新出现的，例如 `hook/started` 的 `HookEventName` 多了 `preModelRequest`、`postModelRequest`，`AbsolutePathBuf` 被删掉，`turn/start` 多了 `responsesapiClientMetadataPatch` 等；
   - 每一条对 codex-acp 2.2.2 有没有影响：适配器读不读这些字段，读的话会不会丢数据或者出错。照 T114 对 `imageGeneration` 的做法，查 codex-acp 2.2.2 的源码。
3. 用 `review <dir> T113 <结论>` 写入记录，结论要写清和 0.165.0 相比多出的影响。基线和已验证范围都不动。
4. 在 GitHub 上用这个分支手动触发一次 nightly，或者只看其中的 `codex-drift`，确认判为通过。要触发时告诉 `main`。

**不做**：升级 codex-acp、扩展已验证范围（T113）。

## 验收标准

- [ ] 记录写入 `reviewed.json`，结论逐条说明对 codex-acp 2.2.2 的影响。
- [ ] 分支上的 `codex-drift` 判为通过，附运行链接。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：50；硬停止线：80

<!-- 记录数据不计入。 -->

## 参考资料

- 设计：5.1.6（`codex-drift`）；ADR-08。
- 卷宗：T114（工具的用法、0.165.0 的审阅结论、`notes.md`）；T102（codex-acp 2.2.2 的源码阅读）。
- `docs/03-plan.md`：T209 一行；`handoff-notes.md` 里 T113 的几行。

## 注意

- 不运行 Codex 的会话，不登录，不读写真实的 `~/.codex`。
- 外发请求只有 npm，不带个人信息。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
