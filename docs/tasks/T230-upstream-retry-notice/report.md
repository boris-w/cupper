# T230 汇报

- 执行者：子 agent（Rust 工程师，宿主编排与协议）
- 分支：`feat/T230-upstream-retry-notice`（头提交 `ff596dc`，基于 main `7e430a8`）
- 完成日期：2026-11-03
- PR：https://github.com/boris-w/cupper/pull/58 。`ff596dc` 上 8 个检查全部通过：https://github.com/boris-w/cupper/actions/runs/41623014607
- 连跑（计划 1.2，新增的 IT 连跑 10 次）：https://github.com/boris-w/cupper/actions/runs/41623000288 ，参数 kind=cargo、package=cupper-host、args=`--features test-harness --test driver_k`、filter=`it_k18`、times=10、parallel=4、runner=macos-15，结果 40 次全部通过。

## 做了什么

**先更正 T112 的一处判断**：`ItemPatch` 从 cb371bb 引入起就有 `params: Option<NoticeParams>`。两边的会话视图早已合并这个字段：Rust 在 `view/session.rs` 里有 `Item::Notice(n) => set!(n: params;)`，TS 的 `PATCHABLE.notice` 是 `["params"]`。界面的 NoticeRow 本来就用 `params.attempt` 和 `params.max` 填 `notice_upstream_retry` 的文案。所以任务书第 1、2、5 项都不用做：不改协议，不跑 `pnpm gen`，`schema-compat` 没有变化。编排者已同意按这个范围做。

**只改了会话这一侧**（b36609c）：
- **合并器**：`OpenTurn` 加 `retry` 字段，写法照计划条目的 `plan`。
  - 回合里第一次重试时，开一条 `notice: upstream_retry`，带 `attempt` 和 `max`。
  - 之后每次重试，用 `item.updated` 只更新 `params`。
  - `settle` 时，它和回合里其他打开的条目一起以 `completed` 结束。
  - 没有打开的回合、或回合已经在结束时，重试不写 notice。
- **会话 actor**：core 把 `DriverEvent::UpstreamRetry` 转成合并器的 `Input::Retry`，仍由会话流唯一的写者写出。
- **用例 IT-K18-upstream-retry**（`tests/driver_k.rs`，草稿取自 T112 的 notes）：codex 形态的假 agent 执行 `upstream_retry 3; say through`。断言：
  - 回合 `completed`；
  - 整个回合只有一条 notice，`code` 为 `upstream_retry`，属于这个回合；
  - 开始时 `attempt` 为 1、`max` 为 5；
  - 两次 `item.updated` 的 `params.attempt` 依次是 2、3；
  - 回合结束时这条 notice 以 `completed` 关闭。

## 关键取舍

- 照计划条目的做法，开、改、关都用合并器已有的 `start`、`patch`、`end` 和 `settle`，不另起机制。
- `start` 会结束正在写的消息或思考条目，所有新条目都是如此。重试如果插在一段消息中间，消息会被分成两条。录制里的重试都出现在回合开头、输出之前，所以没有特别处理。

## 验证方式

- 本机单份运行：`driver_k` 的 18 个用例全部通过（含 IT-K18）；orchestrator 的 86 个单元测试全部通过；clippy（test-harness）没有警告。
- 连跑和 PR 的 CI 结果见文首。

## 规模

增 80、删 3（实现约 35 行，用例约 30 行），停止线 200。

## 提交列表

```
ff596dc docs(task): add the T230 notes
b36609c feat(host): one upstream_retry notice in a turn follows the retries
```

## 遗留问题 / 范围外发现

1. 重试插在消息中间时，消息会分成两条（见关键取舍）。如果真实 Codex 在输出途中也会重试，可以考虑让这条 notice 不结束正在写的消息条目。等 T113 录到含重连的场景后再判断。
2. 06 的 K18 一行里写着"有含重连的录制时加 GOLD-A"，仍待 T113 录制。
3. worktree 保留着，等编排者合并后再清理。

（说明：子任务的报告交接通道已在第一个任务时用掉，所以经 SendMessage 送达，请按此存档为 T230 的 report.md。）
