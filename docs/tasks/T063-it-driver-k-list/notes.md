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
