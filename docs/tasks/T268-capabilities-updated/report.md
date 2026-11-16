# T268 汇报

- 执行者：子 agent（前端，ui-core；T153 的执行者）
- 分支：`fix/T268-capabilities-updated`（PR #126，0243f434）
- 完成日期：2026-11-16

能力缓存的内容变了，宿主现在会经 `agents.updated` 通知界面，内容不变就不发；界面据此只重读那一种 agent 的能力。PR #126 的 8 个检查全部通过。规模 +156 −23，超出停止线 6 行，你已同意。

## 做了什么

**协议**
- `AgentStatus` 加了可选字段 `capabilities_rev`：本次宿主运行里，能力缓存的内容变过几次。
- 这是附加的可选字段，会进快照，没有新方法。生成物已经过 `pnpm gen` 更新。

**宿主**
- 运行时在 `cache()` 里拿新能力和手里的上一份比较。只有内容不同时才把计数加一，并通知索引。
- 会话接上、命令列表变化、探测这三种刷新都经过 `cache()`。
- 索引本来就只在状态有变化时追加 `agents.updated`，所以内容相同时计数不变，也不发事件。
- 缓存先交给写线程，索引随后才追加事件；同一个写线程按顺序提交，所以界面收到事件时缓存已经写进库了。

**界面**
- `staleKeys` 不再处理 `agents.updated`。
- `invalidation()` 记住每种 agent 上一次的三个值：适配器版本、CLI 版本、`capabilities_rev`。只有不同时才让 `["agents/capabilities", agent]` 失效。
- 新连接时清空这份记录，新连接本来就会让全部查询失效。
- 顺带去掉了"适配器运行期间每 30 秒重读一次能力"：以前资源采样一变就会触发重读。

**设计文档**（已改在分支上）
- 5.3.5：`agents.updated` 那一行加上 `capabilities_rev`，"何时写入"补上能力缓存变化。
- 5.6.7：新增"变化的通知"一条。

## 关键取舍

- **复用 `agents.updated`，不加新事件**：新事件约 150 行，还要改事件类型目录、两边的视图和快照规则；加字段约 95 行。你选了复用。
- **用计数，不用摘要或时间戳**：能力里的配置选项带 JSON 值，算摘要会依赖 serde_json 的键顺序，代码约束禁止这样做；时间戳在同一毫秒内变两次就分不出来。
- **宿主每次启动后，第一次刷新算一次变化**：这时手里还没有上一份。宿主刚启动时界面本来就会重连、作废全部查询，所以多这一次无害。
- **宿主测试放在 runtime 层，没写进 `tests/lifecycle.rs`**：
  - 在索引事件流上，第一个会话的命令列表可能随 attach 一起到，也可能稍后到，计数是 1 还是 2 事先不知道；索引追加事件和回合结束之间也没有先后保证，断言会有竞态。
  - runtime 层的通知由 actor 同步发出，结果确定，用的也是真实的 fake agent 进程。
  - 所以我扩展了已有的 `unit_commands_dedup_the_same_list_is_not_sent_again`。没有新增测试编号，符合计划里"不新增编号"。你已同意。

## 验证方式

**本机**
- `pnpm lint`（含 fmt 和 clippy）通过。
- `cargo xtask ci protocol` 通过：193 项，schema-compat、integers、shell-api-m1a 都过。
- ui-core 182 项、宿主 runtime 20 项、protocol 100 项通过；扩展后的宿主测试连跑 5 次都通过。
- trace-ids check 通过。

**验收标准**
- [x] 宿主测试和界面测试都覆盖了。
  - 宿主测试（真实 fake agent，确定结果）：
    - 同一份命令列表重复推送，不计数；
    - 换一份列表，加一；
    - 换回原来的列表，再加一；
    - 第二个会话用同样的能力接上，不计数。
  - 界面测试：
    - `query.test` 逐条核对：资源采样和状态变化不让查询失效；计数变化只让 Claude 的失效；Codex 的 CLI 版本变化只让 Codex 的失效；连接后的第一个事件让两种都失效。
    - 新建会话页的测试核对：收到计数后会重新读 `agents/capabilities`。
  - 协议改动已经过 `pnpm gen`；CI 的 protocol 检查（含 schema-compat）通过。
- [x] PR 上 8 个检查全部通过：https://github.com/boris-w/cupper/pull/126 （运行 https://github.com/boris-w/cupper/actions/runs/43842579919 ）

**规模**：+156 −23，超出停止线 6 行，多出的都在测试里，已报并获同意。

## 提交列表

```
0243f434 docs(design): capabilities_rev in agents.updated (5.3.5, 5.6.7)
d4c06c95 fix(ui-core): an agent's capabilities go stale when its cache may have changed
e40952ca fix(host): count the capability cache's changes in agents.updated
```

## 遗留问题 / 范围外发现

- 无。设计文档已改在分支上，合并时不用再补。
