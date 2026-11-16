# T268 笔记

## 开工前的估算（2026-11-16）

**现状**（main 893cbd15）：
- 能力缓存由 `AgentRuntime` 的 actor 写：会话接上时（`cache(caps)`）、命令列表变了时（`commands()` 里再调 `cache`）；探测也走 attach，所以同样经过 `cache`，探测任务另外再写一次，等写完才回答（`agents/refresh` 随后要读）。actor 内存里留着最近一次的 `cached`。
- 运行时经 `AgentNotes` 把 `AgentStatus` 报给索引；索引是索引流唯一的写者，`agents()` 只在 `AgentsUpdated` 有变化时追加 `agents.updated`。
- 界面的 `staleKeys` 遇到任何 `agents.updated` 都让 `["agents/capabilities"]`（两种 agent）失效。适配器启动时状态变成 starting、running，会发 `agents.updated`，界面这时重新读，读到的还是旧缓存；随后会话接上、缓存刷新，却没有事件，界面要等下一次 `agents.updated` 或重连。
- 资源采样每 30 s 一次，`rss_bytes` 变了也发 `agents.updated`，所以适配器运行期间界面每 30 s 就重新读一次能力（已有的行为，不在本任务范围）。

**两种做法**：

- **A. 复用 `agents.updated`（建议）**：`AgentStatus` 加可选字段 `capabilities_rev`，表示本次宿主运行里缓存内容变过几次。运行时在 `cache()` 里拿新能力和内存里的上一份比较，内容变了才加一，并报一次状态；索引照旧只在状态有变化时追加事件，所以内容不变就不发。
  - 这是附加的可选字段，符合 5.12.5 冻结后的规则；字段是状态，会进快照。`SCOPE` 不涉及（没有新方法）。生成物经 `pnpm gen` 生成。
  - 不用内容摘要：能力里的配置选项带 JSON 值，摘要会依赖 serde_json 的键顺序（代码约束禁止）。也不用时间戳：同一毫秒里变两次就分不出来。
  - 宿主每次启动后第一次接上会话，内存里还没有上一份，会算一次变化，多发一个事件；这发生在宿主启动时，界面本来就因为重连把查询全部作废了，无害。
  - 写数据库和通知的顺序：缓存先交给写线程，索引收到消息后再追加事件，同一个写线程按顺序提交，所以界面收到事件时缓存已经落盘。
  - 界面不用改产品代码：`agents.updated` 已经让两种 agent 的能力查询失效。如果只想让变化的那种 agent 失效，`invalidation()` 要记住每种 agent 上一次的 `capabilities_rev` 并比较；但版本变化导致缓存行被删时版本号不变，还得一并比较版本，约多 15 行。建议保持全部失效。
- **B. 加附加式事件** `agents.capabilities_updated {agent}`，像 `notification.intent` 那样不进快照：界面可以无状态地只让那一种 agent 失效。代价是一个新事件类型：Rust 的 `EventBody`、schema、两边的索引视图（忽略它）、会话视图里穷举的分支、快照构建排除它、索引的 Mail、5.3.5 的表；约 150 行，正好在停止线上。

**A 的估算**（`cargo xtask size` 口径）：

| 模块 | 内容 | 行数 |
|---|---|---|
| 协议 | `AgentStatus.capabilities_rev`（可选，带文档注释）；`pnpm gen` 的生成物不计 | 6 |
| 宿主 | 运行时的 `capabilities_rev`：`cache()` 里比较、加一、`notify()`；`status()` 带上 | 12 |
| 宿主测试 | `tests/lifecycle.rs`：两个会话接上同样的能力，`agents.updated` 里只出现一个版本号；fake agent 换一份命令列表后版本号变 | 50 |
| 界面测试 | 新建会话页：`agents.updated` 带新版本号后重新读 `agents/capabilities`，显示新命令 | 25 |
| 设计文档 | 5.6.7、5.3.5 的 `agents.updated` 一行（不计） | — |
| 合计 | | 约 95 |

编排者 2026-11-16 选 A，并要求界面只在某种 agent 的（版本、`capabilities_rev`）变化时让它的查询失效，约 110 行。

## 实现记录

- **宿主测试放在 runtime 层**：原想在 `tests/lifecycle.rs` 起宿主、看索引里的 `agents.updated`。但第一个会话接上时命令列表可能随 attach 一起到，也可能稍后到，计数是 1 还是 2 不确定；而索引追加事件和回合结束之间没有先后保证，读到的计数会有竞态。runtime 层的 `AgentNotes` 是 actor 同步调用的，回合结束时之前的通知都已经发出，结果确定。所以扩展了已有的 `unit_commands_dedup_the_same_list_is_not_sent_again`（真实的 fake agent 进程）：重复推同一份命令列表不计数，换一份加一，换回来再加一，第二个会话用同样的能力接上不计数。索引只在状态变化时追加事件，这一点是已有的逻辑。
- **界面**：`staleKeys` 不再处理 `agents.updated`；`invalidation()` 记住每种 agent 上一次的（适配器版本、CLI 版本、`capabilities_rev`），不同才让 `["agents/capabilities", agent]` 失效。新连接上清空记录（新连接本来就让全部查询失效）。`agents.updated` 经 `isKnownEvent` 收窄类型，只对这一种事件做一次校验。
- **规模**：+156 −23，超过停止线 6 行，主要是测试（宿主 42 行，界面 64 行）。已报编排者。
