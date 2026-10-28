# T202 Claude 的思考流与计划拒绝

- 派发日期：2026-10-28
- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T202-claude-thinking-plan`
- 依赖：T066 的剧本修改合并之后再请求真实运行（见范围第 3 条）。T099 依赖本任务

## 背景

T066 用真实的 Claude Code 2.1.296 冒烟时，发现两处与宿主的假设不符。编排者已按实测改了设计 5.3.6 和 5.6.3：

1. **思考流**：sonnet 的回合里一条 `agent_thought_chunk` 都没有。
   - 适配器 claude-agent-acp 0.91.0 的源码里写着 "Recent models default `thinking.display` to "omitted""：思考块只有签名、没有文本，适配器就不发 thought。
   - 客户端可以经 `session/new` 的 `_meta.claudeCode.options.thinking` 传思考配置，宿主没有传。
   - 结果是 cupper 永远显示不出 Claude 的思考（MS-2，COMPAT-claude-thinking）。SP-1 的 s02d-thinking 录制里也没有 thought，说明这个问题以前就在。
2. **计划拒绝**：计划确认里选 `reject` 之后，真实的回合立刻以 `cancelled` 结束，工具调用标 `nonExecutionKind: user-rejected`。设计 5.3.6 原先写的是"ends_turn 否、继续规划"，宿主的选项映射也是按这一条写的。

## 范围

**做**：
1. **请求思考显示**：
   - 宿主在 `session/new` 和 `session/resume`（以及 `session/load`，如果走得到）里，经 `_meta.claudeCode.options.thinking` 请求带摘要的显示；
   - 字段名和取值按锁定的 claude-agent-acp 0.91.0 和它依赖的 SDK 核实，写进 `notes.md`，附源码位置；
   - 有没有哪些模型或档位不该带这个配置，也一并核实；
   - 新增 UNIT-claude-session-thinking：宿主发出的 `session/new`、`session/resume` 带这项配置；
   - 提取层、`capabilities.rs` 里与思考有关的声明，按设计 5.6.3 核对，需要的话一并改。
2. **计划拒绝结束回合**：
   - Claude 计划确认的 `reject` 映射改为 `ends_turn: true`，其他字段不变，照设计 5.3.6；
   - 新增 UNIT-claude-plan-reject-ends-turn；
   - 界面已经按 `ends_turn` 显示"拒绝将结束本回合"（Codex 的 `cancel` 用的是同一条路），核对 Claude 的计划确认也走到这条路；
   - 受影响的金样例、录制回放照实更新，逐条说明。
3. **真实验证**（COMPAT-claude-thinking）：
   - 代码改完、本机测试通过后，用 SendMessage 发给 `main`，请用户许可；
   - 用户同意后，用真实 CLI 只跑 `thinking`、`plan` 两个剧本（sonnet，约 2 个回合），报告入库；
   - 要等 T066 改过的剧本已经在 main 上，才能跑。T066 还没合并时，先停下告诉 `main`；
   - 真实运行的约束照 T066 的任务书，包括敏感词扫描和遗留清单。
4. 契约已冻结：`ends_turn` 只是取值变化，不改类型。要改协议类型时，先汇报。

**不做**：兼容套件的剧本和判定（T066 改）；Codex 的对照表；界面文案。

## 验收标准

- [ ] UNIT-claude-session-thinking、UNIT-claude-plan-reject-ends-turn 通过，有修前修后对照。
- [ ] 真实 CLI 上 `thinking` 剧本出现 `agent_thought_chunk`，`plan` 剧本通过；报告入库，入库前过敏感词扫描。
- [ ] 本机单份跑与改动相关的测试，不循环、不并行重跑（守则第 10 条）。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：250；硬停止线：320

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.3.6（选项对照表）；5.6.3（提取层对照，"思考""计划"两行）；5.6.4；5.9.5；5.12 的 MS-2。
- 卷宗：T066 的 `notes.md` 和汇报（现象与证据）；T053（Claude 提取层）；T042（交互请求）。
- `docs/03-plan.md`：T202 一行。

## 注意

- 第 3 条之前，不运行真实的 Claude Code、Codex CLI。
- 不读、不改用户的 `~/.claude`、`~/.codex`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
