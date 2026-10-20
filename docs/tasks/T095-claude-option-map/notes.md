# T095 工作笔记

## 开工前的核对

- 12 个权限请求的工具调用都带 `name`（Write、Edit、Bash、ExitPlanMode），`_meta` 是 null；工具名按 5.6.4 先看 `_meta.claudeCode.toolName`，没有时看 `name`，都没有就判断不出来，`allow-with-updates` 按持久处理（Q7）。
- Bash 的 `allow-with-updates` 原文带前缀：`Yes, and don't ask again for <前缀> commands`，或 `Yes, and allow access to <目录> and <前缀> * commands`。前缀先从原文解析，解析不出时取 `rawInput.command` 的第一个词。
- 金样例里每份录制的 `session` 行都带配置项，档位表换上后 `permission_mode` 各取值的 `risk` 都会变，所以几乎每份金样例都会变。

## 规模估计（开工前）

上限约 700 行（含测试，不含金样例）。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `driver/claude/options.rs` | 按工具区分的对照表、按 kind 兜底、未知 kind、前缀与 `prefix_risk` | 130 | 90 |
| `ask.rs` | 换成查表（净改动） | 20 | — |
| `capabilities.rs` | 档位表，`risk` 按表给 | 20 | 10 |
| `gold_a.rs` | "取值都在表里"的检查，含一份带未知取值的内联录制 | — | 90 |
| 合并器与 `SessionCore` | 按 `tool_call_id` 找条目，`write_held` 填 `item_id`，一条用例 | 15 | 35 |
| 合计 | | 185 | 225 |

合计约 **410 行**，在上限以内。

## 进度统计

不含金样例和本笔记。

| 提交 | 内容 | 行数 | 累计 |
|---|---|---|---|
| 1 | 选项对照表 `options.rs`（实现约 180，UNIT-option-map-claude 约 170）、`ask.rs` 改为查表、`capabilities.rs` 的档位表、unknown-enums 用例的期望；重新生成 17 份金样例 | +392 −58 | 392 |
| 2 | "取值都在表里"的检查与两个用例（GOLD-A-option-map-claude） | +84 −1 | 476 |
| 3 | 合并器 `item_of`、`SessionCore` 的 `write_held` 填 `item_id`、一条用例 | +64 −11 | 540 |

合计 +540 −69，在 700 行的上限以内。选项表的测试比估计多约 80 行：用例改成文本表之后仍有前缀和场景两组。

## 取舍

- **场景**：`Scene::of` 先看 ExitPlanMode（`switch_mode` 加 `exit-plan-` 选项），再看工具名（`_meta.claudeCode.toolName`，没有时看工具调用的 `name`）：Write、Edit、MultiEdit、NotebookEdit 是文件编辑，其余工具是命令及其他，没有工具名就是 `UnknownTool`，`allow-with-updates` 按持久处理（Q7）。
- **查表要求 kind 也一致**：表里的选项 ID 配上不一致的 kind（例如 `allow-once` 却是 `allow_always`），不信任 ID，按 kind 兜底。
- **持久选项的规则**：`effect.rule` 只填 `prefix` 和 `prefix_risk`；适配器没有在 `_meta` 里透出规则原文和写入位置，`text`、`location` 留空。前缀先从 Claude 的原文解析（"don't ask again for <前缀> commands"、"allow access to <目录> and <前缀> * commands"），解析不出时取命令的第一个词；第一个词（去掉路径）是解释器一类（shell、`node`、`python*`、`env`、`sudo`、`xargs` 等）时为 `interpreter`，没有前缀时为 `unknown`。kind 未知的选项同样是持久的，也带 `prefix_risk: unknown`。
- **计划确认**：`exit-plan-default` 带 `mode_after: default`、`tier_after: normal`；`exit-plan-auto`、`exit-plan-clear-auto` 是 `auto`（`elevated`）；`exit-plan-bypass` 是 `bypassPermissions`（`full_access`）。`tier_after` 由档位表算出。
- **档位表**：`default`、`plan` 为 `normal`，`acceptEdits`、`auto` 为 `elevated`，`bypassPermissions` 为 `full_access`，表外的模式按 `full_access`。
- **"取值都在表里"**：对每份 Claude 录制，检查权限请求的每个选项在其场景的表里、工具调用（含权限请求里的）的 kind 是已知的、配置项和 `current_mode_update` 里的权限模式都在档位表里。只看录制里出现的取值，不看提取结果，这样兜底掩盖不了缺项。
- **`item_id`**：`write_held` 写请求时按 `tool_call_id` 在合并器里找工具调用的条目；迟到输出的工具调用也指向原条目。只填 `interaction.requested` 的 `item_id`，`permission.tool.item_id` 没有动（brief 只要求前者）。

## 金样例的变化

17 份变化，`s08-list` 没变（没有会话配置项和权限请求）。配置项的变化只有 `risk` 一个字段，已用脚本核对其余字段不变。

| 金样例 | 变化 |
|---|---|
| 全部 17 份 | `session` 行和 `config_changed` 事件里，`permission_mode` 各取值的 `risk` 从一律 `full_access` 改为按档位表：`default`、`plan` 为 `normal`，`acceptEdits`、`auto` 为 `elevated`，`bypassPermissions` 仍为 `full_access` |
| s02、s04、s07 | Write、Edit 的 `allow-with-updates` 从 `persistent`（持久）改为 `session`（不持久），`requires` 仍为 `privileged` |
| s03 | Bash 的 `allow-with-updates` 仍为持久，新增 `rule`：前缀 `sh -c '…'`，`prefix_risk: interpreter` |
| s04b | Bash 的 `allow-with-updates` 仍为持久，新增 `rule`：前缀 `printf d`，`prefix_risk: normal` |
| s05 | Write 的 `allow-with-updates` 改为 `session`；计划确认：`exit-plan-default` 从 `once` 改为 `plan_accept`（`mode_after: default`、`tier_after: normal`），`exit-plan-auto`、`exit-plan-clear-auto` 从持久改为 `mode_switch`（`auto`、`elevated`），`exit-plan-bypass` 改为 `mode_switch`（`bypassPermissions`、`full_access`），`reject` 从 `reject` 改为 `plan_revise` |
