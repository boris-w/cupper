# T243 能力降级：计划与思考

- 派发日期：（待派发）
- 执行者：子 agent（Rust 工程师，宿主驱动）
- 分支：`feat/T243-degrade-plan-thinking`
- 依赖：T141（已合并）。T179 依赖本任务

## 背景

T140 开工前估约 760 行，把与 Steer 无关的两条降级用例拆到本任务（计划 v1.3.88）。

现状：`interactions.plan_confirm` 和思考的提取，都是按 agent 种类写死的。agent 能让宿主知道"不可用"的，只有它给出的选项：

- Claude：`mode` 选项里的 `plan` 值；
- Codex：`collaboration_mode` 选项；
- 两种 agent：`thought_level` 选项。

## 范围

**做**（计划 T243 一行；设计 5.6.5、5.6.7、NFR-7、AG-4）：

1. **假 agent**：加 `--without plan|thinking`。分别去掉对应的选项和值，也不再发计划块和思考块。写法照现有的命令行开关。
2. **宿主降级**：
   - 能力和选项里没有这个选项或值；
   - `set_config` 设它回 `invalid_params`，agent 收不到；
   - 回合里没有计划条目或思考条目。
3. **`plan_confirm` 和思考的提取是否跟着选项走**：开工前在 `notes.md` 里写出方案，告诉 `main` 之后再动手。
   - T140 的执行者倾向"方案二"：让 `interactions.plan_confirm` 和 `extraction.thinking` 跟着 agent 给出的选项走，约 20 行；`extraction.plan` 没有"无"这个值，不加。
   - 这个方案是按"有没有这个选项"推断出来的，不是 agent 明说的。推断错了会有什么后果，要在方案里写清楚。
4. **用例**（都已登记）：
   - **IT-degrade-plan**：假 agent 关掉计划时，Claude、Codex 两种形态各一条。
   - **IT-degrade-thinking**：假 agent 关掉思考时，两种形态各一条。
5. 协议照 ADR-04，契约已冻结，只能附加，`schema-compat` 0 breaking。

**不做**：界面上的隐藏。界面按能力和选项渲染，选择器由 T231 完成。

## 验收标准

- [ ] IT-degrade-plan、IT-degrade-thinking 通过，并在 runner 上连跑 10 次（由 `main` 触发 test-repeat）。
- [ ] 不关这两项时，行为不变。
- [ ] `schema-compat` 0 breaking；trace-ids 检查通过。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：180；硬停止线：220（T140 拆分时估 120–140）

## 参考资料

- 设计：5.6.5（配置选项）、5.6.7（能力缓存）、5.6.9；PRD AG-4；NFR-7。
- 卷宗：T141（`session/set_config`）、T231（选择器）、T140（`notes.md` 里的两种方案）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 子进程一律经 `cupper-sys::spawn`；时间经 `Clock`；每条事件流只有一个写者。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
