# T146 汇报

- 执行者：子 agent（前端工程师，ui-core 与桌面）
- 分支：`feat/T146-stream-fold`
- 完成日期：2026-11-04
- PR：https://github.com/boris-w/cupper/pull/76（head f4b2603）
- CI：https://github.com/boris-w/cupper/actions/runs/41793643061 ，8 个检查全部通过（ui、rust、compat-framework、e2e 在 macos-15 上；protocol、ts、license、testdata-scan 在 ubuntu-24.04 上）
- ui-baseline：https://github.com/boris-w/cupper/actions/runs/41791295739（在 b588c87 上跑）

## 做了什么

1. **回合折叠与用时**（`rows/flatten.ts`、`rows/basic.tsx`）：
   - **标题位置**：回合标题挪到开头那几条用户消息（不含插话）之后，与 5.7.4 的图和 Codex 一致。
   - **完成的回合**：写"用时 {时长}"，默认折叠。前面是 14 的箭头，收起时朝右，展开时转 90°，动画 150 ms；下方有 1px 边线。
   - **其他状态**：运行中写"已处理 {时长}"；取消、中断、失败的文案不变；这几种都不折叠。
   - **折叠后仍显示的内容**：`final_item_ids` 指向的条目，加上常量 `ALWAYS` 里的 `plan`、`error`、`notice`、`late_tool_output`、请求记录，以及插话进来的用户消息。T132 的改动卡片把它的行类型加进 `ALWAYS` 即可。
   - **虚拟列表**：折叠的回合在行虚拟化里只占一行（被折叠的条目不生成行）。向前插入历史时沿用原来的 ScrollKeeper。
2. **列表**（`stream/StreamList.tsx`、`stream/actions.ts`）：
   - 列表持有展开的回合集合，对外提供 `toggleTurn`。
   - 新增 `jumpTo(itemId)`：先展开条目所在的回合，渲染后滚到包含它的行。条目在活动组里时，滚到所在的活动组。
3. **晚到的输出**（`rows/tools.tsx`，登记 `late_tool_output`）：
   - 默认折叠，显示"取消后仍在运行：{命令}"和"退出码 N"。
   - 展开后看输出；"跳到原条目"跳到原来的工具调用。
4. **原条目**：
   - 工具行在反向索引 `late_by_ref` 里有晚到输出时，旁边显示"最终退出码 N"（还没有退出码时显示"查看后续输出"），点击跳到晚到输出。
   - `item.completed` 带 `may_still_run` 的命令，立刻显示"命令可能仍在运行"。
5. **用例**：
   - UNIT-stream-rows 补了一个用例，覆盖：折叠与展开时各留哪些行、标题的位置、取消和运行中的回合不折叠、`final_item_ids` 为空时只剩标题、原条目带上晚到输出的退出码。
   - UI-stream-fold：亮暗两张折叠的截图，含一个以工具调用收尾、`final_item_ids` 为空的回合；另有展开再收起的一张。
   - UI-late-output：默认折叠、两个方向的跳转、最终退出码、`may_still_run`。
6. **复刻度审阅截图**：系统字体，4 张，在 `docs/tasks/T146-stream-fold/screenshots/`。

## `final_item_ids` 的核对结论

宿主的 `orchestrator/grouping.rs`、`coalescer.rs`、`turn.rs`、`session/core.rs` 我都看过：

- **填哪些**：最后一个过程条目之后的 agent 消息。过程条目指工具调用，或在活动组里的思考。回合里没有过程条目时，取全部 agent 消息。其他条目都不清空、也不加入答案。
- **适用范围**：规则在 coalescer 里，与 agent 无关，Claude 和 Codex 一样。
- **各种结局**：
  - `stop_reason` 除 `Cancelled` 以外（`end_turn`、`max_tokens` 等）都是 `completed`；
  - `cancelled` 和 `failed` 也按同一规则算；
  - 宿主重启时补发的 `interrupted` 一律为空。
- **正常结束却为空的情况**：回合以工具调用收尾，最后一个工具之后没有 agent 消息。你选了按设计只显示折叠标题，UI-stream-fold 里有一张这种情形的截图守着。
- **夹具**：只按这条规则补了现有夹具：markdown `[i_2]`、code-highlight `[i_1]`、plan-markdown `[i_2]`；error 那张失败、没有消息，保持空。

## `late_tool_output` 的缺陷修正

这原来是界面的一个缺陷：晚到的输出以前显示不出来。`flatten` 给 `late_tool_output` 行取的是 `ItemView.text`，但宿主把晚到的输出按 `item.delta` 的 `field: output` 流过来，视图把它存在 `ItemView.output`，所以行里的输出一直是空的。改为取 `output` 之后，单测和 UI-late-output 都能看到"test result: ok …"。原来这个行类型没有登记组件，只显示占位，所以之前没人发现。

## 关键取舍

- **跳转时先展开回合**：原条目可能在折叠的回合里，先展开才能滚过去。被跳到的行不加高亮，省下行数。
- **原条目旁的链接**：还没有退出码时显示"查看后续输出"，有了退出码再改成"最终退出码 N"。
- **`ToolLine` 的结构**：外面套了一层 flex，原来的展开按钮改用 `flex-1`，没有晚到输出时外观不变。UI-stream-tools 的变化只来自标题挪位，与这层改动无关。

## 验证方式

- **本机**：ui-core vitest 114 个、i18n vitest 10 个全过；tsc、eslint、prettier 都通过；整套截图用新基线跑，61 个全过。
- **基线清单**（run 41791295739，产物 61 个文件，逐个与分支上的做字节比较；亮暗都逐张看过）：

  | 基线 | 变化 | 原因 |
  |---|---|---|
  | stream-fold-light、stream-fold-dark、stream-fold-open、stream-late-original、stream-late-output | 新增 5 张 | — |
  | stream-markdown-light/dark | 变化 | 标题挪到用户消息下，"› 用时 8 秒"折叠，"已思考"收起 |
  | stream-code-light/dark | 变化 | 标题挪位，"用时"并折叠 |
  | stream-plan-markdown-light/dark | 变化 | 标题挪位，"用时"并折叠，计划和回答仍在 |
  | stream-tools-light/dark | 变化 | 运行中、不折叠，只是"已处理 12 秒"挪到用户消息下 |
  | stream-error-light/dark | 变化 | 失败、不折叠，只是"失败"标题挪位 |
  | 其余 46 个（含 stream-plan-steps 两张、meta.json） | 逐字节相同 | — |

  stream-plan-steps 两张不变，是因为截图滚到了计划那里，挪动后的标题不在画面里。基线提交为 b9527b3。
- **安全约束**：没用 `innerHTML` 一类写法；文案都走 i18n；agent 给出的命令、标题都经 `escapeInvisible`，只作为文本渲染。
- **规模**：`cargo xtask size` 是 +398 −52（净 +346），在停止线 500 以内（估计约 358）。

## 提交列表

```
f4b2603 docs(task): T146 fidelity screenshots with system fonts
b9527b3 test(ui-screenshots): UI-stream-fold and UI-late-output baselines, and the stream ones with the moved header
b588c87 docs(task): T146 notes: estimate, the host's final answer rule, size
1d50149 test(ui-screenshots): UI-stream-fold and UI-late-output
0a2fedc feat(ui-core): late tool output, its final exit code, and may_still_run
8faebbc feat(ui-core): completed turns fold to 用时 {duration}
```

## 遗留问题 / 范围外发现

- **以工具调用收尾的回合**：这种回合 `final_item_ids` 为空，折叠后只显示标题。这种情形下的显示，留给 T148 的复刻度审阅和本机 Codex 比对后再定。
- **T132 接手时**：回合改动卡片的行类型要加进 `rows/flatten.ts` 的 `ALWAYS`，并放在最终回答之后。
- **"已询问 n 个问题"不能展开**：T165 交接里提到请求记录的"已询问 n 个问题"可以展开看回答，这不在本任务范围内，没有做。
