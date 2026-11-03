# T111 工作笔记

## 开工前的核对

- 运行时（`driver/runtime/mod.rs`）里写死 Claude 的地方：提取层（`ClaudeSession`）、错误分类（`Wire::Opened`、`Wire::Configured`、`Wire::Prompted` 三处）、能力（`claude::capabilities`）、`session/new` 的 `_meta`、能力缓存的作废、状态里的 `agent`、Q43 的告警；`launch.rs` 里的版本范围、适配器版本、环境变量名、设置里的 CLI 路径、`adapters` 行的 `agent`；`agents.rs` 只认 Claude；安装器只装 Claude 的组件，`adapter_state` 和索引的运行时状态只算 Claude。
- 信任门控：`policy::attach` 已经对未信任的 Codex 返回 `unavailable/project_untrusted`（`session/create`），但 Codex 没有注册进会话管理器，所以现在是 `unavailable`（no such agent）。会话运行中项目变为 `stale` 时，actor 会以 `ProjectConfigPolicy::Ignore` 重新 attach；codex-acp 不能关掉项目配置，`session/new` 就会拉起项目里的 MCP，所以运行时自己必须拒绝（设计 5.6.2）。
- `RunState`：Codex 提取层已经产出，`AutonomousJudge` 已经消费；只差运行时把 Codex 的事件转过去。
- 回收：会话收到的 `AdapterExited` 会让回合以 `adapter_exited` 失败、写 `agent_restarted` 通知。回收不是崩溃，空闲的会话不该收到这条通知，所以给 `AdapterExited` 加 `reclaimed`。
- 测试进程里的 CLI 发现（T203）：进程级用例经 `spawn_from` 把 `HOME` 和 `cli_root` 挪到临时目录，看不到本机的 codex；但进程内启动宿主的两处（`supervisor` 的 actor 种类用例、`gold_b`）用的是用户自己的 `HOME`，Codex 的运行时一旦走生产发现，就会在 `/usr/local/bin/codex` 找到本机的 codex 并运行 `--version`。这两处要给 Codex 也注入程序（`gold_b` 的进程内适配器改为对两种 agent 都生效）。
- codex 的 npm 布局（只读了 jsdelivr 上 `@openai/codex@0.163.0/bin/codex.js` 的源码，curl 默认 User-Agent，没有安装、没有运行）：入口 `bin/codex.js` 先 `require.resolve("@openai/codex-darwin-arm64/package.json")`，取它旁边的 `vendor/aarch64-apple-darwin/bin/codex`；找不到平台包时退回包内的 `vendor/aarch64-apple-darwin/bin/codex`（SP-2 记录的 0.159.2 就是这种）。

## 规模

### 开工前的估计

停止线 900、硬停止线 1100。共用代码的搬动（运行时按 agent 分派，行为不变）单独一个提交，按净增计。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 搬动：`runtime/agent.rs` 与运行时分派 | `Agent`（Claude、Codex）：版本范围、变量名、布局、分类器、能力、`_meta`；提取层枚举；`launch`、`status`、`cache`、`agents` 按 agent | 净增约 120 | — |
| `locate/resolve.rs` | `CODEX` 布局 | 30 | 60（UNIT-cli-resolver-codex） |
| 安装器 | codex-acp 的计划（包内复制）、按 agent 的运行时状态、索引按 agent 记运行时 | 50 | 15 |
| 未信任 | Codex 收到 `Ignore` 直接拒绝；`AgentErrorClass`、`FailureCode` 加 `project_untrusted` | 20 | 50（IT-codex-untrusted） |
| 回收 | 在途 prompt、线程状态、最近活动；采样时判断；回收走关闭顺序，`AdapterExited{reclaimed}`；回收期间的 attach 等新实例；回合机不写通知 | 90 | 80（IT-K20-reclaim） |
| `agents/restart` | 路由、`AgentRuntime::restart` | 35 | 25 |
| 宿主装配 | 两个运行时、两个 Finder、设置变更时两边都重新发现 | 50 | 10 |
| T218 对 Codex | 现有用例按两种 agent 跑 | — | 15 |
| 版本探测重试（T066 交接） | 超时后 30 s 再试一次 | 10 | 15 |
| 小计 | | 搬动净增约 120 + 285 | 270 |

合计约 675，余量用来吸收 rustfmt 的展开。

### 实际

`cargo xtask size` 按增加的行数算（不含生成物和 `docs/`）：

| 提交 | 增 | 删 | 说明 |
|---|---|---|---|
| 搬动：运行时按 agent 分派 | 376 | 168 | 按净增计 208（`agent.rs` 的单变体分派和宿主装配的缩进占了大头） |
| Codex 的 CLI 布局 | 78 | 0 | 其中用例 60 |
| Codex 的运行时（安装器、索引、宿主装配） | 148 | 58 | |
| 未信任的项目 | 65 | 2 | 其中用例 50；另有 `pnpm gen` 的生成物 |
| 回收 | 231 | 30 | 其中用例 105 |
| `agents/restart`，T218 对 Codex | 57 | 17 | 其中用例 23 |
| 版本探测重试 | 34 | 7 | 其中用例 20 |
| 四个小修 | 42 | 11 | 自检报告的期望；回收时把在途的 attach 算作工作、代理 10 s 内有输出时不回收；回收或重启期间的 prompt 以 "Session not found" 失败、由会话 resume 后重发（含用例） |

合计增 1016、删 278；搬动按净增计时是 1016 − 376 + 208 = 848，在停止线 900 之内。`cargo xtask size` 不认搬动提交，显示越过停止线 116（硬停止线 1100 之内）。

## 过程中的发现

- `cupper-host` 的进程级用例 `it_cli_commands_read_elevate_write_and_answer`、`it_elevate_lineage_waits_limits_expiry_grant_and_close` 需要带 `test-harness` 的 `cupper` CLI（`cargo xtask ci` 的 `cli-e2e` 一步会构建）；本机只 `cargo build -p cupper-cli` 时这两个用例会失败，与本任务无关。
- 用 `ManualClock` 的 `tick` 推进时钟时，它在 future 挂起的每个间隙都把时钟推到下一个截止时间，不等真实 I/O：回收用例要先等真实的事件（线程变为 active、prompt 返回），再推时钟到下一次采样，否则采样会在 prompt 真正发出或返回之前发生。
- `agents/restart` 返回时适配器还在关闭：会话还没收到 `AdapterExited` 就发出的 prompt，若在关闭期间到达运行时，以 "Session not found" 失败并由会话 resume 后重发（K6）；若在实例结束之后才到，运行时丢弃它，回合由随后的 `AdapterExited` 以 `adapter_exited` 结束（与崩溃时相同）。进程级用例因此先等 `agent_restarted` 通知再发下一条消息。
