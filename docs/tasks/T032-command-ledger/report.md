# T032 汇报

- 执行者：子 agent（Rust 工程师，宿主网关）
- 分支：`feat/T032-command-ledger`
- 完成日期：2026-10-19

## 做了什么

在 `crates/cupper-host/src/ledger/` 新建命令账本模块。共享文件只做了附加：`lib.rs` 加一行 `pub mod ledger;`，`durations.rs` 加两项；`store/` 没有改。

- **键与哈希**：
  - 键是 `(PrincipalId, CommandId)`，用 principal 的稳定 ID，所以断线重连后重试同一条命令仍能命中。
  - `params_hash` 去掉顶层的 `command_id`、`attestation`、`input`，再用 `cupper_protocol::canonical::digest` 做 JCS 摘要（`d_` 加 sha-256）。
  - 方法名不同或哈希不同，都回 `idempotency_mismatch`。
- **三种状态**（按 5.2.5）：
  - **进行中**：只在内存里。重复请求拿到 `Repeat::Wait(Waiter)`，等第一次执行的结果。
  - **已完成**（`done`）：重复请求拿到 `Repeat::Replay`，结果对象里加上 `replayed: true`。错误结果原样回放。
  - **结果未知**（`pending_external`）：记录了"开始"、没有结果的命令，重试回 `conflict/outcome_unknown`。宿主重启后从表里加载出来的这类记录是这样；同一次运行里，标了 external 的执行没有给出结果就被丢弃，也是这样。
- **对外接口**：
  - `Ledger::begin`：只读内存，读循环可以同步调用。第一次执行返回 `Begin::Run(Ticket)`，重复请求返回 `Begin::Repeat`。
  - `Ledger::lookup`：查询重复请求现在会得到什么，不登记执行。
  - `Ticket::complete`：返回 `Completion`。
  - `Ticket::mark_external`：先写 `pending_external`，用 `Durability::Full` 提交，落盘后才返回。
  - `Ticket::reject`：命令在执行前失败，不记录，重试会重新执行。
  - 丢弃 `Ticket` 或 `Completion`：等同于宿主停在这里。
- **与事件同事务**：
  - `Completion::row()` 交给 `Batch::with`，与首批事件在同一个事务里提交。提交后执行者调用 `completion.committed()`。
  - 没有事件的命令用 `Completion::commit()` 单独提交。
- **存储与过期**：
  - `Ledger::load` 在启动时从 `commands` 表加载 48 小时内的记录。
  - `Ledger::sweeper()` 经 `Clock` 定时，启动时清理一次，之后每小时一次，同时清内存和表；进行中的命令不清。
  - 时长登记为 `ledger_retention`（48 小时）和 `ledger_sweep`（1 小时）。
- **错误**：`LedgerError` 用 thiserror，实现了 `Into<RpcError>`：
  - `Mismatch` 映射为 `idempotency_mismatch`；
  - `OutcomeUnknown` 映射为 `conflict/outcome_unknown`；
  - 参数没有规范形式时映射为 invalid params；
  - 存储错误映射为 `internal`。

## 关键取舍

- **在一把锁下判断加登记**：`begin` 在同一次加锁里查表并登记进行中，不做"先 `lookup` 再登记"两步，免得两个并发的同一命令都被判成第一次执行。`lookup` 只读，作为 5.2.1 列出的查询接口保留。
- **参数不一致在等待之前就判定**：进行中的记录保存了方法名和哈希，带着不同参数来的重复请求立刻回 `idempotency_mismatch`，不用等第一次执行结束。SP-4a 是等结束后才比较。
- **执行后的错误也记录**：命令执行之后才出现的错误（例如 `git_error/hook_failed`）作为结果记下，重试时回放，符合"重复请求拿到第一次的结果"。执行前就失败的用 `reject`，不记录，与 5.2.5 一致。这一点与 SP-4a 不同，SP-4a 只记录成功的结果。
- **`reject` 在 `mark_external` 之后**：删除 `pending_external` 行时不等回执。如果宿主在删除提交前崩溃，这一行留下，重启后回 `outcome_unknown`，这是更谨慎的答案。
- **等待者的结果**：第一次执行被 `reject` 时，等待者拿到同一个错误；第一次执行没给出结果就结束时，等待者拿到 `internal`。
- **过期判定**：只靠每小时一次的清理，加上启动加载时的同一截止时间（创建时间不晚于"现在减 48 小时"的算过期）。所以一条记录实际最多可能保留 49 小时，满足 ADR-04"至少 24 小时"。
- **表里 `result` 列的形态**：`{"result": ...}` 或 `{"error": {...}}`。写入一律用 `INSERT OR REPLACE`，同一个键过期后被重新执行时，可以覆盖旧行。
- **类型签名**：store 的 `Write` 类型别名是私有的，为了不改 `store/`，`row()` 返回 `impl FnOnce`，调用方写成 `with: Some(Box::new(completion.row()))`。
- **测试用真实存储**：测试用真实的 `host.db`、写线程和读连接池。临时目录借用已有的 `gateway::testkit::TempDir`，没有去改 `store` 私有的 `testutil`。

## 验证方式

- UNIT-ledger 共 10 个用例，名字都以 `unit_ledger_` 开头，对照验收标准：
  - **首次执行与同参数重试**：`unit_ledger_first_run_is_replayed_to_repeats_and_after_restart`，覆盖成功和错误两种结果、`lookup`、重启之后的回放。
  - **参数不同**：`unit_ledger_other_params_or_method_are_a_mismatch`，对已完成和进行中的命令都测了参数不同、方法不同。
  - **进行中的重复请求**：`unit_ledger_inflight_repeats_wait_for_the_first_run`，覆盖等待中拿到结果（带 `replayed`），以及 `reject` 和丢弃之后等待者拿到什么、重试会重新执行。
  - **重启后的 `outcome_unknown`**：`unit_ledger_restart_leaves_outcome_unknown_only_where_it_began`。重启后只有只记了"开始"的命令回 `outcome_unknown`；已完成的回放；没标 external 的重新执行。还覆盖了同一次运行里的三种结局：丢弃、完成、`reject`。
  - **48 小时过期**：`unit_ledger_commands_expire_after_48_hours_by_clock`，用 `ManualClock` 驱动清理任务，不睡眠。覆盖启动加载跳过过期行、启动时立即清理、到第 47 小时仍在、到第 48 小时被清掉、表里的行也删掉。
  - **参数哈希**：`unit_ledger_params_hash_ignores_command_id_attestation_input_and_key_order`。键的顺序和顶层三个字段不影响哈希；值、数组顺序、嵌套的 `input`、多出的字段都会改变哈希。
  - **principal 互不影响**：`unit_ledger_principals_keep_their_own_command_ids`。
  - **同事务**：`unit_ledger_result_commits_with_its_first_events_or_not_at_all`，经真实的 `StreamWriter::append` 和 `Batch::with`。用临时触发器让账本这一行在事务里失败后，事件和这一行都没有落盘，等待者拿到 `internal`，重试会重新执行。
  - 另有两个：`unit_ledger_load_refuses_a_bad_row`（坏行加载报 `Corrupt`）、`unit_ledger_errors_map_to_protocol_codes`（错误码映射）。
- 变异检查：改坏 8 处关键逻辑，每处都有用例失败（明细见 `notes.md`）。
- `cargo clippy -p cupper-host --all-targets` 在带全部特性和不带特性两种情况下都没有警告；已跑 `cargo fmt`。
- 在干净的 worktree 里跑了 `cargo xtask ci pr --record docs/tasks/T032-command-ledger/ci/`：rust 828 通过、protocol 176、ts 232、license 1，全部 pass，UNIT-ledger 为 pass。摘要 `pr-6d22ad961fbb.json` 已提交。
- 规模：按分叉点统计，实现 627 行、测试 475 行，合计 1,102 行（估计约 990 行，上限约 1,500 行）。开工前在 `notes.md` 按模块写了估计；实现写完时统计过一次，是 628 行，推算总数约 1,120 行，于是继续写测试。

## 提交列表

```
6c9a979 docs(task): T032 notes, the race as observed
97a5f48 docs(task): T032 pr CI summary
6d22ad9 docs(task): T032 actual line counts and notes
251eec0 test(host): ledger row commits with the command's first events or not at all
fa60dcf feat(host): ledger drops commands after 48 hours on Clock timers
cb9a6de feat(host): command ledger keyed by principal and command_id
006fd59 docs(task): T032 line estimates by module
```

## 遗留问题 / 范围外发现

1. **给 T033 的接入要点**：
   - 启动时在 `host.db` 连接上调用 `Ledger::load`，并 `spawn(ledger.clone().sweeper())`。
   - 读循环对命令类请求调用 `begin`，传入原始 params 的 `Value`：
     - `Run(ticket)`：把 ticket 随命令交给执行者；
     - `Replay`：直接回复；
     - `Wait(w)`：另起 task，等 `w.outcome()` 后再回复。
   - `LedgerError` 可以直接转成 `RpcError`。
   - `test/run_maintenance` 可以直接调用 `Ledger::sweep()`。
2. **给 T043 和 git 执行者**：
   - 受理后调用 `ticket.complete(Ok(result))`，把 `Box::new(completion.row())` 放进 `Batch::with`；`append` 成功后调用 `completion.committed()`，失败时丢弃 `completion` 即可。
   - 改动用户仓库的 git 命令，在执行 git 之前先 `mark_external().await`。
   - 前置条件不满足时用 `reject(err)`。
   - `Appended::rejected` 不为空时，记日志由执行者负责。
3. **`replayed: true` 没有写进协议 schema**：各方法的结果类型里都没有这个字段，生成的 zod 不是 strict，所以客户端现在能接受，但没有声明。要不要在协议里写明，由编排者或协议任务决定。
4. **可能的合并冲突**：`durations.rs` 新增的两项紧挨在 `INTERRUPT_FALLBACK` 之后。T027 如果在同一处附加，合并时会有相邻行冲突，按两边都保留处理即可。
5. **早已存在的文档警告**：`cargo doc -p cupper-host` 报 `driver::tap` 的公开文档链接到私有项 `redact`，不是本任务引入的，没有改。
