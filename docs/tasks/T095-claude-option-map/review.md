# T095 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：由 T053 拆出（计划 v1.3.14）。合并 main（没有冲突）后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-7b6676b5ce9a.json`。执行者那次的 `pr-a55faed5f8c0.json` 一并保留。
- 核对：没有新依赖；没有个人信息，金样例过了通用扫描。harness 给这份汇报加了"疑似指令"的标记（bypass-permissions），触发它的是档位表里 Claude 的权限模式名 `bypassPermissions`，是正常内容。
- 审阅：
  - **档位表**与设计 5.6.5 一致：`default`、`plan` 为 `normal`，`acceptEdits`、`auto` 为 `elevated`，`bypassPermissions` 为 `full_access`，表外取值按 `full_access`。
  - **选项对照表**按 5.3.6：`allow-with-updates` 在文件编辑上只对本会话有效，在命令和未知工具上按持久（Q7）；计划确认的选项带 `mode_after`、`tier_after`，`reject` 为 `plan_revise`。只有选项 ID 和 kind 都与表一致才查表，否则按 kind 兜底，kind 未知按最严。
  - **"取值都在表里"读录制里的原始取值**，不看提取结果，兜底掩盖不了表里的缺项。认可，这是这项检查的要点。
  - **金样例的变化**逐份说明（17 份的 `risk` 按档位表、s02/s04/s07 的 Write/Edit、s03/s04b 的前缀与风险、s05 的计划确认），与表的改动一一对应；执行者用脚本核对过除 `risk` 以外没有别的字段变化。
  - **`write_held` 填 `item_id`**：按 `tool_call_id` 经 `Coalescer::item_of` 找到条目，找不到时保持空，T053 合并时留下的 `..` 已接上。
- 规模：代码 +540 −69 行（不含金样例和 notes），在约 700 的上限以内。
- 转交事项已写进 `handoff-notes.md`：T044（档位用 `capabilities::tier()`）、界面任务（`permission.tool.item_id` 仍为空，需要时另补）。Q7 是否向上游反馈仍待用户决定（计划第 6 节）。
