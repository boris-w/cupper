# T095 Claude 选项对照表与档位表

- 派发日期：2026-10-20
- 执行者：子 agent（工程师，驱动层）。T052、T053 的执行者接手
- 分支：`feat/T095-claude-option-map`
- 依赖：T053（合并中）。T044（档位核对与信任门控）依赖本任务

## 背景

T053 开工前估计约 1,690 行，拆成两部分（计划 v1.3.14，见 `docs/tasks/T053-claude-extraction/brief.md` 末尾的补充裁定）。T053 已经交出提取层、能力提取和 A 层运行器，选项先按 5.3.6 的"按 kind 兜底"和"kind 未知"两行定效果（结果保守：`allow_always` 一律算持久、`privileged`），配置项的 `permission_mode` 每个取值都标 `full_access`。本任务换上按工具区分的完整对照表和档位表。

## 范围

**做**（计划 T095 一行；设计 5.6.4、5.6.5、5.3.6；Q7）：
1. **按工具区分的选项对照表**：给每个权限选项标出 `effect` 和 `requires`；按工具区分 `allow-with-updates`；前缀风险。Q7 的规则：判断不出按持久。替换的位置是 T053 的 `ask.rs` 里的 `option()`。
2. **档位表**：模式到档位的映射（5.6.5），替换 `capabilities.rs` 里给 `permission_mode` 每个取值都标 `full_access` 的过渡写法；映射里没有的模式按 `full_access`（T031 的交接）。
3. **"取值都在表里"的检查**：A 层回放时，提取出的选项 kind、工具、模式等取值都必须在表里，出现不在表里的值就失败（GOLD-A-option-map-claude）。
4. **重新生成受影响的金样例**：换表后 s04、s04b、s05 等的金样例用 `UPDATE_GOLDEN=1` 重新生成；在汇报里逐份说明变化，编排者评审金样例的 diff。
5. **`SessionCore::write_held` 填 `item_id`**（T088 的交接，T053 合并时没能做）：`DriverEvent::Ask` 已带 `tool_call_id`，按它在合并器里找到对应条目，写进 `interaction.requested` 的 `item_id`；找不到时保持空。T053 合并时，`core.rs` 里对 `Ask` 的匹配用 `..` 暂时忽略了这个字段，本任务接上。补一条用例。

**不做**：
- Codex 的对照表（T110）；
- 档位核对与 `config_unconfirmed`（T044）。

## 验收标准

- [ ] GOLD-A-option-map-claude：取值都在表里的检查通过；构造一份带未知取值的内联录制，检查失败。
- [ ] UNIT-option-map-claude：每个工具的每种选项的 `effect`、`requires`；`allow-with-updates` 的按工具区分；前缀风险；判断不出时按持久。
- [ ] 重新生成的金样例逐份说明变化原因。
- [ ] `write_held` 填 `item_id` 的用例。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T095 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T095-claude-option-map/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T053：`ask.rs` 的 `option()` 是换表的位置；换表后 s04、s04b、s05 等要重新生成并评审。
- T088：`write_held` 填 `item_id`。
- T031：`session/create`、`set_config` 的档位用能力里的 `risk` 换算成 `Tier`，映射里没有的模式传 `FullAccess`。

## 注意

- 不运行真实的 Claude Code CLI，不登录，不联网。
- 代码约束：提取层是纯函数；导出给其他语言的产物不依赖 serde_json 的 map 键顺序。
- Q7 是否向上游反馈由用户决定（计划第 6 节），本任务只按"判断不出按持久"实现。

## 规模

计划估为 M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**（金样例不计）。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 700 行（包括贴线）时，必须先停下来汇报**。

## 参考资料

- 设计：`06-agent-driver.md` 5.6.4、5.6.5；`03-protocol-v1.md` 5.3.6；`13-open-issues.md` Q7。
- `docs/tasks/T053-claude-extraction/`（`notes.md`、`review.md`）、`docs/tasks/T088-orch-interaction/`、`docs/tasks/T031-policy/`。
- `crates/cupper-host/src/driver/claude/`、`orchestrator/session/core.rs`。

## 工作方式

T053 合并之后，在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b feat/T095-claude-option-map main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
