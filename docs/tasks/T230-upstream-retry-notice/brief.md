# T230 upstream_retry 的 notice 在回合内 upsert（K18）

- 派发日期：2026-11-03
- 执行者：子 agent（Rust 工程师，宿主编排与协议）
- 分支：`feat/T230-upstream-retry-notice`
- 依赖：T111（Codex 接入）；T112（发现）

## 背景

06 的 K18：Codex 遇到上游过载会自动重试，最多 5 次，设计要求宿主写 `notice: upstream_retry` 显示进度（5.6.3、K18、协议 `NoticeCode` 的注释：同一回合内 upsert 一条带 attempt、max 的 notice）。T112 发现：运行时已经把 `DriverEvent::UpstreamRetry {attempt, max}` 发给会话，会话却只拿它判定"有活动"，从不写这条 notice。按设计做要改接口：notice 现在是即时条目，开始即结束；`ItemPatch` 没有 `params` 字段，没法用 `item.updated` 改 attempt。这超出了 T112 就地修的上限，所以另开本任务。

## 范围

> 2026-11-03 开工时更正：`ItemPatch` 早就有 `params`，两边的会话视图和界面的 NoticeRow 都已支持，下面第 1、2、5 条不用做，没有协议改动。本任务只做第 3、4 条（会话这一侧和用例）。

**做**：
1. 协议附加：`ItemPatch` 加可选的 `params`。契约已冻结，只能附加，`schema-compat` 必须 0 breaking；`pnpm gen` 并提交生成物。
2. Rust 与 TS 两边的会话视图都按 `params` 合并。
3. 会话：回合里第一次收到 `UpstreamRetry` 时开一条 `notice: upstream_retry`（带 `attempt`、`max`），之后用 `item.updated` 更新 attempt；回合结束时关掉这条 notice。每个回合至多一条，经会话流唯一的写者写。
4. 用例 **IT-K18-upstream-retry**（已登记，由 T112 移来）：codex 形态的假 agent 连续重试几次，断言只有一条 notice，attempt 随之更新，回合结束时关掉。T112 的 `notes.md` 里有草稿可以取回。
5. 界面上这条 notice 怎么显示：消息流现有的 notice 行能显示 `params` 就顺带接上，不能的话列进汇报，不在本任务造新组件。

**不做**：改 Claude 的重试行为；其他 notice 改成 upsert。

## 验收标准

- [ ] IT-K18-upstream-retry 通过。
- [ ] 不改协议。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：200；硬停止线：260（生成物不计）

## 参考资料

- 设计：06 的 5.6.3（自动重试）与 K18、5.3（`ItemPatch`、`item.updated`、`NoticeCode`）、5.2.6（会话流）。
- 卷宗：T112（notes.md 里的草稿、report）、T111、T226（即时条目的写法 `orchestrator::instant()`）。

## 注意

- 不运行真实的 Codex CLI 或 Claude Code；只用假 agent。
- 本机只单份跑（守则第 10 条）。
- 每条事件流只有一个写者；时间经 `Clock`。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
