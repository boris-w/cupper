# T243 汇报

- 执行者：子 agent（Rust 工程师，宿主驱动）
- 分支：`feat/T243-degrade-plan-thinking`
- 完成日期：2026-11-04
- PR：https://github.com/boris-w/cupper/pull/83
- CI 运行：https://github.com/boris-w/cupper/actions/runs/41810387124 。`f11d3430` 上 8 个检查全部通过。
- test-repeat（`--features test-harness --test driver_k`，filter `it_degrade_`，10 次 × 4 份，macos-15）：https://github.com/boris-w/cupper/actions/runs/41810386785 。日志里的汇总是 `40 passed, 0 failed`，通过数 40，与预期一致。

## 做了什么

1. **能力跟着选项走（方案二）**
   - 新增 `driver::extract::offers(options, id, value)`：判断选项里有没有 `id`；指定了 `value` 时，还要求它在可选值里。agent 还没报告任何选项时一律返回 true。
   - Claude：`plan_confirm` 看 `permission_mode` 是否有 `plan` 值，`thinking` 看是否有 `reasoning_effort`。
   - Codex：`plan_confirm` 看是否有 `plan_mode`，`thinking` 看是否有 `reasoning_effort`。
   - `extraction.plan` 没有"无"这个取值，不动。
   - 协议 `capabilities.rs` 里这两个字段的文档注释写明：只用来决定入口；计划确认请求和思考条目，不论字段取什么值都照样显示。
2. **假 agent 的 `--without plan|thinking`**
   - `config.rs`：独立的 `--without` 分支、字段和 `without()` 方法。USAGE 只改第 2 行。
   - `persona::config_options(&Config)`：
     - `plan`：去掉 Claude `mode` 里的 `plan` 值和 Codex 的 `collaboration_mode` 选项。旧的 `modes` 镜像随之变化。
     - `thinking`：去掉 `thought_level` 类别的选项。
   - `agent.rs`：只改了 `open()` 里的调用。
   - `script.rs`：`exec` 开头加判断，跳过 `plan_update`、`plan_confirm`、`think`。
   - persona 的单元测试改为用 `Config` 调用。
3. **`set_config`**：照你的决定保留现状。值不在时回 `invalid_params`；选项整个不在时回 `unsupported`，带 `data.capability`。agent 都收不到。
4. **用例**：在 `tests/driver_k.rs`，紧接已有的 thought、plan 用例，没有放在文件末尾。共用一个 `degraded(agent, what)`，四条：
   - `it_degrade_plan_claude`
   - `it_degrade_plan_codex`
   - `it_degrade_thinking_claude`
   - `it_degrade_thinking_codex`

## 关键取舍

- **取方案二**：AG-4 要求按能力隐藏入口，能力本身就得反映出来。推断只来自"有没有这个选项"，所以用文档约束把它限定在入口用途。
- **推断出错的实例，以及怎么处理的**
  - 第一版照方案二直接推断，37 条 gold_a 用例失败了。
  - 原因：适配器 `initialize` 之后、`session/new` 之前，宿主会按空的选项列表先算一次能力，两个字段于是都成了 false。
  - 这正是推断出错的现实例子：当时只是还不知道有哪些选项，并不是没有。
  - 改为：没有报告任何选项时视为可用；报告了选项、却缺少对应的那一项时才为 false。gold 文件不用改；Claude 的单元用例加了一条断言覆盖这种情形。
- **推断错了的其他后果**
  - 假阴性：例如旧版适配器只在旧的 `modes` 里给 `plan`，或者模型思考强度固定、没有 effort 选项却仍输出思考。现在没人读这两个字段，所以没有影响。
  - 危险用法：界面按这两个字段藏掉已经收到的计划确认请求。请求没人能答，回合会卡到交互超时。文档约束就是为了排除这种用法。
  - 假阳性：与原来的常量 true 一样，没有新增风险。
- **避开 T140 的写法**：开关、过滤、跳过分别放在不同位置，用例插在文件中段。不过 T140（PR #82）也改了 `driver_k.rs`，两条分支合并时仍有冲突，按你的安排由后合并的一方 merge main 解决。

## 验证方式

本机（单份运行）：
- `cargo test --workspace --all-features`：932 个通过，0 失败。
- clippy（带和不带 `--all-features` 两种，`-D warnings`）和 `cargo fmt --all --check`。
- `cargo xtask ci protocol`：通过。只改了文档注释，schema 里只有描述变化，0 breaking。
- `cargo xtask ci testdata-scan`：通过，含 trace-ids。

对照验收标准：

1. **IT-degrade-plan、IT-degrade-thinking 通过，并在 runner 上连跑 10 次**：是，40 次全部通过，链接见开头。每条用例断言：
   - 能力里没有这个选项。Claude 的计划例外：`permission_mode` 还在，但没有 `plan` 值。
   - 关掉的那个字段为 false，另一个仍为 true。
   - `session/set_config` 被拒：
     - Claude 的计划：`invalid_params`，没有 data。
     - 其余三种：`unsupported`，`data.capability` 为该选项。
   - 假 agent 的 `--calls` 里没有 `session/set_config_option`。
   - 回合正常结束（说出了 `done`）；会话流里没有 `plan`、`thought` 条目，也没有 `interaction.requested`。

   单元用例：两个 agent 的 s01 录制，在选项齐全时两个字段为 true，去掉对应选项后为 false；没有任何选项时仍为 true。

2. **不关这两项时行为不变**：已有的用例照旧通过，包括 `it_thought_claude_streams_a_thought_item`、`it_plan_entries_*`、全部 gold_a，以及 persona 与 SP-1 录制的比对。

3. **`schema-compat` 0 breaking；trace-ids 检查通过**：是。

4. **8 个检查都通过**：是，链接见开头。

## 设计偏差 / 补充（请合并时写进设计）

1. **5.3.7 能力描述**：
   - `interactions.plan_confirm` 和 `extraction.thinking` 由 agent 给出的选项推断。
     - Claude：`permission_mode` 的 `plan` 值。
     - Codex：`plan_mode` 选项。
     - 思考：`reasoning_effort` 选项。
   - 没有报告任何选项时（`initialize` 之后），视为可用。
   - 这两个字段只用来决定入口（选择器、开关）；宿主照旧提取所有计划确认和思考块。
2. **5.7 界面（请求面板、思考条目的显示，以及新建会话页和模式选择器里按能力隐藏入口的地方）**：收到计划确认请求或思考条目时照样显示，不按这两个字段隐藏，否则请求没人答，回合会卡到交互超时。
3. **5.6.5 和任务书**：选项整个不在时，`session/set_config` 回 `unsupported` 并带 `data.capability`；只有值不在时才回 `invalid_params`。任务书写的是一律回 `invalid_params`，与 T141 的实现不一致，以 T141 的实现为准。
4. **5.9.2 假 agent**：新增 `--without plan|thinking`。

## 提交列表

```
f11d3430 feat(host): plan and thinking capabilities follow the agent's options
```

## 遗留问题 / 范围外发现

- **与 T140（PR #82）在 `driver_k.rs` 上的冲突**：等你通知后，由后合并的一方 merge main 解决。其余文件 git 能自动合并。
- **这两个字段现在没有读者**：界面（T242 以后）用它们隐藏入口时，要遵守设计第 2 条的约束。
- **规模**：+176 −21，在停止线 180 以内。比估计的约 110 多出的部分主要在 IT：两种错误要分开断言，还要查数据库。

worktree 请保留，以备 merge main 解决冲突。
