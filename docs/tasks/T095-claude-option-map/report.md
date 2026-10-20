# T095 汇报

- 执行者：子 agent（工程师，驱动层）
- 分支：`feat/T095-claude-option-map`，在原 T052 的 worktree 里从 main 开出
- 完成日期：2026-10-20

## 做了什么

T095 的五项都已完成。在干净的 worktree 里跑 `cargo xtask ci pr --record` 通过，摘要已提交。代码共 +540 −69 行，在约 700 行的上限以内；这个数不含金样例和 notes。

1. **按工具区分的选项对照表**：新增 `driver/claude/options.rs`，`ask.rs` 里原来的 `option()` 换成查这张表。
   - 先按 `Scene::of` 判断请求属于哪一类：
     - ExitPlanMode 的请求（`switch_mode` 加 `exit-plan-` 选项）是计划确认；
     - 工具名先看 `_meta.claudeCode.toolName`，没有时看工具调用的 `name`。Write、Edit、MultiEdit、NotebookEdit 是文件编辑，其余工具是命令；
     - 没有工具名时归为"未知工具"。
   - 表的内容照 5.3.6：
     - `allow-with-updates` 在文件编辑上只对本会话有效，在命令和未知工具上算持久（Q7：判断不出就按持久）；
     - 计划确认的选项带 `mode_after` 和 `tier_after`；计划确认里的 `reject` 是 `plan_revise`。
   - 只有选项 ID 和 kind 都与表一致才查表；不在表里的选项，或 kind 不一致的，按 kind 兜底；kind 未知的选项按最严处理。
   - 持久选项带 `effect.rule`：前缀先从 Claude 的选项原文里解析，解析不出时取命令的第一个词；`prefix_risk` 分 `interpreter`、`normal`、`unknown` 三档。
2. **档位表**：`capabilities.rs` 新增 `TIERS` 和 `tier()`，替换原来给 `permission_mode` 每个取值都标 `full_access` 的过渡写法。
   - `default`、`plan` 是 `normal`；`acceptEdits`、`auto` 是 `elevated`；`bypassPermissions` 是 `full_access`；
   - 表里没有的模式按 `full_access`（T031 的交接）。
3. **"取值都在表里"的检查**（GOLD-A-option-map-claude）：回放每份 Claude 录制，下面三类取值有一个不在表里就失败：
   - 权限请求的每个选项，要在它所属场景的表里；
   - 工具调用的 kind（包括权限请求里的），要是已知的；
   - 配置项和 `current_mode_update` 里的权限模式，要在档位表里。

   检查读的是录制里的原始取值，不看提取结果，这样兜底逻辑掩盖不了表里的缺项。另有一份内联录制，带一个未知选项、两个未知模式、一个未知工具 kind，四个都被报出。
4. **重新生成金样例**：17 份有变化，`s08` 没变。变化见"关键取舍"之后的表。
5. **`write_held` 填 `item_id`**（T088 的交接）：
   - `SessionCore` 收到请求时一并记下 `tool_call_id`；写请求时，用新增的 `Coalescer::item_of` 按它找到工具调用的条目，填进 `interaction.requested` 的 `item_id`，找不到时保持空；
   - 用例 `it_orch_interaction_request_names_its_tool_calls_item` 同时覆盖找得到和找不到两种情况。

## 关键取舍

- **持久选项只填前缀和风险**：适配器没有在 `_meta` 里透出规则原文和写入位置，所以 `effect.rule.text` 和 `location` 留空。
- **前缀的解析**：Claude 的原文有两种写法，一种是 "don't ask again for <前缀> commands"，一种是 "allow access to <目录> and <前缀> * commands"。
- **解释器名单**：shell 一类、`node`、`python*`、`env`、`sudo`、`xargs`、`npx` 等，判断前会先去掉路径，所以 `/usr/bin/env` 也算解释器。
- **kind 未知的选项也带 `prefix_risk: unknown`**：它本来就算持久。为此改了 GOLD-A-unknown-enums 的一处期望。
- **`item_id` 只填了一处**：只填 `interaction.requested.item_id`，`permission.tool.item_id` 没有动，因为任务书只要求前者。如果界面也需要后者，可以另行补上。

**金样例的变化**（全文也在 `notes.md` 里）：

| 金样例 | 变化 |
|---|---|
| 全部 17 份 | `session` 行和 `config_changed` 事件里 `permission_mode` 各取值的 `risk` 改按档位表，见上面第 2 项。我用脚本核对过，这些行除了 `risk` 没有别的字段变化 |
| s02、s04、s07 | Write、Edit 的 `allow-with-updates` 从持久改为 `session`，`requires` 仍为 `privileged` |
| s03 | Bash 的 `allow-with-updates` 仍为持久，新增 `rule`：前缀 `sh -c '…'`，`prefix_risk: interpreter` |
| s04b | Bash 的 `allow-with-updates` 仍为持久，新增 `rule`：前缀 `printf d`，`prefix_risk: normal` |
| s05 | Write 的 `allow-with-updates` 改为 `session`。计划确认的选项改为：`exit-plan-default` 为 `plan_accept`（`default`、`normal`）；`exit-plan-auto`、`exit-plan-clear-auto` 为 `mode_switch`（`auto`、`elevated`）；`exit-plan-bypass` 为 `mode_switch`（`bypassPermissions`、`full_access`）；`reject` 为 `plan_revise` |

## 验证方式

- 在干净的 worktree 里于 `a55faed` 跑 `cargo xtask ci pr --record docs/tasks/T095-claude-option-map/ci/`，全部通过。摘要是 `pr-a55faed5f8c0.json`，提交为 `164137f`。

  | 作业 | 结果 |
  |---|---|
  | rust | 1,431 通过 / 0 失败 |
  | protocol | 185 / 0 |
  | ts | 323 / 0 |
  | license | 1 / 0 |
  | testdata-scan | 3 / 0，本机没有敏感词清单，`sensitive-words` 跳过 |
  | ui、e2e | 还没实现 |

- 对照验收标准：
  - GOLD-A-option-map-claude 通过，带未知取值的内联录制按预期报出全部四项。✅
  - UNIT-option-map-claude 通过，由三个用例组成：各场景下每个选项的效果（含按工具区分 `allow-with-updates`、兜底和"判断不出按持久"）、场景的判断、前缀与风险。✅
  - 重新生成的金样例已逐份说明变化。✅
  - `write_held` 填 `item_id` 的用例通过，在摘要里计入 IT-orch-interaction。✅
  - 测试名都以编号开头；两个编号本来就在 `tests/ids.toml` 的 T095 段里。✅
- 金样例和摘要都过了通用扫描，没有命中。

## 提交列表

```
164137f docs(task): record the T095 pr CI summary
a55faed docs(task): record the T095 line counts, choices and golden changes
194a77a feat(host): name the tool call's item in interaction requests
e36e6f9 test(host): check that recorded values are in Claude's tables
8ffbb7b feat(host): add Claude's option table and tier table
d1a1fa1 docs(task): record the T095 checks and size estimate
```

## 遗留问题 / 范围外发现

1. **`permission.tool.item_id` 仍为空**：只填了请求级别的 `item_id`（见"关键取舍"）。
2. **Q7 的上游反馈**：仍待用户决定。反馈落地之后，适配器如果在 `_meta` 里透出规则原文或写入位置，就可以填进 `effect.rule.text` 和 `location`。
3. **给 T044**：档位用 `capabilities::tier()`。能力里 `permission_mode` 各取值的 `risk` 已经按档位表给出，表外的模式是 `full_access`。
4. worktree 已保留。