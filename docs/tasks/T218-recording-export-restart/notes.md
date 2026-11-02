# T218 工作笔记

- 执行者：子 agent（Rust 工程师）

## 规模

| 项 | 估计 | 实际 |
|---|---|---|
| 诊断探针（48df587）：宿主在 adapter 结束、重启和会话 attach 的各步写日志；用例在第三个回合迟到或以别的状态结束时，打印回合、两条流、原始帧、host.log 和 adapter 进程。runner 上跑完就 revert（92cf4e3），净增 0 | 0（过程中约 +190） | 0（过程中 +194） |
| 修复：runtime 不再把被连接关闭切断的 prompt 的失败转给会话，加模块文档 | 5–10 | +9 |
| 测试：`crash()` 收紧；进程内 adapter 的确定性用例，含"别的 prompt 失败照旧转给会话" | 20–30 | +78 −6 |
| **合计**（`cargo xtask size`） | **约 30–40** | **+87 −6** |

估计偏小，是因为编排者批准方案时要求加确定性的 runtime 单元测试，并守住"别的 prompt 失败照旧转给会话"。进程内 adapter 的应答器约 30 行。在停止线 100 之内。

## runner 上失败的现场

- 4 次失败（41395945555 copy 3 run 10；41399294080 copy 1 run 6、copy 3 run 49、copy 1 run 184）的日志里只有一行 panic：`common/mod.rs:265` 的 `until` 报 `SELECT status FROM turns ORDER BY turn_no: not ["completed", "failed", "completed"]`。`until` 只打印期望值，不打印读到的值；宿主的 stderr 接到 `/dev/null`，`host.log` 在用例的临时目录里，用例结束就删了。所以从这几次失败本身看不出第三个回合是"慢"还是"已经以别的状态结束"。
- 4 次的二进制耗时是 32.86、32.98、32.86、32.92 s。同一次运行（41399294080）的 Run 步骤 24 分钟跑了 200 遍，整个二进制平均约 7.2 s。也就是说，这个用例开跑加上前两个回合一共不到 3 s，随后才是等满的 30 s。前两个回合在负载下照样很快，第三个回合却 30 s 都没到 `completed`：不像是负载把正常路径拖到 30 s 以上。

## 本机单份：正常路径的时间线

给宿主临时加了日志（探针提交 48df587），本机单跑一次（负载约 2.5）：

| 时刻（ms） | 事件 |
|---|---|
| 0 | runtime 把第二个回合的 prompt `exit 0` 发给实例 1 |
| +1 | 实例 1 退出（waitpid）；SDK 以 `incoming_transport_closed` 让这个 prompt 失败，会话收到 `PromptFailed`（类别 `adapter_exited`），第二个回合 `failed` |
| +2 | `adapters` 行写下 `ended_at`；之后 runtime 才结束实例 1，向会话广播 `AdapterExited`，会话的 link 这时才变成 `Detached` |
| +20 | 测试看到 `failed`，发出 `say again`；会话 attach（resume），runtime 起实例 2 |
| +40 | 第三个回合 `completed` |

关键在 +1 和 +2 之间：会话已经知道第二个回合失败了，但它的 link 还 `Attached` 在实例 1 上。

## 原因

1. adapter 在 prompt 中途退出时，有两条消息先后到达会话：
   - SDK 在读到 EOF 时让在途的请求失败，错误是 `-32603 Incoming transport closed`（`data.reason: incoming_transport_closed`）。T068 把它归为 `AgentErrorClass::AdapterExited`，runtime 把它作为 `PromptFailed` 转给会话，回合就此 `failed`（`adapter_exited`）。
   - `AdapterExited` 要等进程退出、连接关闭，并且 `launch::ended` 把 `ended_at` 写进 `host.db` 之后才由 `end()` 广播。会话收到它才把 link 改成 `Detached`、写 `agent_restarted` 通知（turn.rs 的 `adapter_exited`）。
2. 两条消息之间，会话认为回合已经结束，link 却还连着死掉的实例。这时来一条新消息：
   - 新回合直接进入 `Prompting`，把 prompt 发给 runtime；
   - runtime 要么在实例 1 的连接上发请求失败（`conn.request` 返回 Err，被 `drop(sent)` 丢掉），要么实例 1 已经结束、`link()` 为 `None` 直接返回。两种情况都没有任何回应；
   - 随后到达的 `AdapterExited` 和 link 里的实例对得上，正在 `Prompting` 的新回合被判成 `failed`（`adapter_exited`）。
3. 结果是 `["completed", "failed", "failed"]`。用例的 `until` 等满 30 s，报的仍是 `not ["completed", "failed", "completed"]`，和 runner 上的形态一样。
4. 平时这两条消息只差约 1 ms，测试要 20 ms 左右才发出下一条消息，赶不上。负载下中间隔着一次 `host.db` 写入（T205、T206：带 fullfsync 的检查点会让写入停几秒）和两三次 task 调度，窗口被拉长到能让下一条消息插进来。
5. 这违反设计 5.2.6 崩溃表："适配器进程退出……`link` 变为 `Detached`……下一次发送时重新连接并 `session/resume`"。是产品问题，不是 PATIENCE 不够。

## 复现

- **本机，确定性**：在 runtime 发出 `Wire::Exited` 之前临时插 300 ms 延迟（只在本机，没有提交）。修前每次都得到 `["completed", "failed", "failed"]`：第三个回合的 prompt 在第二个回合失败约 25 ms 后发给了实例 1，再过约 280 ms `AdapterExited` 到达，把它判成失败。修后同样的延迟下，第二个回合在 300 ms 后由 `AdapterExited` 结束，第三个回合 attach 到实例 2，`completed`。
- **runner，修前对照**（main 34df1c3，run 41405207333，整个二进制 `it_` 200 × 3）：598 过、2 败（copy 2 run 16、run 30），都是本用例，形态和 T216 那 4 次一样（`common/mod.rs:265`，二进制 33.07、32.70 s）。比 T216 时的 main 只多了文档、ids.toml 和 e2e-repeat.yml，宿主代码相同。
- **runner，探针**（48df587，run 41405203587，同样的参数）：598 过、2 败（copy 3 run 87、run 180），都是本用例，探针打印的回合都是 `["completed", "failed", "failed"]`，第三个回合 `failed`（`adapter_exited`），不是慢。两次的 host.log 时间线（毫秒，同一秒内）：

| | run 87 | run 180 |
|---|---|---|
| 第二个回合的 prompt 发出 | .398 | .944 |
| SDK 的失败到 runtime 并转给会话（`PromptFailed`，`adapter_exited`）；连接关闭 | .401 | .946 / .949 |
| waitpid 拿到退出 | .404 | .949 |
| `ended_at` 写完，`Wire::Exited` | .411 | .960 |
| runtime 结束实例 1（`end()`，广播 `AdapterExited`） | .412 | .960 |
| 第三个回合开始（测试在 .409 / .958 发出 `say again`） | .410 | .959 |
| 会话发出第三个回合的 prompt | .413 | .961 |
| 会话处理 `AdapterExited`，第三个回合 `failed` | .413 | .962 |
| runtime：实例已结束，prompt 被丢掉（`command dropped`） | .413 | .962 |

  窗口只有 10 ms 左右：退出比 EOF 晚 3 ms，`ended_at` 的写入用了 7–11 ms（本机约 1 ms）。测试每 20 ms 查一次库，刚好在窗口里查到 `failed` 就会撞上，所以是几百分之一的概率，而不是每次都超时。

## 修法

runtime 收到被连接关闭切断的 prompt 的失败（类别 `AdapterExited`）时不再转给会话；回合由实例结束时的 `AdapterExited` 结束。

- `AdapterExited` 在 `end()` 里发出，这时 runtime 已经清掉了实例和它的 wire。会话收到它才知道回合结束，同时 link 变成 `Detached`；下一条消息一定先 attach，而 attach 到 runtime 时旧实例已经不在，就排进 `waiting`，按退避起下一个实例再 resume。
- 回合的结果不变：`failed`、`adapter_exited`；挂着的交互请求照旧 `expired`（`ResolutionReason::AdapterExited`，SEC-10）；`agent_restarted` 通知照旧写。
- 事件顺序：通知现在落在回合的结束快照和 `turn.completed` 之前（`adapter_exited` 先 `end` 再写通知，`Prompting` 的回合要等结束快照）。修前 `AdapterExited` 先到时就是这个顺序，不是新形态。
- 只吞这一类：类别由 `claude::agent_error` 定，`data` 里带 `incoming_transport_closed` 才是 `AdapterExited`。agent 自己报的错（`-32000`、`-32603` 的 CLI 崩溃、Session not found 等）照旧作为 `PromptFailed` 转给会话，K6、K14 的处理不变。
- 不改重启、退避和清理的规则。

## 测试

- **`it_adapter_restart_a_cut_off_prompt_ends_with_the_instance`**（runtime 单元测试，test-harness，进程内 adapter）：
  - 应答器 `cutting`：`fail` 的 prompt 以 agent 的错误（`-32603 boom`）回复；`cut` 的 prompt 不回复，关掉自己的输出，读到宿主那边关闭连接之后才退出。所以 SDK 给这个 prompt 的失败一定先进 runtime 的 wire，`Wire::Exited` 在后，和 runner 上的先后一样，而且不靠时序。
  - 步骤：`fail` 的失败照旧到会话，类别 `Other`；`cut` 之后会话听到的第一个结果必须是 `AdapterExited`，听到 `PromptFailed` 就失败；之后"下一条消息"按回合机的做法 attach（resume），退避 1 s 后起第二个实例，prompt 完成。
  - 修前本机连跑 10 次，10 次都失败在 `PromptFailed(AdapterExited, "Incoming transport closed")`；修后 10 次都通过。
- **`crash()` 收紧**：IT-adapter-restart 和 IT-K21-reap-group 用的真进程 `exit 1`，也要求先听到 `AdapterExited`，不许有 `PromptFailed`。修前本机 IT-adapter-restart 连跑 5 次都失败，IT-K21-reap-group 的采样用例跑 1 次也失败（`ended` 的写入让 `Wire::Exited` 总在后面），修后都通过。
- 用例 `it_recording_export_two_instances_read_back_and_sanitize` 本身没有改。

## AdapterExited 是否在每条实例结束的路径上都会发出

被吞掉的失败只来自连接关闭，这时 `FromAgent::Closed` 一定随后进同一条 wire（连接的 future 结束后才发），所以 `closed` 一定成立；`end()` 剩下的条件是 `exit`。逐条看：

| 路径 | `exit` 从哪来 | `AdapterExited` | 回合 |
|---|---|---|---|
| adapter 自己退出（崩溃、`exit`、外部 kill -9） | waitpid 线程 → `launch::ended` 写库（结果不论成败都往下走）→ `Wire::Exited` | 发出；`Closed` 没到时 `Due::Kill` 在 2 s 后把 `closed` 置上 | 由它结束，`failed`（`adapter_exited`） |
| 进程内 adapter（GOLD-B） | `exit` 的 oneshot 结束 → `exited` → 同上 | 发出 | 同上 |
| runtime 自己关实例（`Part::stop`：宿主退出） | 关闭顺序 5.6.8 | 发出，但会话在 `stopping` 时忽略它 | 不靠它：supervisor 先停会话再停 runtime；强制退出时回合由 `HostExit` 标成 `interrupted`；不强制的退出只在没有回合运行时发生（`host/exit` 有回合时要 `force`，watch 的退出等回合结束） |
| kill 失败（`killpg` 出错被忽略） | 自己退出的路径里 `Due::Kill` 只在已有 `exit` 之后才设，`end()` 不查进程组 | 照样发出 | 不受影响 |
| 启动失败（`started(Err)`） | 没有实例 | 不需要：没有会话 attach 到它，attach 直接收到错误 | 回合以 attach 的错误结束 |
| **连接关了、进程不退**（adapter 关掉 stdout 却还活着） | 没有 | **不会发出** | 修后：回合停在 `Prompting`，直到进程退出，或用户中断（10 s 后 `cancelled`，`host_timeout`）。修前：回合当场 `failed`，但会话还连着这个只剩进程的实例，之后每个 prompt 都被静默丢掉，下一个回合同样挂住。所以修法只是把挂住从下一个回合挪到当前回合，不是新问题 |

最后一行可以补：`FromAgent::Closed` 到了而进程没退、也不在停止中时，按"意外结束"处理，杀掉进程组，`exit` 随之而来。这改的是 5.6.8 的清理规则，任务书不让我自己改，列为范围外发现。

另有一个理论上的情形：agent 自己在 prompt 的错误回复里带上 `incoming_transport_closed`，连接却不关。这时失败被吞，回合同样只能等中断。claude-agent-acp 不会这样回复，不处理。

## 考虑过、没有采用的做法

- **把 PATIENCE 加长**：没有用，第三个回合已经以 `failed` 结束，等多久都不会变成 `completed`。
- **在回合机里处理**：`PromptFailed` 带 `AdapterExited` 时把 link 改成 `Detached` 并写通知。还不够：新回合的 attach 到 runtime 时旧实例可能还没结束，`attach()` 会在关闭的连接上发 `session/resume`，attach 失败，回合照样 `failed`。要一起改 runtime 的 attach（实例正在结束时排进 `waiting`），改动跨两层，还要避免 `AdapterExited` 再写一次通知。
- **runtime 遇到发给已死实例的 prompt 回 `PromptFailed`**：不再"静默丢弃"，但第三个回合还是失败，没有按设计重连。

## 设计可以补的地方（交编排者）

- 5.2.6 崩溃表"适配器进程退出"一行：补一句"adapter 退出时 SDK 先以 `incoming_transport_closed` 让在途的 prompt 失败；runtime 不转发这个失败，回合由随后的 `AdapterExited` 结束，会话因此总在旧实例清掉之后才发下一条消息（T218）"。
- 5.6.2 的 `DriverEvent` 表：`PromptFailed` 不包括连接关闭造成的失败，这种情况由 `AdapterExited` 代替。
- 5.6.8：是否把"连接关闭而进程未退"也当作意外结束、杀进程组（见上表最后一行）。
