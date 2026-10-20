# T089 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）
- 日期：2026-10-20

## 读到的现状（开工前）

- **会话只能由 `create` 建出来**：`SessionManager::create` 写行和事件、构造 `SessionCore` 与 `Parts`，再用 `SessionActor::run(parts, …, first, …)` 起 actor。`run` 必须带第一条消息。按需载入要另写一条路径：读 `sessions` 行、打开流、构造 `Parts`、起一个不带第一条消息的 actor；`deliver` 遇到不在内存里的会话时先走这条路径。
- **要从库恢复的内存状态**分散在几个纯核心里：
  - `TurnMachine` 的 `link`（`agent_session_id`）、`last_turn_no`、`QueueCore`（条目、版本、暂停原因）；
  - 每条排队消息的 `command_id`、`principal`、`reply_to`（在 `queue_entries`）；
  - `InteractionBroker` 需要已结束的请求（`interactions` 表，经 `Input::Ended`）；
  - `TierGuard` 的设定者（库里只有 `mode_set_by` 的 ID）；
  - 信任闸门（`session.configured.project_config` 加上恢复后的第一次重扫）。

  每个核心都要加一个"从库恢复"的入口，加上读行的代码。
- **`ignore_for_good` 没有地方存**：`sessions` 表没有对应的列，要加迁移 `0002`（例如 `project_config_ignored`），写入点在 T109 的"不加载项目配置继续"。
- **补写事件**：启动清理只改了行（T037），载入时要对 `turns.status = interrupted` 但流里没有 `turn.completed` 的回合、`interactions.status = expired` 但流里没有 `interaction.resolved` 的请求补写事件，并按 T040 的规则决定是否暂停队列。
- **检查点作废**：`streams` 在 `view_v` 不符时丢掉检查点、从日志重放。日志被修剪过时，重放得到的视图缺 `session.configured`、`session.capabilities`、`queue.updated`，要从行和缓存重发。
- **准备阶段崩溃**：`initial_prompt_json` 不为空、第一个回合没开始的会话，要把第一条消息重新交给机器。
- **空闲退出**：actor 要有自己的空闲定时器（`Clock`、`durations`），退出时从管理器摘掉句柄，静止屏障照 T047 的规则。
- **超长工具输出**：在 coalescer 和 actor 之间：超过上限写 blob（`BlobStore`，I/O 在 actor）、事件里标 `truncated`、尾部每秒节流更新。
- **零碎的交接**：
  - `StreamWriter::append` 遇到空 `bodies` 时跳过 `with`（T100）；
  - `rescan` 的 `canonicalize` 挪进 `spawn_blocking`（T109）；
  - 恢复后 `session/get` 与索引算出的状态要一致（T100）。

## 开工前的规模估计

实现按以往偏差乘 1.1，测试按写出来的估。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 按需载入 | `deliver` 里载入；读行、开流、构造 `Parts`；`run` 不带第一条消息；并发载入同一会话时只起一个 actor | 110 | — |
| 核心的恢复入口 | `TurnMachine`（链接、回合号）、`QueueCore`（条目、版本、暂停、`command_id`、`reply_to`）、`TierGuard` 的设定者、信任闸门的原因 | 120 | — |
| 交互 | `Input::Ended` 载入已结束的请求；`interaction/get` 对淘汰的请求改读表 | 70 | — |
| 读行 | `sessions`、`queue_entries`、`interactions`、`turns` 的读取与解析 | 90 | — |
| `ignore_for_good` | 迁移 `0002`、写入与读回；`canonicalize` 挪进 `spawn_blocking` | 45 | — |
| 补写事件 | 中断的回合、过期的请求；是否暂停队列 | 60 | — |
| 检查点作废时重发 | 三类事件从行和能力缓存重写 | 45 | — |
| 准备阶段崩溃 | 重开第一个回合，第一条消息不丢 | 40 | — |
| 空闲退出 | 空闲定时器、摘句柄、静止屏障；退出后再投递 | 80 | — |
| 超长工具输出 | 上限、写 blob、`truncated`、尾部每秒节流 | 120 | — |
| 零碎 | `append` 的空 `bodies`；`session/get` 与索引一致 | 35 | — |
| IT-orch-recovery | 1–7 各条加一致性：重启后 `send`、队列与 `reply_to`、`interaction/get`、补写、三条重发、准备阶段崩溃、空闲退出后再投递、超长输出与节流 | — | 420 |
| **合计** | | **约 815，乘 1.1 约 900** | **约 420** |

合计约 **1,320 行**，远超约 850 的停止线。按任务书和守则第 8 条，开工前停下汇报，没有写代码。

## 拆分建议

| 拆分 | 内容 | 验收 | 估计 |
|---|---|---|---|
| **T089a 载入与恢复**（挡 T062、T069 的部分） | 按需载入；各核心从库恢复（含 `command_id`、`reply_to`、设定者、信任闸门与 `ignore_for_good` 的持久化）；`Input::Ended` 与 `interaction/get` 读表；补写中断与过期；检查点作废时重发；准备阶段崩溃重开；`append` 的空 `bodies`；`session/get` 与索引一致 | IT-orch-recovery 的 1–5 条与一致性 | 实现约 670，乘 1.1 约 735；测试约 280；**约 1,000** |
| **T089b 空闲退出与超长输出** | actor 空闲退出（依赖 a 的按需载入）；超长工具输出转 blob、`truncated`、尾部节流 | IT-orch-recovery 的 6、7 条（或各给一个新编号） | 实现约 200，乘 1.1 约 220；测试约 140；**约 360** |

a 仍然超出 850。要压到线以内，可以再把"检查点作废时重发"和"准备阶段崩溃重开"（约 130 加测试约 70）移进 b，或者单独成一份：

| 三分 | 内容 | 估计 |
|---|---|---|
| **T089a 载入与恢复** | 按需载入；各核心恢复与 `ignore_for_good`；`Ended` 与读表；补写；零碎 | 约 800 |
| **T089b 崩溃后的重建** | 检查点作废时重发；准备阶段崩溃重开 | 约 200 |
| **T089c 空闲退出与超长输出** | 同上 | 约 360 |

- 依赖：b、c 都要在 a 之后（重发和重开发生在载入时，空闲退出后的再投递靠按需载入）。b 与 c 之间相互独立，可以并行。
- T062、T069 至少要 a；T069 的"故障点与第一条消息不丢"还要 b。
- 另一种：不拆，上限提到约 1,400。

## 裁定后（计划 v1.3.38）

范围收窄为载入与恢复，上限约 900 行，硬停止线 980。检查点作废时重发、准备阶段崩溃重开归 T169；空闲退出、超长输出归 T175。

### 设定者的恢复

库里只有 `mode_set_by` 的 ID 和 `full_access_confirmed_at`。按 ID 找回握手时的 principal：

| `mode_set_by` | 找回为 | 上限 |
|---|---|---|
| `pr_desktop`，有 `full_access_confirmed_at` | 桌面 control 角色 | `full_access` |
| `pr_desktop`，没有 | 桌面 window 角色 | `elevated` |
| `pr_cli` | CLI（只有 `read`） | `normal` |
| `pr_test`（只在测试构建里存在） | 测试 harness | `full_access` |
| 其他、空 | 来源不明：按远程设备算 | `normal` |

- 桌面两个角色共用一个 ID。两者的结论只在"档位是 `full_access` 且已确认"时不同（control 不需要确认，window 是 `tier_above_setter`）。完全访问只有 control 能确认，所以有确认记录时按 control 算。这之后若由 window 改了设定者、agent 又自己把档位提上去，崩溃前就已经是 `config_unconfirmed`；这个状态写在 `session.configured.confirmation_required` 里，载入时照原样恢复，不会因为按 control 算而丢掉。
- **"由创建者设定"与"来源不明"两种假设的差别**：
  - 由创建者设定：创建时的策略检查已经通过，等于把会话当前的档位当作设定者可设的档位。不会出现 `tier_above_setter`；`full_access` 没有确认记录时仍是 `full_access_unconfirmed`。
  - 来源不明：上限按 `normal` 算。档位是 `elevated` 或 `full_access` 时都是 `tier_above_setter`。
  - 两者只在档位高于 `normal` 时结论不同。按裁定取"需要确认"，即 ID 找不到时按"来源不明"处理。
- 载入时守卫用库里最后的权限模式立即检查一次。所以恢复出的设定者更保守、结论变成需要确认时，在载入时就进入 `config_unconfirmed`，不必等第一次连接。

### 信任闸门与 `ignore_for_good`

- `ignore_for_good` 存在 `sessions.project_config_ignored`。
- 计划 P-4 规定 T160 之前表结构只改 `0001`，所以列加在 `0001` 里，没有另写 `0002`。裁定写的是"迁移 `0002`"，这一点在汇报里说明。
- "想加载项目配置"按以下规则重新得出：`session.configured.project_config` 是 `loaded`，或者队列因 `project_config_changed` 暂停，并且没有选过"不加载项目配置继续"。之后由第一次重扫按原规则更新。
- 闸门的原因按以下规则恢复：
  - `config_unconfirmed`：来自 `session.configured.confirmation_required`；
  - `project_config_changed`：来自队列的暂停原因。队列为空时这个原因不留痕；下一条消息的检查会重新得出。

### 补写与队列

- 补写以会话流的视图为准：
  - 视图里最后一个没有 `turn.completed` 的回合，补写 `turn.completed{interrupted}`，`ended_at` 用启动清理写进 `turns` 行的时间；
  - 视图里仍待处理的请求，补写 `interaction.resolved{expired, host_restarted}`，写在 `turn.completed` 之前。
- 回合里没结束的条目不补写，不在本任务范围内（见汇报）。
- 载入后没有回合在运行。队列里有消息、又没有暂停时，一律以 `turn_interrupted` 暂停，等用户继续，不在重启后自己开始回合。
