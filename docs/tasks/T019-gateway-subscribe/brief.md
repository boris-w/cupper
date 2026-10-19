# T019 网关 III：订阅、历史与发送合并

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主网关）
- 分支：`feat/T019-gateway-subscribe`
- 依赖：T033（已合并）。本任务由 T033 拆分而来（计划 v1.3.7）。T062、T090 依赖它。

## 范围

**做**（计划 T019 一行；设计 5.3.3、5.3.4（流）、5.2.8；5.3.11 末尾"T033 开工前的裁定"第 4–6 条）：
1. **`stream/subscribe`**：
   - 四种情况，用 `sync::subscribe_mode(resume, &LogExtent)` 判断；
   - 快照分页：`paginate(snapshot_ops(), 帧负载减去信封的余量)`；
   - 补发用 `store::log::read_events`；
   - 补发时读到缺口：摘掉订阅，清掉这条流在队列里的实时项，发 `stream/reset`，`reason` 沿用 `rebuilt`。
2. **`stream/unsubscribe`**。
3. **发送时合并**：按 `coalesce_ms` 合并，它存在 `Session` 里（T030）。
4. **慢客户端**：网关的发送队列实现 `store::streams` 的 `Sink`。`push` 返回 `Full` 时，流会摘掉这个订阅者并调用 `reset`（`slow_consumer`）。
5. **订阅日志（Q16）**：网关把 `stream.subscribed` 发进审计 channel；host.log 由 T028 负责。
6. **`history/page`**：
   - 用 `store::history::page`；
   - 不带游标时，用 `StreamHandle::read` 取窗口里最早的 `turn_no`；
   - `max_bytes` 截到 48,000；
   - 会话流的 head 为 0 时回 `not_found`。
7. **能力**：`HOST_FEATURES` 补上本任务提供的能力。

**不做**：分发（T033，已合并）、blob（T029）。

## 验收标准

- [ ] 新增 IT-gw-subscribe，覆盖：
  - 四种订阅；
  - 续传；
  - 快照分页；
  - 慢客户端重置；
  - 帧上限；
  - epoch 与游标规则；
  - 补发时读到缺口。
  - 该编号已在 `tests/ids.toml` 的 T019 段登记。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T019-gateway-subscribe/ci/` 通过，摘要随任务提交。
- [ ] 测试名以 `it_gw_subscribe_` 开头。

## 交接事项

- **接入点**（T033）：见 `docs/tasks/T033-gateway-dispatch/notes.md` 的"给 T019、T029 的接入点"。
  - `stream/*`、`history/page` 在方法表里登记为 `Target::Streams`；
  - 订阅表放进每条连接的 `Dispatch`，断开时自动清理；
  - 接上之后，从 UNIT-unsupported-set 的允许表里删掉相应的行。
- **流的接口**（T026）：
  - `Sink` 的两个方法都在流锁内调用，只能排队，不能回调流；
  - 订阅时把响应、快照页或补发标记，放进 `subscribe` 的 `start` 回调里。
- **同步规则**（T014）：见 5.3.11 末尾"T014 实现时的裁定"。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 并行注意

- T037 在改 `gateway/`（连接登记表、`host/shutdown` 广播），T043 在实现 `SessionPort`。
- 本任务的订阅和历史放进新文件，共享文件只做附加，冲突由编排者处理。

## 规模

计划估为 L，T033 估计这部分约 1,260 行，上限约 1,500 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**（实现按草估的 1.7 倍计）。**每完成一个模块就用 `git diff --numstat` 统计一次**。**预计会超出上限时，必须先停下来汇报**。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
