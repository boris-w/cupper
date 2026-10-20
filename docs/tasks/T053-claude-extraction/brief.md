# T053 Claude 提取层、选项对照表与 A 层金样例

- 派发日期：2026-10-20
- 执行者：子 agent（工程师，驱动层与测试基础设施）。T052 的执行者接手，因为录制格式和 SP-1 录制的转换都是它做的
- 分支：`feat/T053-claude-extraction`
- 依赖：T040、T050、T052（都已合并）。T055（AgentRuntime 与 Claude 接入）依赖本任务；本任务离剩余关键路径有 2 个单位的松弛

## 背景

驱动层要把适配器发来的 ACP 帧逐帧变成宿主内部的 `DriverEvent`（设计 5.6.2、5.6.3）。这一步是纯函数，不知道回合和取消，所以可以用录制直接回放：每份录制提取出的 `DriverEvent` 序列规范化后与金样例比对，这就是 A 层金样例（5.9.3）。SP-1 的 48 份录制已由 T052 转成格式 v1，在 `testdata/recordings/` 下。

本任务只做 Claude 一侧；Codex 归 T110。

## 范围

**做**（计划 T053 一行；设计 5.6.2–5.6.5、5.3.6、5.9.3"两层金样例""规范化"）：
1. **Claude 提取层**：逐帧纯函数，ACP 帧 → `DriverEvent`（`driver/api.rs` 里现有的定义，缺的字段按下面的交接补上）。覆盖 5.6.3 对照表里 Claude 一列的全部更新类型；未知的更新只进原始帧，不产生事件。
2. **选项对照表**（5.6.4、5.3.6，Q7）：给每个权限选项标出 `effect` 和 `requires`；按工具区分 `allow-with-updates`。数据由驱动层维护，做成表。
3. **kind 兜底与未知 kind**：工具 kind 的兜底规则；未知的封闭枚举值按 K19 保留原值（交接，T050）。
4. **配置项与档位**（5.6.5）：`configOptions`、模式到档位的映射所需的数据。
5. **能力提取**（5.6.7 的输入）：从 `initialize`、`session/new` 等响应里提取能力，含 `steering.supported` 从 `initialize` 响应顶层的 `_meta` 读取。
6. **A 层金样例运行器**：读 `testdata/recordings/` 下 `usage` 含 `gold-a` 的 Claude 录制，经提取层得到 `DriverEvent` 序列，按 5.9.3 的规范化后与 `testdata/golden/` 下的金样例比对；`UPDATE_GOLDEN=1` 时更新，CI 里设置了它就判失败。接入 `xtask ci rust`（现有的 rust 作业里跑即可，不新增作业）。
7. **两项检查**：每份录制都有它 `usage` 里列出的金样例（本任务只管 Claude 的录制和 `gold-a`）；提取出的取值都在表里（选项对照表、kind 表等），出现不在表里的值就失败。

**不做**：
- Codex 的提取层与选项表（T110）；
- B 层金样例（T065）；
- `AgentRuntime`、拉起适配器（T055）。

## 验收标准

- [ ] GOLD-A-claude-s01、s02c、s03、s05、s06、s07、s07b、s09，以及其余 Claude 录制的 GOLD-A 编号（交接，T002：由本任务在 `tests/ids.toml` 的 T053 段里追加）。
- [ ] GOLD-A-unknown-enums、GOLD-A-tool-fields-claude、GOLD-A-option-map-claude。
- [ ] UNIT-option-map-claude、UNIT-capabilities、UNIT-capabilities-plan。
- [ ] 金样例一行一个事件，键排好序，方便评审；规范化规则（delta 折叠、占位等）在模块文档里写明。
- [ ] 测试名以用例编号开头；编号都在 `tests/ids.toml` 的 T053 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T053-claude-extraction/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T017：`ToolCall::missing_required(ended)` 不为空时，把 `extraction` 降为 `partial`。
- T050：封闭枚举的原值用 `tap::k19::original` 读取（`_meta.cupper.original.<字段名>`），覆盖 `stopReason`、选项 `kind` 和工具调用的 `status`。`initialize` 只声明了 elicitation、`terminal_output`、`plan`，比 SP-1 的客户端少了 `session.{configOptions.boolean, notices, compaction}` 和 `auth.terminal`；不声明 `session.compaction` 时适配器还推不推 compaction，用录制核对，必要时提出补设计 5.6.1。
- T042：按 `AskRequest` 交出 agent 的请求：权限请求用 `Classified`，form 模式的 elicitation 用 `Form`（原样带 JSON schema）。**`DriverEvent::Ask` 还没有 `toolCallId`，由提取层补上**（actor 填 `item_id` 要用）。
- T041：`NormUpdate::ToolStart` 要给出 `cwd`，否则合并器按 `missing_required` 把 `execute` 类工具定为 `partial`。
- T052：录制格式库的接口（`Recording::parse`、`segments()`、`Body::event()`）。
- T002：其余录制的 GOLD-A 编号由本任务追加。

## 注意

- **SP-1 的录制来自 SP-1 自己的客户端**（`clientInfo`、配置项请求序列、故意的并发 prompt 都与宿主不同），只用于 A 层（5.9.2）；A 层是逐帧纯函数，不受这些差异影响。
- **不运行真实的 Claude Code CLI，不登录，不联网**。
- 代码约束：提取层是纯函数，不做 I/O；协议里的整数不超过 2^53−1；导出给其他语言的产物不依赖 serde_json 的 map 键顺序。
- 金样例和测试里不能出现个人信息；金样例由录制生成，录制已经脱敏，生成后仍过一遍 `testdata-scan`。
- 改了 `cupper-protocol` 的类型就跑 `pnpm gen`，生成物随提交。

## 规模

计划估为 L，上限约 1,500 行。金样例是数据，不计入规模；运行器和检查计入。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**；计划 1.1 的执行经验是实现按草估的 1.7 倍计。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出上限时，开工前就停下来汇报，并给出拆分建议**，例如：
- 第一部分：提取层、A 层运行器与两项检查（T055 主要依赖它）；
- 第二部分：选项对照表、配置项与能力提取。

## 参考资料

- 设计：`06-agent-driver.md` 5.6.1–5.6.5、5.6.7、5.6.9（K 清单）；`03-protocol-v1.md` 5.3.6；`09-testing.md` 5.9.2、5.9.3；`01-repo-build-ci.md` 5.1.2（`testdata/golden/`）。
- `docs/tasks/SP-1-adapter-behavior/`（场景说明）；`docs/tasks/T050-driver-transport/`、`T041-autonomous-coalescer/`、`T042-interaction-broker/`、`T052-recording-replay/`。
- `crates/cupper-host/src/driver/`（`api.rs`、`acp.rs`、`tap/`）；`crates/cupper-protocol/src/recording.rs`。

## 工作方式

在原来的 T052 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T053-claude-extraction main`。T052 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 1,690 行（按 T052 的偏差可能到 1,800–1,900），在写代码之前停下汇报，并建议拆成两部分。

**裁定：拆分（计划 v1.3.14），但能力提取留在本任务。** T055 的能力缓存要用能力提取，留在这里，T055 就只依赖本任务；本任务在剩余关键路径上，拆分是为了缩短它的实际工期。
- **T053 收窄为**：`driver/api.rs` 的改动（`Ask` 的 `tool_call_id`、`ToolStart` 的 `cwd`、`ToolUpdate` 的命令和路径、金样例用的 `Serialize`）、合并器（应用新字段，结束时按 `missing_required(true)` 降为 `partial`）、Claude 提取层、交互请求的分类与构造（选项先按 5.3.6 的"按 kind 兜底"和"kind 未知"两行，结果保守）、配置项按 category 透传、**能力提取**、A 层运行器与"每份录制都有金样例"、xtask 的 `UPDATE_GOLDEN` 检查。验收 GOLD-A-claude-*（含其余 Claude 录制，由你追加编号）、GOLD-A-tool-fields-claude、GOLD-A-unknown-enums、UNIT-capabilities、UNIT-capabilities-plan。上限约 1,400 行，硬停止线 1,500。
- **新增 T095**（依赖 T053）：按工具区分的选项对照表与前缀风险（Q7）、档位表、"取值都在表里"的检查（GOLD-A-option-map-claude、UNIT-option-map-claude），换下按 kind 兜底并重新生成 s04、s04b、s05 等受影响的金样例。T044 改为也依赖 T095。`tests/ids.toml` 已在 main 上改好。

执行者核对出的几点（SDK 能解析全部 11 种更新、命令参数流式补全、工具调用不带 cwd 要取会话工作目录、合并器要多记必填字段的状态）都按其估计纳入。compaction 照常处理，宿主不声明 `session.compaction` 时适配器是否照推，留给兼容剧本（T064、T066）核对。

**交接（T088，已合并）**：`DriverEvent::Ask` 带上 `toolCallId` 之后，在 `SessionCore::write_held` 里填上 `item_id`。这一步属于本任务对 `api.rs` 的改动，请一并做。
