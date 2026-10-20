# T053 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：
  - 开工前估计约 1,690 行，执行者停下汇报，拆分（计划 v1.3.14）：选项对照表、档位表和"取值都在表里"归 T095，能力提取留在本任务，让 T055 只依赖本任务。
  - 合并 main 时 `driver/mod.rs` 有一处相邻行冲突（`claude` 与 main 的 `locate`、`probe`），都保留。T088 的会话核心和用例要补上 `DriverEvent::Ask` 的新字段 `tool_call_id`：编排者只做了让它编译的最小改动，核心里的匹配用 `..` 暂时忽略这个字段，行为与合并前一样（`item_id` 仍为空），填 `item_id` 交给 T095。
  - 合并后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-1d59394f960e.json`。执行者那次的 `pr-45f72a2a5d76.json` 一并保留。
- 核对：没有新依赖（开了 SDK 已有的 `unstable_end_turn_token_usage` 特性，让 `PromptDone` 带 token 用量）；`cupper-protocol` 的类型没改；没有个人信息，金样例约 1 MB 过了 `testdata-scan` 的通用扫描。
- 审阅：
  - **回放链路与运行时一致**：金样例经过分接层的 K19 改写和 SDK 的类型，与运行时走同一条路。GOLD-A-unknown-enums 用内联的小录制覆盖未知状态、选项 kind、stop reason 和更新类型。
  - **流式补全的工具字段**：Claude 的命令文本在后续的 `tool_call_update` 里才到，所以 `ToolUpdate` 带 `command`、`paths`，合并器结束时按 `missing_required(true)` 重新确定 `extraction`（开始是 `partial`，补全后结束时是 `full`）。
  - **保守的选项效果**：本任务按 kind 兜底，`allow_always` 一律持久、`privileged`，T095 换成按工具区分的表。
  - **`AskId`、`PromptTag` 由运行时分配**，提取层只交出 `(tool_call_id, AskRequest)` 和 `(stop_reason, usage)`：认可。
  - **金样例约 1 MB**，大部分是每次 `available_commands_update` 的完整命令列表；按 5.6.2，去重是运行时（T055）的事，提取层逐帧照出。认可。
  - `fork` 先给 `false`（P1）、`thinking` 给 `true`：认可。
  - `cargo xtask ci` 在环境里设置了 `UPDATE_GOLDEN` 时拒绝运行，守住 5.9.3。
- 规模：代码 +1,345 −28 行（不含金样例和 notes），在收窄后约 1,400 的上限以内。
- 转交事项已写进 `handoff-notes.md`：T095（`ask.rs` 的 `option()` 是换表的位置；重新生成受影响的金样例；`write_held` 填 `item_id`）、T055（`ClaudeSession`、编号由运行时分配、`CommandsChanged` 去重、`capabilities()` 的 `Versions`）。compaction 是否照推留给兼容剧本（T064、T066），已在交接里。
