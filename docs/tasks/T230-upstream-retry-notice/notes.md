# T230 工作笔记

## 开工前的核对

- `ItemPatch` 早就有 `params: Option<NoticeParams>`（cb371bb 引入 `ItemPatch` 时就有）。Rust 的会话视图（`view/session.rs` 的 `Item::Notice(n) => set!(n: params;)`）和 TS 的会话视图（`PATCHABLE.notice = ["params"]`）都已合并它，界面的 NoticeRow 用 `params.attempt`、`params.max` 填 `notice_upstream_retry` 的文案。T112 里"要给 `ItemPatch` 加 `params`"的判断是错的，已经告诉编排者。所以 brief 的第 1、2、5 项不用做：不改协议，不 gen。
- 缺的只是会话这一侧：`DriverEvent::UpstreamRetry` 只交给了自发回合的判定，合并器从来不写这条 notice。

## 做法

合并器的 `OpenTurn` 加 `retry`，写法照计划条目（`plan`）：回合里第一次重试时 `start` 一条 `notice: upstream_retry`（带 `attempt`、`max`），之后每次重试用 `item.updated` 只改 `params`，`settle` 时和回合里其他打开的条目一起以 `completed` 结束。没有打开的回合、或回合已在结束时，重试不写 notice。会话 actor 的 core 把 `UpstreamRetry` 转成合并器的 `Input::Retry`，写者仍是会话流唯一的写者。

`start` 会结束正在写的消息或思考条目（所有新条目都这样）：重试插在一段消息中间时，消息会分成两条。录制里重试都出现在回合开头、输出之前，所以没有特别处理。

## 规模

增 80、删 3（实现约 35，用例约 30），停止线 200。
