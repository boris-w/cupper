# T053 汇报

- 执行者：子 agent（工程师，驱动层与测试基础设施）
- 分支：`feat/T053-claude-extraction`，在原 T052 的 worktree 里从 main 开出
- 完成日期：2026-10-20

## 做了什么

裁定后收窄的 T053 范围已全部完成，只有 T088 的 `write_held` 一项没做：分支上没有那段代码，见遗留问题第 1 条。在干净的 worktree 里跑 `cargo xtask ci pr --record` 通过，摘要已提交。代码共 +1,345 −28 行，在 1,400 行的上限以内；这个数不含金样例和 notes。

1. **`driver/api.rs`**：
   - `DriverEvent::Ask` 加了 `tool_call_id`（T042 交接）；
   - `NormUpdate::ToolStart` 加了 `cwd`（T041 交接）；
   - `ToolUpdate` 加了 `command` 和 `paths`。原因是 Claude 的命令参数在后续的 `tool_call_update` 里才补全；
   - 驱动事件加了 `Serialize`，给金样例用。`AdapterExited` 不序列化。
2. **合并器**：
   - 应用上面的新字段；
   - 每个工具调用记下已拿到的必填字段，结束时按 `missing_required(true)` 重新确定 `extraction`（T017 交接）。所以 Claude 的命令开始时是 `partial`，补全之后结束时会改成 `full`；
   - 新增用例 `coalescer_tool_extraction_follows_the_fields_a_call_ends_with`。
3. **Claude 提取层 `driver/claude/`**：
   - `ClaudeSession` 每个会话一份，只记三样东西：工作目录、最近一次 Markdown 计划、当前配置项。
   - 录制里出现的 11 种会话更新都有映射。SDK 解析不了的更新不会到这里，只留在原始帧里。
   - 工具字段：命令文本、`execute` 类工具的工作目录、相对路径、diff 及行数统计、去掉 "Exit code N" 行的输出、`_meta.terminal_exit` 里的退出码。
   - prompt 结果：开了 SDK 的 `unstable_end_turn_token_usage` 特性，`PromptDone` 才能带上 token 用量。
   - 错误分类：`not_logged_in`、`process_exited`、`session_not_found`、`other`。
   - K19：未知的 `stopReason` 原样下发；未知的工具状态不改变状态；未知的选项 kind 原样下发。
4. **交互请求（`ask.rs`）**：
   - ExitPlanMode 发来的请求（`switch_mode` 加 `exit-plan-` 选项）归为 `plan_confirm`，计划正文取 `rawInput.plan`，取不到时用最近一次 `plan_update`；其余归为 `permission`；form 模式的 elicitation 原样交出 schema。
   - 选项只按 5.3.6 的"按 kind 兜底"和"kind 未知"两行定效果，是保守的：`allow_always` 一律算持久、`privileged`；kind 未知时 scope 为 `unknown`、算持久、`privileged`。
5. **配置项与能力（`capabilities.rs`）**：
   - 配置项按 category 映射，在档位表（T095）之前，`permission_mode` 的每个取值都标 `full_access`。
   - 能力由 `initialize` 加会话的配置项和命令组成。`steering.supported` 从 `initialize` 响应顶层的 `_meta` 读；`plan` 是 `markdown_only`。
   - 处理了 `current_mode_update` 和 `set_config_option` 的回复。
6. **A 层运行器（`gold_a.rs`）**：
   - 每份录制依次经过分接层的 K19 改写、SDK 解析、提取层，按 5.9.3 规范化后与 `testdata/golden/gold-a/` 下同路径的金样例比对；`UPDATE_GOLDEN=1` 时改为写入。
   - 规范化：同一消息的文本折叠成一条，对象的键显式排序。工作区里的 serde_json 开了 `preserve_order`，所以不能依赖默认顺序。
   - 18 份 Claude 录制各有一个用例。另有一项检查：每份录制都有用例和金样例，每个金样例都有对应的录制。
   - 规范化规则写在模块文档里。
7. **xtask**：`cargo xtask ci` 发现环境里设置了 `UPDATE_GOLDEN` 就拒绝运行。
8. **编号**：其余 10 份 Claude 录制的 GOLD-A 编号已追加到 `tests/ids.toml` 的 T053 段：s02、s02b、s02d、s04、s04b、s07x、s08、s10、s11、s12。

## 关键取舍

- **分接层的改写也在回放链路里**：金样例经过 `k19::rewrite`（改为 `pub(crate)`）和 SDK 的类型，与运行时走同一条路。GOLD-A-unknown-enums 用一份内联的小录制，覆盖未知状态、未知选项 kind、未知 stop reason 和未知更新类型。
- **`AskId` 和 `PromptTag` 不在提取层分配**：提取层交出 `(tool_call_id, AskRequest)` 和 `(stop_reason, usage)`，由运行时分配编号；运行器按顺序从 1 开始编。
- **`fork` 先给 `false`**：读它需要 SDK 的另一个 unstable 特性，而 fork 是 P1。
- **`thinking` 给 `true`**：适配器支持思考流，只是录制里 haiku 没有产生。
- **金样例约 1 MB**：大部分是每次 `available_commands_update` 的完整命令列表。按设计 5.6.2，去重是运行时的事，提取层逐帧照出。

## 验证方式

- 在干净的 worktree 里于 `45f72a2` 跑 `cargo xtask ci pr --record docs/tasks/T053-claude-extraction/ci/`，全部通过。摘要是 `pr-45f72a2a5d76.json`，提交为 `57b1043`。

  | 作业 | 结果 |
  |---|---|
  | rust | 1,302 通过 / 0 失败 |
  | protocol | 185 / 0 |
  | ts | 323 / 0 |
  | license | 1 / 0 |
  | testdata-scan | 3 / 0，本机没有敏感词清单，`sensitive-words` 跳过 |
  | ui、e2e | 还没实现 |

- 对照验收标准：
  - GOLD-A-claude-s01 至 s12（共 18 个）、GOLD-A-tool-fields-claude、GOLD-A-unknown-enums、UNIT-capabilities、UNIT-capabilities-plan 在摘要里都是 pass。✅
  - 金样例一行一个事件，键已排序，规范化规则写在模块文档里。✅
  - 测试名都以编号开头，编号都登记在 T053 段。✅
  - 金样例和摘要都过了 `testdata-scan` 的通用扫描，没有命中。✅
- `cupper-protocol` 的类型没改，不需要跑 `pnpm gen`。

## 提交列表

```
57b1043 docs(task): record the T053 pr CI summary
45f72a2 docs(task): record the T053 line counts and choices
1edb0db feat(xtask): refuse to run CI with UPDATE_GOLDEN set
0c1a574 test(host): add the A-layer goldens for Claude's recordings
a3b9646 feat(host): extract Claude's frames into driver events
1f05946 feat(host): carry streamed tool fields and the ask's tool call
485a96d docs(task): record the T053 checks and size estimate
```

## 遗留问题 / 范围外发现

1. **T088 的交接没做**：`SessionCore::write_held` 填 `item_id` 这一步，在这个分支上做不了。分支开出时 T088 的代码还不在 main 上，至少我看到的 main（最新是 `12d1bb9`）不包含它的提交，分支上也就没有 `write_held`。`DriverEvent::Ask` 已经带上 `tool_call_id`，合并 main 之后再接：由你合并时处理，或者交给 T095。合并时还要留意，T088 如果解构了 `DriverEvent::Ask`，需要补上新字段。
2. **给 T095**：`ask.rs` 的 `option()` 是换成按工具区分的对照表的位置。换表后 s04、s04b、s05 等的金样例要用 `UPDATE_GOLDEN=1` 重新生成并评审。
3. **给 T055**：
   - 运行时要做的事：为每个会话建 `ClaudeSession::new(cwd, config)`；分配 `AskId` 和 `PromptTag`；`CommandsChanged` 按设计 5.6.2 去重。
   - `capabilities()` 要从 CLI 定位那边拿到 `Versions`。
4. **compaction 是否照推仍未确认**：提取层照常处理 `compaction_update`。宿主不声明 `session.compaction` 时适配器还推不推，留给兼容剧本（T064、T066）。
5. worktree 已保留。