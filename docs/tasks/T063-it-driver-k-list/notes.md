# T063 工作笔记

- 执行者：子 agent（Rust 工程师；假 agent、适配器运行时与 T090–T092、T185 的执行者）

## 开工前读过的

- 任务书；设计 5.6.9 的 K 清单、5.9.2 的剧本；`handoff-notes.md` 第一列含 T063 的五行；共用夹具 `tests/common/mod.rs`。
- 宿主现状：
  - K6：只有"在任何更新之前失败、类别为 Session not found"的 prompt，才会在恢复后重发一次（`turn.rs`）。`crash_cli` 本身的 -32603 归为 `process_exited`。
  - K14：`CliTooOld` 这个类别有定义，但**没有地方产生它**。运行时收到"需要认证"时只会标成未登录。注入的假 agent 也没有 CLI 版本（`cli: ""`，`unsupported`）。
  - 时长可调：`interrupt_fallback`（K1）、`autonomous_silence`（K9）。

## 开工前的规模估计

用例按 1.1 倍、其余代码按 1.15 倍放大。

| 模块 | 内容 | 草估 |
|---|---|---|
| 假 agent 的调用记录 | `--calls <文件>`：每个收到的请求写一行 `{method, session, text}`，K1、K6 据此断言 `session/close`、`session/resume`、`session/new` 和重发与否；含单元测试 | 35 |
| `AdapterLog` 打不开时的 SIGPIPE（就地修） | 没有日志时也起一个只写原始帧的读取任务；含用例 | 45 |
| K14 的改判（就地修） | 运行时收到"需要认证"、CLI 版本又低于已验证范围时，改判为 `cli_too_old`；`test_config.agents.<agent>.cli_version` 给注入的假 agent 一个版本；含单元测试 | 50 |
| `driver_k.rs` 的共用部分 | 宿主与假 agent 的配置、读调用记录、读条目文本、数宿主的进程树 | 40 |
| IT-K1-close-resume | `hang` 后中断，`interrupt_fallback` 到期；记录里 `session/close` 在 `session/resume` 之前，然后才是新的 prompt；`whoami` 回 `resumed=true` | 45 |
| IT-K5-withdraw | 权限请求挂着时中断，Claude 人设发 `$/cancel_request`；请求以 `cancelled`/`withdrawn` 结束 | 30 |
| IT-K6-crash-resume | `crash_cli` 的回合失败；下一条消息先收到 Session not found，宿主恢复后重发一次并跑完；连续五轮崩溃与恢复，宿主的进程树大小不变（Q43） | 55 |
| IT-K6-output-then-crash | 先输出再崩溃：回合失败、输出保留，崩溃的那条 prompt 不再发送 | 35 |
| IT-K9-autonomous | prompt 返回后的自发更新开 `autonomous` 回合；静默超过阈值后又输出，再开一个 | 40 |
| IT-K14-old-cli-auth | 低版本加 `auth_none`：回合以 `cli_too_old` 失败 | 25 |
| IT-auth-none | 未登录：`agents.updated` 为未登录，回合以 `not_logged_in` 失败 | 25 |
| IT-K19-unknown-kind-privileged | 不认识的选项 kind（`permission_options`）：选项 `requires: privileged`、效果未知；以 `approve` 范围回答被拒 | 30 |
| IT-thought-claude | `think`：思考条目及其文本 | 20 |
| IT-plan-entries | `plan_update`：计划条目及其各项 | 25 |
| | 合计 | 约 500 草估，放大后约 **560** |

在停止线 850 以内，开工。两处就地修单独提交，行数在汇报里单列。

## 做完之后

### 实际规模

`cargo xtask size` 报新增 478 行、删除 34 行（净增 444），离停止线 850 还剩 372。

| 模块 | 新增 / 删除 | 估计 |
|---|---|---|
| 假 agent：`--calls` 调用记录（含单元测试） | +54 | 35 |
| 假 agent：`withdraw_after`，及撤回后剧本继续执行（含单元测试） | +24 | 计入 K5 |
| 就地修 1：`AdapterLog` 打不开时的 SIGPIPE（含单元测试） | +67 −14 | 45 |
| 就地修 2：K14 的改判与 `cli_version` 注入 | +47 −20 | 50 |
| `driver_k.rs`：十个用例与共用部分；`Cargo.toml` 测试目标 | +286 | 约 395 |

### Q43：多次 resume 后的进程数

IT-K6-crash-resume 连续五轮"`crash_cli` → 下一条消息 Session not found → `session/resume` → 重发"，每轮结束都数一次宿主下面的进程树（含宿主自己）：五轮都与第一轮之前相同，没有累积。

不过假 agent 的"崩溃"只是模拟，进程本身不退出，所以这里只证明宿主侧的恢复不会多拉起进程。真实 `claude` 在多次 resume 后会不会留下 CLI 进程（上游 #1011），仍要由兼容矩阵（T066）用真实 CLI 观察。

### 就地修的两处缺陷

1. **`AdapterLog` 打不开时，适配器收到 SIGPIPE**（T116 的交接）
   - 现象：日志目录写不了时，launch 不读适配器的 stderr，直接关掉管道。适配器再往 stderr 写就收到 SIGPIPE，原始帧里也没有它的 stderr。
   - 原因：launch 只在拿到日志时才起读取任务。
   - 修法：
     - 日志文件打不开时，supervisor 改用不带文件的 `AdapterLog::unwritten`，它照样读出每一行写进原始帧，并保留脱敏后的末尾；
     - 运行时的测试原来也在不带日志的情况下拉起适配器，一并改用它。
   - 行数：+67 −14，含单元测试 `unit_redaction_adapter_stderr_read_without_the_log_file`。
2. **K14 的改判缺失**
   - 现象：CLI 低于已验证范围、读不了配置时报"需要认证"，宿主一律当作未登录（`not_logged_in`），还把登录状态标成未登录。
   - 原因：`CliTooOld` 这个类别有定义，但没有任何地方产生它。
   - 修法：
     - prompt 以未登录失败、且 CLI 版本低于已验证范围（有版本号时）时，改判为 `cli_too_old`，登录状态不变；
     - `test_config.agents.<agent>.cli_version` 让注入的假 agent 有一个版本，与探测到的版本走同一套分类（`launch::cli_versions`）。
   - 行数：+47 −20。用例是 IT-K14-old-cli-auth。

### 发现：中断时请求的原因是 `turn_cancelled`，不是设计 K5 写的 `withdrawn`

- **现状**：用户中断时，回合状态机在给 agent 发 `session/cancel` 的同时，就以 `turn_cancelled` 结束了挂着的请求。Claude 随后发来的 `$/cancel_request` 找不到挂着的请求，什么也不做。只有 agent 自己撤回时（`withdraw_after`），原因才是 `withdrawn`。
- **两种情况都满足**：每个请求只结束一次，状态是 `cancelled`。IT-K5-withdraw 两种情况都断言了。
- **界面不受影响**：请求记录行和面板都不读原因，两种情况都显示"未回答"，不会出现错误的提示。
- **倾向改设计**：
  - 中断由用户发起，宿主先结束请求，不必等 agent 回话；Codex 根本不撤回（K4），宿主替用户应答 `Cancelled`，两边走同一条路；
  - 原因 `turn_cancelled` 也如实反映了"是回合被中断"；
  - `withdrawn` 留给 agent 自己撤回的情形，宿主已经这样做了。
  - 建议把设计 5.6.9 K5 一行的"请求标为 cancelled（withdrawn）"改成："agent 自己撤回时为 `withdrawn`；中断时宿主先以 `turn_cancelled` 结束，随后的撤回不再起作用"。

### 其他

- **IT-K6-output-then-crash 的"不重发"**：
  - 宿主只在"prompt 在任何更新之前以 Session not found 失败"时恢复并重发；`crash_cli` 本身的 −32603 归为 `process_exited`，不会重发。
  - 所以两条 K6 用例里，崩溃的那条 prompt 都只发过一次；下一条消息在恢复后重发一次。
  - "先有输出、再遇到 Session not found"在假 agent 上造不出来（崩溃之后的请求都是立刻失败）。这一支由进程内用例（`gating_tests.rs`）覆盖。
- **IT-K5 的不稳定**：撤回后 prompt 立刻结束时，宿主有时先处理回合结束，把请求记成 `turn_ended`，10 次里失败了 2 次。现在撤回后剧本继续执行，prompt 晚 300 ms 结束。这个先后关系在真实的 Claude 上也可能出现，但结果同样是请求只结束一次，状态 `cancelled`。

### 验证

- `driver_k` 连跑 10 次全绿（修 IT-K5 之后的最后一轮），每次约 2.5–3.7 s，其中一次 23 s，是本机负载高的时候。跑完本 worktree 没有 `cupper-host`、假 agent 或测试进程残留。
- 合入 main（74b86a9）之后重跑，10 条全过。
