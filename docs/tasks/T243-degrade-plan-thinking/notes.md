# T243 notes

## 现状（2026-11-04 读代码）

- **能力是常量**：`interactions.plan_confirm` 和 `extraction.thinking` 在 `driver/claude/capabilities.rs` 和 `driver/codex/capabilities.rs` 里都写死为 `true`。
- **没有人读这两个字段**：宿主和界面（`packages/`、`apps/`）里都没有。界面的请求面板按请求的 `type` 渲染 `plan_confirm`，与能力无关；思考条目也一样。宿主的提取同样不看这两个字段。
- **agent 给出的选项**，按类别映射成宿主的 id：

  | agent | 原选项 | 宿主 id |
  |---|---|---|
  | Claude | `mode`（含 `plan` 值） | `permission_mode` |
  | Codex | `collaboration_mode` | `plan_mode` |
  | 两者 | `thought_level` 类别 | `reasoning_effort` |

- **`session/set_config` 的拒绝**（T141）：
  - 值不在选项里：`invalid_params`。
  - 选项整个不在：`unsupported`，`data.capability` 为这个选项。

## 方案

**方案一：只锁定现有行为，产品代码不改。** 用例断言：
- 选项或值不在了；
- 设它被拒，agent 收不到；
- 回合里没有对应的条目。

**方案二：在方案一的基础上，两个字段跟着选项走。**
- `plan_confirm`：Claude 的 `permission_mode` 有 `plan` 值，或 Codex 有 `plan_mode` 选项时为真。
- `thinking`：有 `reasoning_effort` 选项时为真。
- 约 10 行，加单元断言。
- `extraction.plan` 没有"无"这个取值，不动。

**推荐方案二**，加一条约束：这两个字段只用来决定入口（选择器、开关）显不显示。宿主照旧提取所有计划确认和思考块，不看这两个字段；界面收到计划确认请求或思考条目时，照样显示。这条约束写进设计，交接给界面。

### 推断错了的后果

| 情形 | 例子 | 后果 |
|---|---|---|
| **假阴性**：能力报 false，实际有 | Claude 的旧版适配器只在旧的 `modes` 里给 `plan`，`configOptions` 里没有 `mode`；某个模型思考强度固定、没有 effort 选项，却仍然输出思考 | 现在没有任何影响，因为没人读。将来的界面如果按字段隐藏入口，只是少一个入口：选项本来就不在，选择器里也不会有。真正危险的是界面按这两个字段藏掉"收到的"计划确认请求：请求没人能答，回合会卡到交互超时；思考条目被藏起来，用户看不到。上面那条约束就是为了排除这种用法。 |
| **假阳性**：能力报 true，实际没有 | 选项还在，agent 却不再发计划或思考 | 与现在的常量 `true` 一样，没有新的风险。 |

这两个字段是从"有没有这个选项"推断的，不是 agent 明说的。等 agent 有了真正的能力位，再改为直接读。

## 假 agent：`--without plan|thinking`

为了不和 T140（T054 正在改 `config.rs` 的 FAULTS 和 USAGE 最后一行、`agent.rs`、`script.rs` 的 steer 部分）冲突，各处的写法如下：

- **`config.rs`**：
  - 新增 `--without <plan|thinking>` 这个 match 分支和 `without: Vec<String>` 字段，加 `fn without(&str) -> bool`。
  - USAGE 只改第 2 行，与 T140 改的最后一行隔开，不相邻。
- **`persona.rs`**：`config_options` 改为接收 `&Config`。
  - `plan`：去掉 Claude `mode` 里的 `plan` 值和 Codex 的 `collaboration_mode` 选项。
  - `thinking`：去掉 `thought_level` 类别的选项。
  - 旧的 `modes` 镜像由 `session_result` 从选项生成，会一起变。
- **`agent.rs`**：只改 `open()` 里的那一处调用。
- **`script.rs`**：在 `exec` 开头加一个判断：关掉计划时，`plan_update`、`plan_confirm` 不执行；关掉思考时，`think` 不执行。位置离 T140 改的 steer 分支较远。

## 用例

`tests/driver_k.rs`，插在已有的 `it_thought_*`、`it_plan_*` 之后，不放在文件末尾，以免与 T140 在末尾追加的内容冲突。共用一个 `degraded(agent, what)`，四条用例：

- `it_degrade_plan_claude`、`it_degrade_plan_codex`
- `it_degrade_thinking_claude`、`it_degrade_thinking_codex`

每条断言：
1. 能力里没有这个值或选项。取方案二时，对应字段为 false，另一个字段仍为 true。
2. `session/set_config` 被拒：Claude 的 `plan` 值回 `invalid_params`；其余三种是整个选项不在，回 `unsupported{capability}`。见下面的问题。
3. 假 agent 的 `--calls` 里没有 `session/set_config_option`。
4. 回合正常结束，没有对应的计划或思考条目，也没有 `interaction.requested`。

不关这两项时的行为，由已有的 `it_thought_claude_streams_a_thought_item`、`it_plan_entries_*` 等用例照旧覆盖。

## 要确认的问题

1. **取方案一还是方案二**，以及方案二时那条约束（入口用，请求和条目照旧显示）要不要写进设计。
2. **任务书写的 `set_config` 回 `invalid_params`**：只有"值不在"是这样。"选项整个不在"按协议和 T141 的实现回 `unsupported`，带 `data.capability`。提议保留现状，用例按这两种分别断言。

## 规模估计

停止线 180，硬停止线 220。

| 模块 | 行 |
|---|---|
| 假 agent（`config`、`persona`、`agent`、`script`） | 30 |
| 宿主，方案二（两个 `capabilities.rs`） | 10 |
| 单元断言（两个 agent 的能力跟着选项走） | 15 |
| IT（helper 40 加四条用例 16） | 56 |
| 合计 | 约 110；取方案一时约 85 |

## 编排者的决定（2026-11-04）

- 取方案二。"能力字段只决定入口，请求和条目照旧显示"这条约束写进设计；`cupper-protocol` 的 `capabilities.rs` 两个字段的文档注释里也写了一句。
- `set_config` 保留现状：值不在时回 `invalid_params`；选项整个不在时回 `unsupported`，带 `data.capability`。用例分别断言。

## 实测中的发现：选项还不知道时，不能推成"没有"

- 照方案二第一次实现时，37 条 gold 用例（gold_a）失败了。原因是适配器 `initialize` 之后、`session/new` 之前，宿主先按空的选项列表算了一次能力，于是两个字段都成了 false。
- 这正是"推断错了"的一个现实例子：这时只是还不知道有哪些选项，不等于没有。
- 改为：agent 还没报告任何选项时，两个字段照旧为 true；报告了选项、却没有对应的那一个时，才为 false。这样 gold 文件不用改。Claude 的单元用例加了一条断言覆盖这种情形。

## 实际规模

`cargo xtask size`：+176 −21，在停止线 180 以内。比估计多出来的部分在 IT：`set_config` 要分两种错误断言，还要查数据库。
