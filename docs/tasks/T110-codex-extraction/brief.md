# T110 Codex 提取层、选项对照表与 A 层金样例

- 派发日期：2026-10-30
- 执行者：子 agent（Rust 工程师）
- 分支：`feat/T110-codex-extraction`
- 依赖：T053、T102（都已合并；可以提前，计划 1.7）。T111 依赖本任务

## 背景

Claude 的提取层（T053）、选项对照表与档位表（T095）已经做完。本任务给 Codex 做同样的事：把 codex-acp 2.2.2 发来的帧，逐帧变成宿主的 `DriverEvent`，交互请求分类、选项映射到生效范围、模式映射到档位，并用 SP-1 的 Codex 录制做 A 层金样例。T102 已经核实了 Q8、Q25、Q31，结论在它的汇报里。本任务不接入运行时，Codex 的发现、运行时和回收归 T111。

## 范围

**做**（计划 T110 一行；设计 5.6.3–5.6.5、5.3.6）：
1. **Codex 提取层**（逐帧纯函数，照 T053 的结构）：
   - `session_info_update._meta.codex.threadStatus`（`active` 带 `waitingOnApproval`、`waitingOnUserInput`，`idle`，`systemError`）；
   - `_meta.codex.error` 和文本形式的错误；
   - 读取类命令的输出在 `content` 文本里；
   - 命令输出 `_meta.terminal_output.data` 逐行增量；
   - diff 每个 hunk 一个、带 `_meta.kind`，模型用 shell 运行 `apply_patch` 时没有 diff；
   - 工具状态一出现就是 `in_progress`；
   - 思考 `agent_thought_chunk`；
   - 计划协作模式下的 `plan_update`；
   - 标题。
   - 以上都照设计 5.6.3 的对照表。
2. **Q31**：按 T102 的结论，cupper 现在不声明 JetBrains AIR，`phase` 拿不到，最终回答按宿主规则（5.3.5）判定。要不要为它声明 AIR，由本任务评估，结论写进 `notes.md`。评估要考虑三个副作用：命令列表会带出 `skillPath`（本机路径），模式选项会带 `kind`，工具调用的渲染方式会变。默认不声明；要声明的话先汇报。
3. **选项对照表与档位**：
   - 照 5.3.6 的 Codex 各行：`allow_once`、`allow_for_session`、`accept_execpolicy_amendment`（持久，写入执行策略）、`cancel`（结束回合）、`implement_plan`、`revise_plan`（结束回合，T102 实测）；
   - 4 个模式取值映射到 5.6.5 的档位：`read-only`、`workspace-write` 为 normal，`agent` 为 elevated，`agent-full-access` 为 full_access；不认识的取值一律按 full_access；
   - "录制里出现的选项和模式取值都在表里"的检查，照 T095。
4. **A 层金样例**：SP-1 的 Codex 录制（`testdata/recordings/codex-acp/2.2.2/0.159.2/`）每一份都有金样例，编号照 `tests/ids.toml` 的 T110 段；其余录制的 GOLD-A 编号由你追加。另有 GOLD-A-tool-fields-codex、GOLD-A-option-map-codex、UNIT-option-map-codex。
5. **交接事项**（`handoff-notes.md` 第一列含 T110 的行）逐条处理，结论写进汇报：
   - T042：`isOther` 和 `<id>_note`；
   - T058：裁判与 threadStatus 的 idle；
   - T058 修复：空闲时的 thread active 要不要挡住出队；
   - T055：第二种 agent 要一个自己的 `ActorKind`；
   - T002：其余录制的编号；
   - T102：Q31、Q25。

**不做**：Codex 的发现、运行时复制、`CODEX_HOME`、回收（T111）；Codex 的真实运行和兼容矩阵（T113）；界面。

## 验收标准

- [ ] 计划 T110 一行列出的编号都通过，登记在 `tests/ids.toml`。
- [ ] SP-1 的每份 Codex 录制都有 A 层金样例；"取值都在表里"的检查覆盖 Codex。
- [ ] Q31 的评估和交接事项逐条有结论。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：1000；硬停止线：1200

<!-- 金样例不计入。执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：
  - 5.6.3（提取层：Claude 与 Codex 对照）；
  - 5.6.4（交互请求的分类与选项映射）；
  - 5.6.5（模式与档位）；
  - 5.3.5（最终回答的判定）、5.3.6（选项对照表）；
  - 5.9.3（A 层）；
  - 5.13 的 Q25、Q31。
- 卷宗：
  - T053、T095（Claude 的同类实现，照它的结构）；
  - T102（Q8、Q25、Q31 的结论，codex-acp 2.2.2 的源码阅读）；
  - SP-1（Codex 录制的场景说明）；
  - T042、T058。
- `docs/03-plan.md`：T110 一行和注。

## 注意

- 不运行真实的 Claude Code、Codex CLI；不读、不改 `~/.claude`、`~/.codex`。需要看 codex-acp 的源码时，从 npm 下载锁定的 2.2.2 到临时目录读，不入库。
- 本机只单份跑与改动相关的测试（守则第 10 条）。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
