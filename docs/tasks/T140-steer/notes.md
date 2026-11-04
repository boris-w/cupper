# T140 notes

## 1. 按模块估行数

| 模块 | 做什么 | 估计 |
|---|---|---|
| `orchestrator/turn.rs`（纯核心） | `SendRequest` 带 `expected_turn`；`Attached` 时记下 `steer.supported`；三项前提；在途的插话（每会话至多一个）和 5 s 定时；结果的映射；`StartedNewTurn` 之后的新回合；回合在插话在途时结束；晚到的应答；`SteerFallback` 补到六个值 | 170 |
| `orchestrator/queue.rs` | 按 `queue_id` 取出一条（晚到应答用） | 10 |
| `orchestrator/autonomous.rs` | `steer_new_turn` 的回合也按自发回合判定结束 | 3 |
| `orchestrator/session/actor.rs` | 传 `expected_turn_id`；执行 `Steer`（经端口的 future，结果经 channel 回到 actor，与 `set_config` 同法）；六个原因的映射 | 45 |
| `driver/api.rs`、`driver/runtime/mod.rs` | `AgentPort::steer`；runtime 发 `_session/steering`（`idleBehavior: promptRequired`），应答映射成 `SteerOutcome`，错误按 `AgentError` 分类 | 55 |
| CLI（`args.rs`、`run.rs`） | `send --steer`：`session/get` 取正在运行的回合，输出 disposition 和原因 | 35 |
| 假 agent | `steer_prompt_required`、`steer_error` 两条脚本命令；`--without steering\|plan\|thinking` | 40 |
| 计划、思考的降级（见第 3 节，取方案二时） | 能力由 agent 给出的选项推出 | 0 或 20 |
| **产品小计** | | **360–380** |
| 测试：`turn.rs` 单元 | 各条路径、六个原因、晚到应答、回合结束时的插话 | 150 |
| 测试：PROP-steer-only-prompting | `props.rs` 的操作序列加上插话结果，断言"只在 `Prompting` 且回合对得上时发 `Steer`" | 40 |
| 测试：runtime 单元 | 请求的形状、四种 outcome、错误 | 30 |
| 测试：IT-K10、IT-K11 | 进程级，`--calls` 断言有没有 steering | 65 |
| 测试：IT-degrade-steering、-plan、-thinking | 进程级 | 75 |
| 测试：IT-cli-commands 新增一条 | | 15 |
| 测试的夹具改动（`AgentPort` 的测试替身等） | | 15 |
| **测试小计** | | **390** |
| **合计** | | **约 750–770** |

超过停止线 600 和硬停止线 700。可以拆的地方：
- **方案 A**：IT-degrade-plan、IT-degrade-thinking，连同假 agent 的 `--without plan|thinking` 和第 3 节的降级，另开任务。这部分和 Steer 无关，约 120–140 行。拆完约 620–640 行。
- **方案 B**：在 A 的基础上，再把 CLI 的 `--steer` 和 IT-cli-commands 的新增一条也拆出去，约 50 行。剩下约 570–590 行。但这样宿主侧暂时只有 IT 一个使用者。

## 2. 两个时序问题

### 2.1 超时之后才到的应答

5 s 到时，消息转为排队（`fallback_reason: timeout`），回复 `queued` 和 `queue_id`。这个回复已经发出，不再改。核心记下"晚到的插话"，内容是 `queue_id` 和目标回合；之后到的应答按下表处理：

| 晚到的应答 | 条件 | 处理 |
|---|---|---|
| `Injected` | 排队的那一条还在队列里，并且目标回合还没有写 `turn.completed` | 从队列取出（写 `queue.updated`），在目标回合里写一条 `steered: true` 的用户消息。事件流和 agent 实际做的一致：消息在回合里，不在队列里 |
| `Injected` | 排队的那一条已经出队（已经开了回合），或者目标回合已经完成 | 不再处理，只写宿主日志。agent 会收到两次这条消息，界面上也能看到两次。这是 5 s 超时的代价，概率很低（实测 1–4 ms 就应答） |
| `StartedNewTurn` | 那一条还在队列里 | 从队列取出；等目标回合结束后，开 `origin: steer_new_turn` 的回合（见 2.3） |
| `StartedNewTurn` | 已经出队 | 不再处理；agent 自己开的回合按自发回合（K9）出现 |
| `PromptRequired`、`Unsupported`、错误 | | 什么也不做，消息本来就在队列里 |

更简单的做法是晚到的应答一律只记日志，接受重复。这样能省大约 25 行。我倾向上表的做法。

### 2.2 等应答的 5 s 里，回合结束或被中断

原则是：回合的结束要等在途的插话有了结果（应答或 5 s），这样消息只会出现在一个地方。具体是：
- **prompt 返回**（`end_turn`、`cancelled`、出错），或者中断之后到了 10 s 截止：回合照常 `Settle`、拍结束快照，但 `turn.completed` 推迟到插话有结果之后（最多 5 s）。
  - 插话结果是 `Injected`：在这个回合里写 `steered` 的用户消息（仍在 `turn.completed` 之前），回复 `steered` 和这个回合。
  - 结果是 `StartedNewTurn`：回合完成后接着开 `steer_new_turn` 的回合。
  - 结果是其余几种，或者超时：转为排队，回复 `queued` 和原因。这个回合结束时队列照常往下走，或者照常暂停：中断、失败时暂停，消息留在队列里等用户。
- **中断本身**（`Prompting` → `Cancelling`）不取消在途的插话，等它的结果，处理同上。中断之后再来的 `steer` 不满足"处于 `Prompting`"，转为排队，原因是 `not_prompting`。
- **适配器退出**：端口的 future 以 `ProcessExited` 结束，按出错处理：排队，原因 `agent_error`。回合由 `AdapterExited` 照常结束。
- **宿主强制退出**（`HostExit`）：回合立刻以 `interrupted` 完成，在途的插话当场转为排队（`agent_error`），不再等。
- **在途期间又来一条 `steer`**：每个会话同时只等一个插话的应答。actor 的延后回复只能挂一个；而且插话最多 5 s 就有结果。第二条有两种处理方式，请你定：
  - (a) 转为排队，原因用现有的 `not_prompting`。语义不太准，但不改协议。
  - (b) 附加一个新原因，例如 `steer_pending`。这是协议附加，需要你同意。
  - 在途期间来的 `mode: queue` 照常立即排队。

### 2.3 `StartedNewTurn`

按 SP-1 s06：只有 agent 在插话那一刻已经空闲时，才会返回 `startedNewTurn`。Codex 忽略 `idleBehavior`；Claude 带了 `promptRequired` 时返回 `promptRequired`。宿主只在 `Prompting` 时插话，所以它只会出现在"agent 的回合刚结束、宿主还没收到 prompt 响应"这个竞态里，此时原回合的 `PromptDone` 马上就到。处理：
- **原回合**：照常等它的 `PromptDone`，以它的 `stop_reason` 和 usage 结束，宿主不替它判定。
- **新回合**：原回合完成之后，先开 `origin: steer_new_turn` 的回合，再考虑出队。新回合里写一条 `steered: true` 的用户消息；没有 prompt，阶段与自发回合相同，由 `AutonomousJudge` 判定结束。这一点要在 `autonomous.rs` 里让 `steer_new_turn` 与 `autonomous` 同样处理。
- **回复**：`steered` 和新回合的 `turn_id`，在新回合开出来的那一步发出。

## 3. IT-degrade-plan、IT-degrade-thinking 在宿主这一层断言什么

现状：Claude、Codex 的能力里，`interactions.plan_confirm`、`extraction.thinking`、`extraction.plan` 都是按 agent 种类写死的常量，不是从 agent 读出来的。agent 能让宿主"知道"计划和思考不可用的，只有它给出的配置选项：
- 计划：Claude 的 `mode` 选项里有没有 `plan` 这个值；Codex 有没有 `collaboration_mode`（宿主 id 是 `plan_mode`）。
- 思考：有没有 `thought_level` 类的选项（宿主 id 是 `reasoning_effort`）。

假 agent 加 `--without plan` 和 `--without thinking`：前者去掉上面计划相关的值或选项，从此不发 `plan_update`，也不发计划确认；后者去掉 `thought_level` 选项，从此不发思考块。下面两种方案请你定。

**方案一：只锁定现有行为，产品代码不改。**
- IT-degrade-plan：
  - `session/get` 和 `agents/capabilities` 的能力里，Claude 的 `permission_mode` 没有 `plan` 值，Codex 没有 `plan_mode` 选项；
  - `session/set_config` 设这个值或选项，回 `invalid_params`，假 agent 的 `--calls` 里没有 `session/set_config_option`；
  - 回合照常进行，不出现计划条目。
- IT-degrade-thinking：
  - 能力里没有 `reasoning_effort` 选项；
  - 设它时回 `invalid_params`，agent 收不到；
  - 回合里没有思考条目。

**方案二：在方案一的基础上，再让能力的两个字段跟着选项走**（约 20 行，加单元断言）：
- 计划不可用时，`interactions.plan_confirm` 为 false；
- 思考不可用时，`extraction.thinking` 为 false。

界面（T242 以后）就能按这两个字段隐藏计划确认、思考的显示。`extraction.plan` 没有"无"这个取值，要表达"不可用"就得给枚举附加一个值，需要你同意，这次不做。

我倾向方案二：AG-4 说的"按能力隐藏"，能力本身得反映出来。不过它依赖"有没有这个选项"这种推断，不是 agent 明说的。如果你更看重准确，就取方案一，把推断留到 agent 真有能力位的时候再做。

## 4. 定下的做法（2026-11-04，main）

- 拆分取 A：IT-degrade-plan、IT-degrade-thinking 和计划、思考的降级归 T243（假 agent 的 `--without plan|thinking` 也归它）；本任务只保留关掉 steering 的开关和 IT-degrade-steering。停止线 650，硬停止线 720。
- 时序按第 2 节的完整方案：晚到的 `Injected`、`StartedNewTurn` 那一条还在队列里就取出来处理，已出队的只记日志；等应答的 5 s 里回合结束，`turn.completed` 推迟到插话有结果之后（最多 5 s）；中断不取消在途的插话；适配器退出按 `agent_error`；`steer_new_turn` 由 AutonomousJudge 按 autonomous 同样对待。每条都有用例，`turn.completed` 的推迟写进单元或 PROP。
- 插话在途时又来一条 `steer`：附加 `FallbackReason::steer_pending`（协议附加，补样例，schema-compat 为 additive）。
- 计划、思考的断言留给 T243；我倾向的方案二记进 T243 的任务书。

### 实现上的一处取舍

`Steer` 的结果不另开回复的 future，而是像 `Prompt` 一样以驱动事件回到会话（`SteerDone`、`SteerFailed`）：每个会话同时只有一个插话在途，晚到的结果按"晚到"判，不需要再对号；actor 也不必再开一个 channel。适配器退出时不等 SDK 的错误，由核心在 `AdapterExited` 时把在途的插话当作 `agent_error`。

## 5. 越线之后（2026-11-04，main）

做到 +730 时越过硬停止线 720，停下来报告。main 决定剩下的部分（CLI `--steer`、IT-cli-commands 的一条、IT-K10、IT-K11、IT-degrade-steering）全部在本任务做完，不拆；硬停止线放到 880。测试可以为了好读而整理（表驱动、共用助手），不为压行数挤断言。
