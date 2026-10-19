# T032 命令账本

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主网关）
- 分支：`feat/T032-command-ledger`
- 依赖：T025（已合并）。T033（网关 II，关键路径）依赖它。

## 背景

写入类的方法都是"命令"（5.3.4 的幂等一栏）：客户端带 `command_id`，宿主按账本去重，重试不会重复执行（设计 5.2.5）。命令的结果和它产生的事件在同一个事务里落盘，所以崩溃后重试也不会重复执行（5.2.3 第 3 条）。本任务实现账本本身；路由和执行者在 T033、T043。

## 范围

**做**（计划 T032 一行；设计 5.2.5"命令账本"）：
1. **键**：`(principal 稳定 ID, command_id)`。
2. **三种状态**：进行中、已完成、结果未知。具体名称和转换按 5.2.5。
3. **参数哈希**：
   - 对参数去掉 `command_id`、`attestation`、`input` 之后做 JCS，再取哈希；
   - 同一个 `command_id` 带着不同参数重来，回 `idempotency_mismatch`。
4. **存储**：
   - 48 小时的内存表；
   - `commands` 表持久化，表结构在 `0001` 里（T025）；
   - 过期清理经 `Clock` 定时，时长登记在 `durations.rs`。
5. **`outcome_unknown`**：宿主崩溃前只记下"开始"、没有结果的命令，重试时回 `conflict/outcome_unknown`。
6. **与事件同事务的接口**：命令结果和它产生的首批事件放进同一个写线程的 `Batch`，用 T026 的 `Batch::with`。
7. **对外接口**：`begin`、`complete`、`lookup`（5.2.1 的模块表）。

**不做**：
- 路由与方法分发（T033）；
- 执行者（T043 等）。

## 验收标准

- [ ] 新增 UNIT-ledger：
  - 首次执行，以及同参数的重试直接返回原结果；
  - 不同参数回 `idempotency_mismatch`；
  - 进行中的重复请求按 5.2.5 处理；
  - 宿主重启后，只有"开始"的命令回 `outcome_unknown`；
  - 48 小时后过期，用 `ManualClock` 验证，不睡眠；
  - 参数哈希不受 `command_id`、`attestation`、`input` 和键顺序影响；
  - 不同 principal 的同一个 `command_id` 互不影响。
- [ ] 结果与首批事件同事务：用 T026 的 `Batch::with` 写一个用例，提交失败时两者都不落盘。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T032-command-ledger/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **同事务**（T026）：
  - 命令结果和它产生的事件放进同一个 `Batch`，用 `Batch::with`，同一事务提交；
  - `Appended::rejected` 不为空，说明写者违反了视图规则，要记日志。
- **写线程**（T024）：回执在提交之后才返回；操作不能自己结束事务，否则整批作废。
- **惯例**：
  - JCS 用 `cupper-protocol` 的 `canonical`；
  - 时间只经 `Clock`；
  - 错误类型用 thiserror；
  - 改了协议类型就跑 `pnpm gen` 并提交生成物（T015）。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 并行注意

T027 同时在改 `store/`，T042 在改 `orchestrator/`。本任务放在宿主的 `ledger/` 模块，`lib.rs` 只加一行；需要 `store` 的接口时只做附加。

## 规模

计划估为 M（400–900 行）。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**预计会超出约 1,500 行时，必须先停下来汇报**。

## 参考资料

- 设计：
  - `02-host.md` 5.2.1（模块表）、5.2.3（同事务）、5.2.5（策略与命令账本）；
  - `04-data-model.md` 5.4.2（`commands` 表）；
  - `03-protocol-v1.md` 5.3.4（幂等一栏、`idempotency_mismatch`、`outcome_unknown`）。
- T024、T025、T026 的代码和汇报。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
