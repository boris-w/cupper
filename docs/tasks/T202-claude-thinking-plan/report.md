# T202 汇报

- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T202-claude-thinking-plan`（PR #17：https://github.com/boris-w/cupper/pull/17）
- 完成日期：2026-10-28

**状态：** 全部完成。编排者已经把 main（含 T203）合进本分支，合并提交是 `22b7090`，并连同 `352f73c` 一起推送了。PR #17 的 8 个检查由编排者看，交回时还不知道这一次推送的结果。

## 做了什么

1. **请求思考显示**（`1e3e15c`）
   - `session/new` 和 `session/resume` 都带同一个 `_meta.claudeCode.options.extraArgs = {"thinking-display": "summarized"}`。只请求显示方式，不动思考的开关和档位；这是编排者选的 B（5.6.5）。
   - 不信任的项目另带 `settingSources: ["user"]`。
   - 新增 UNIT-claude-session-thinking，同步改了 IT-trust-settingsources。
2. **计划拒绝结束回合**（`6caa4f3`）
   - Claude 计划确认的 `reject` 改成 `ends_turn: true`，其余字段不变。
   - 新增 UNIT-claude-plan-reject-ends-turn。
   - GOLD-A `s05-questions` 只有这一个字段变化。
   - 界面已经会显示"拒绝将结束本回合"，不用改。
3. **两次真实运行**
   - 只跑两个剧本的那次：plan 通过，回合里有 14 条 thought；thinking 失败，因为 17×23 太简单，模型没有思考。只记进 notes。
   - 换题（`2893c4e`）：thinking 剧本改成一道 5 人排名次的逻辑题，3 个约束，唯一解，穷举核对过。
   - 整套 `--real --write`（`5530cdc`）一次通过：19 个通过、2 个观察、steer 不适用，配置没有变化。
     - thinking 有 38 条 thought，答案正确；plan 14 条；haiku 的 6 个剧本也有 thought。
     - 新报告取代 T066 的那份。20 份录制和 6 份金样例跟着重新生成。
     - 已验证范围重新生成后没有变化（2.1.200–2.1.296）。
     - COMPAT-claude-thinking 变为 passed。
4. **文档**
   - T202 的 `notes.md`（`c556a59`、`290b9c2`、`a0667bf`）：源码位置、取舍、修前修后对照、两次运行的记录、`rec-shape-diff` 的差异、扫描结果。
   - T066 的 `man-compat-claude.md`（`352f73c`）：末尾加了一节"报告已被取代"，不新增 `[[evidence]]`。

## 关键取舍

- **思考配置用 `extraArgs` 而不用 SDK 的 `thinking` 字段**：后者的 `type` 必填，会把思考强制打开。依据是 claude-agent-acp 0.91.0 和 SDK 0.3.300 的源码。2.1.200–2.1.296 都认 `--thinking-display`，依据是 SDK 0.2.100、0.3.142、0.3.200 都会发这个参数。
- **new 和 resume 带同一个值**：适配器在 resume 时按 options 比对，值不同会重建进程。
- **只跑两个剧本的那次不写入**：`xtask compat` 拒绝 `--real --write --only`。按编排者补充的范围，改为换题后整套重跑。

## 验证方式

| 验收标准 | 结果 |
|---|---|
| 两个 UNIT 用例通过，有修前修后对照 | 通过。修前：meta 是 `Null`；`ends_turn` 是 `Some(false)`。修后都通过 |
| 真实 CLI 上 thinking 出现 thought、plan 通过，报告入库，过敏感词扫描 | 通过。27 个文件 `sanitize words`、`scan` 都报 clean，编排者抽查通过 |
| 本机单份跑与改动相关的测试 | 完成：driver 77 个、GOLD-A、GOLD-B 12 个、interaction_trust 7 个、compat 单元测试、clippy、fmt |
| PR 上 8 个检查都通过 | 由编排者在 PR #17 上看，交回时没有结果 |

额度：累计约 61 个回合、约 0.66 美元，在用户许可的 150 个回合、1.5 美元以内。

规模：代码 +101 −12，停止线 300。

## 提交列表

```
22b7090 Merge branch 'main' into fix/T202-claude-thinking-plan（编排者合并）
352f73c docs(T066): its compat report is replaced by T202's
a0667bf docs(T202): notes on the new thinking prompt and the full real run
5530cdc test(compat): the real Claude suite again, thoughts and all
2893c4e test(compat): a puzzle for the thinking scenario
290b9c2 docs(T202): notes on the real run of the thinking and plan scenarios
b23f06f Merge branch 'main' into fix/T202-claude-thinking-plan（编排者合并）
c556a59 docs(T202): notes on the thinking display and the plan reject
6caa4f3 fix(host): rejecting Claude's plan ends the turn
1e3e15c fix(host): ask Claude for its thinking as summaries
```

## 遗留问题 / 范围外发现

1. `xtask compat --out` 要写绝对路径：执行者在 `crates/cupper-host` 下运行，相对路径找不到计划文件。没有改。
2. 未核实：Bedrock、Vertex 上 `summarized` 是否生效。
3. 已记进界面任务的交接事项：`request.spec.ts` 的计划确认样例里 `plan_revise` 没有带 `ends_turn`。
4. 编排者抽查时发现 SP-1 来的 s0x 录制里有残留，归 T204 处理。
5. 遗留清单写在主仓库的 `.private/cleanup/T202-leftovers.md`，共 28 个 CLI 会话，由用户决定是否清理。本机的运行输出（worktree 的 `target/compat*`）可以随 `cargo clean` 一起删。
