# T087 host-client I-b：订阅与同步

- 派发日期：2026-10-19
- 执行者：子 agent（TypeScript 工程师）。与 T071 是同一位执行者。
- 分支：`feat/T087-host-client-sync`
- 依赖：T071（已合并）。本任务由 T071 拆分而来（计划 v1.3.9），**零松弛**：T087 → T072 → T080 → …

## 范围

**做**（计划 T087 一行；设计 5.7.6、5.3.3；5.7.6 末尾"T071 开工前的裁定"；5.3.11 末尾"T014 实现时的裁定"）：
1. 新增 `sync.ts` 和 `HostClient.subscribe`：
   - 订阅与游标，包括 `data_epoch`；
   - 快照分页与重置；
   - 连续性检查和三种计数：`gaps`、`duplicates`、`resets`；
   - 断线重连后重新订阅。
2. 同步规则与 Rust 的 Mirror（`crates/cupper-cli/src/mirror.rs`）保持一致：
   - 只有游标对得上的 `replay` 才保留视图和游标；
   - 快照（不论原因）一律丢弃视图和游标；
   - 出现缺口或重复时，带游标重新订阅；
   - `ViewError` 或信封解不开时，不带游标重新订阅；
   - `stream/reset` 保留游标。
3. 事件逐个 `zEvent.safeParse`，一个事件残缺只影响它自己。

**不做**：视图和 reducer（T072）。

## 验收标准

- [ ] 新增 UNIT-hc-sync，覆盖：
  - 订阅与游标；
  - `data_epoch` 改变时整体重置；
  - 快照分页与重置；
  - 连续性缺口与重复；
  - 三种计数；
  - 断线重连后重新订阅；
  - 一个事件残缺时只影响它自己。
  - 可以参照 Rust Mirror 的 7 个用例。
- [ ] 测试标题以 `UNIT-hc-sync` 开头（该编号已在 `tests/ids.toml` 的 T087 段登记）。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T087-host-client-sync/ci/` 通过，摘要随任务提交。

## 交接事项

- **T071 留下的接入点**（`docs/tasks/T071-host-client-conn/notes.md` 的"给 T087 的接入点"）：
  - 所有 `stream/*` 通知都按顺序经 `onNotification` 到达。
  - `stream/subscribe` 必须用 `client.call(method, params, done)`：`done` 同步执行，排在同一数据块里随后的通知之前。
  - `onState` 的 `reconnecting`、`failed`、`closed` 表示停下并保留游标；`ready` 带着新的握手结果（含 `data_epoch`），这时重新订阅。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 规模

估为 M，约 450–560 行，上限约 1,500 行。**每完成一个模块统计一次行数**（守则第 8 条）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
