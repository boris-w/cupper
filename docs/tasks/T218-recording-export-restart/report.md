# T218 汇报

- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T218-recording-export-restart`
- 完成日期：2026-11-02
- PR：#36 https://github.com/boris-w/cupper/pull/36 。ci 运行：https://github.com/boris-w/cupper/actions/runs/41408172373 ，8 个检查全部通过。

**状态**：问题出在产品，已经修好。runner 上修前对照 600 次里本用例失败 2 次，修后 600 次全部通过。

## 做了什么

**1. 查明原因**（证据和时间线都写在 `docs/tasks/T218-recording-export-restart/notes.md`，已随分支提交）

- **现象**：失败时第三个回合不是"慢"，而是已经以 `failed`（`adapter_exited`）结束。回合表是 `["completed", "failed", "failed"]`。原用例的 `until` 只打印期望值，所以等满 30 s 后报的是 "not [completed, failed, completed]"。
- **过程**：
  1. adapter 在第二个回合的 prompt（`exit 0`）中途退出。SDK 先以 `incoming_transport_closed` 让这个 prompt 失败。runtime 把这个失败作为 `PromptFailed`（类别 `adapter_exited`，T068 加的）转给会话，第二个回合就此 `failed`。
  2. 这时会话的 link 还 `Attached` 在死掉的实例 1 上。`AdapterExited` 要等 waitpid 拿到退出、`launch::ended` 把 `ended_at` 写进 host.db、runtime 执行 `end()` 之后才发出。
  3. 测试恰好在这个窗口里看到 `failed`，发出 `say again`。会话把第三个回合的 prompt 发给了实例 1，runtime 静默丢掉这个 prompt。随后到达的 `AdapterExited` 和 link 里的实例对得上，就把第三个回合也判成 `failed`。
- **设计依据**：这违反设计 5.2.6 崩溃表："适配器进程退出……`link` 变为 `Detached`……下一次发送时重新连接并 `session/resume`"。
- **runner 上的证据**：临时探针 48df587，在 runner 上跑完后已由 92cf4e3 revert。探针抓到 2 次失败，回合都是 `[completed, failed, failed]`。两次的 host.log 时间线（毫秒）：

| | run 87 | run 180 |
|---|---|---|
| 会话收到 `PromptFailed`（adapter_exited） | .401 | .946 |
| waitpid 拿到退出 | .404 | .949 |
| `ended_at` 写完，`Wire::Exited` | .411 | .960 |
| 第三个回合开始（测试在 .409 / .958 发出） | .410 | .959 |
| 第三个回合的 prompt 发出，runtime 报 "command dropped" | .413 | .961–.962 |
| `AdapterExited` 到达会话，第三个回合 `failed` | .413 | .962 |

  窗口约 10 ms：退出比 EOF 晚 3 ms，写库 7–11 ms（本机约 1 ms）。测试每 20 ms 轮询一次，轮询正好落在窗口里才会撞上，所以失败率只有几百分之一。
- **本机确定性复现**（单份，未提交）：在发出 `Wire::Exited` 之前人为延迟 300 ms，修前每次都得到 `[completed, failed, failed]`。修后同样的延迟下，第二个回合由 `AdapterExited` 结束，第三个回合 resume 到实例 2 后 `completed`。

**2. 修复**（ccac2e4，`crates/cupper-host/src/driver/runtime/mod.rs`）

- 在 `Wire::Prompted` 的错误分支里，类别为 `AdapterExited` 的失败不再转给会话，回合统一由实例结束时的 `AdapterExited` 结束。
- `AdapterExited` 在 `end()` 清掉实例和它的 wire 之后才发出。会话因此总是在旧实例不在之后才把 link 改成 `Detached`；下一条消息先 attach，进入 `waiting`，按退避起新实例再 resume。
- 回合结果不变：仍是 `failed`（`adapter_exited`），挂着的请求仍是 `expired`（SEC-10），`agent_restarted` 通知照写。
- 模块文档加了一条规则。
- 不改重启、退避和清理的规则。

**3. 测试**（同一提交）

- **新用例** `it_adapter_restart_a_cut_off_prompt_ends_with_the_instance`（runtime 单元测试，test-harness，用进程内 adapter）。
  - 应答器先回一个 agent 自己的错误（`-32603 boom`）：会话照旧收到 `PromptFailed`（类别 `Other`），用来守住"只吞 adapter_exited 一类"。
  - 然后在 `cut` 这个 prompt 上关掉自己的输出，等读到宿主关闭连接之后才退出。所以 SDK 的失败一定先进 wire，不靠时序。会话听到的第一个结果必须是 `AdapterExited`；之后的下一条消息 resume，在第二个实例上完成。
  - 本机修前 10/10 失败，修后 10/10 通过。
- **`crash()` 收紧**：真进程 `exit 1` 时，也不许在 `AdapterExited` 之前出现 `PromptFailed`。修前 IT-adapter-restart 本机 5/5 失败，修后通过。
- `it_recording_export_two_instances_read_back_and_sanitize` 本身没有改。

## 关键取舍

- **为什么改 runtime，不改回合机**：考虑过在 `PromptFailed(adapter_exited)` 时由回合机把 link 改成 `Detached`、自己写通知。这样不够：新回合的 attach 到 runtime 时旧实例可能还没结束，`session/resume` 会发在已关闭的连接上，attach 失败，回合照样 `failed`。要一起改 runtime 的 attach，改动跨两层，还要防止通知写两次。现在的改法只动一处，路径就是设计 5.5 时序图里的路径。
- **为什么不放宽 PATIENCE**：第三个回合已经结束为 `failed`，等多久都不会变成 `completed`。
- **事件顺序**：`agent_restarted` 通知现在落在第二个回合的结束快照和 `turn.completed` 之前。修前 `AdapterExited` 先到的那一支本来就是这个顺序，不是新形态。没有 golden 或用例依赖这个顺序。
- **`AdapterExited` 在各条实例结束路径上是否一定发出**（编排者第 2 点）：
  - **adapter 自己退出**（崩溃、`exit`、外部 kill -9）：waitpid → `ended` 写库（成败都往下走）→ `Wire::Exited` → `end()`，一定发出。连接的 `Closed` 迟迟不到时，`Due::Kill` 在 2 s 后把 `closed` 置上。
  - **进程内 adapter**：`exit` 的 oneshot 结束后走同一条路，一定发出。
  - **kill 失败**：在 adapter 自己退出的路径上，`Due::Kill` 只在已经拿到 `exit` 之后才设，`end()` 不检查进程组，所以照样发出。
  - **runtime 自己关实例（宿主退出）**：会发出，但会话处于 `stopping` 时忽略它，回合也不靠它结束：supervisor 先停会话；强制退出时回合由 `HostExit` 标成 `interrupted`；不强制的退出只在没有回合运行时发生。
  - **启动失败**：没有实例，attach 直接收到错误，回合以这个错误结束。
  - **缺口：连接关了、进程不退**。这时拿不到 `exit`，`AdapterExited` 不会发出。修后回合停在 `Prompting`，要等进程退出或用户中断（10 s 后 `cancelled`，`host_timeout`）。修前同样的情况下，回合当场 `failed`，但会话仍连着死实例，后续 prompt 都被静默丢掉，下一个回合照样挂住。按编排者裁定不在本任务处理，见遗留问题 1。
- **Codex**（编排者问的位置）：runtime 现在在 `Actor::wire` 的 `Wire::Prompted` 错误分支里写死了 `claude::agent_error`，新加的判断就在它后面（`driver/runtime/mod.rs`，约第 853 行）。Codex 的分类器 `driver/codex/mod.rs` 的 `agent_error` 已经把 `incoming_transport_closed` 归为 `AdapterExited`。T111 接 Codex 时：
  - 如果仍用同一个 runtime actor、只按 agent 换分类器，这条判断要保留在换上去的分类器之后，就自动适用；
  - 如果 Codex 另起 actor，要在它的 prompt 结果处理里加同样的判断。
  - `Wire::Opened`、`Wire::Configured` 也写死了 `claude::agent_error`，T111 会一起改到。

## 验证方式

**runner**（test-repeat，macos-15，kind=cargo，package=cupper-host，args=`--features test-harness --test lifecycle`，filter=`it_`，200 × 3）

| 运行 | 提交 | 结果 |
|---|---|---|
| 修前对照 https://github.com/boris-w/cupper/actions/runs/41405207333 | main 34df1c3 | 598 过、2 败（copy 2 run 16、30），都是本用例，形态和 T216 那 4 次一样（等满 30 s，二进制 33.07 / 32.70 s） |
| 探针 https://github.com/boris-w/cupper/actions/runs/41405203587 | 48df587 | 598 过、2 败（copy 3 run 87、180），都是本用例，回合为 `[completed, failed, failed]`，时间线见上 |
| 修后 https://github.com/boris-w/cupper/actions/runs/41408172015 | 27086b0 | **600 过、0 败**：`results.txt` 600 行全是 ok，每份 200；`binaries.txt` 只有 lifecycle |

- 修前对照跑在 main 34df1c3 上，不是 db8b97a。两者之间只多了文档、`tests/ids.toml` 和 `e2e-repeat.yml`（T099 第一部分），宿主代码相同。
- 修前对照与 T216 合计 1,500 次里本用例失败 6 次（约 0.4%）。修后若失败率不变，600 次全过的概率约为 0.996^600 ≈ 9%；加上探针抓到的机制和确定性用例，可以认定已经修好。

**本机**（都只跑一份）
- fmt 通过。clippy `-D warnings` 无告警，覆盖无特性、test-harness、all-features 三组，均带 `--all-targets`。
- cupper-host lib（test-harness）：456 过，3 个 ignored。
- 不带特性的 `driver::runtime`：15 过。
- lifecycle 19 过，interaction_trust 7 过，gold_b 12 过。
- 规模：`cargo xtask size` 输出 `T218: +87 -6 (net +81); 13 left to the stop line 100, hard stop 140`。开工时估计约 30–40 行，偏小，是因为批准方案时加了确定性用例和"别的失败照旧转发"的守护。

**对照 brief 的验收标准**
- [x] **原因有证据**：runner 修前对照和探针的失败、探针抓到的 host.log 时间线、本机加延迟的确定性复现、修前修后的确定性单元测试。
- [x] **修后 runner 上整个二进制 600 次本用例 0 次失败**：run 41408172015。
- [x] **PR 上 8 个检查都通过**：PR #36，ci 运行 41408172373。8 个检查是 compat-framework、e2e、license、protocol、rust（18m45s）、testdata-scan、ts、ui。

## 提交列表

```
27086b0 docs(task): T218 notes on the restart race, its runner evidence and the fix
ccac2e4 fix(host): a prompt the adapter's exit cuts off ends with the instance
92cf4e3 Revert "test(host): a probe times IT-recording-export's restart (T218)"
48df587 test(host): a probe times IT-recording-export's restart (T218)
```

## 遗留问题 / 范围外发现

1. **"连接关了、进程不退"时实例永不结束**（编排者已定为交接事项）。
   - 修前，回合当场失败，但会话仍连着死实例，后续 prompt 被静默丢掉，回合挂住；修后，挂住的是当前回合。两种情况都要靠用户中断。
   - 可能的补法：`FromAgent::Closed` 到了、进程没退、也不在停止中时，按意外结束处理，杀掉进程组，`exit` 随之到来。这要改 5.6.8 的清理规则。
   - 另有一个理论上的情形：agent 自己在 prompt 的错误回复里带上 `incoming_transport_closed`，连接却不关，回合同样只能等中断。claude-agent-acp 不会这样回复。
2. **设计可以补的三处**（交编排者改）：
   - 5.2.6 崩溃表"适配器进程退出"一行：补一句"adapter 退出时 SDK 先以 `incoming_transport_closed` 让在途的 prompt 失败；runtime 不转发这个失败，回合由随后的 `AdapterExited` 结束，所以会话总在旧实例清掉之后才发下一条消息（T218）"。
   - 5.6.2 的 `DriverEvent` 表：`PromptFailed` 不包括连接关闭造成的失败，这种情况由 `AdapterExited` 代替。
   - 5.6.8：是否把"连接关闭而进程未退"也当作意外结束（见第 1 条）。
3. **同类窗口在 attach 上也存在**，没有改：adapter 恰好在 `session/new` 或 `session/resume` 途中退出时，attach 以 `adapter_exited` 失败，link 变成 `Detached`。之后 `AdapterExited` 到达时 link 不是 `Attached`，所以不写 `agent_restarted` 通知。下一条消息如果赶在旧实例结束之前 attach，也会再失败一次。这比 prompt 的情形少见得多，可以在第 1 条的交接里一起看。
4. **测试辅助的建议**：`common/mod.rs` 的 `until` 失败时只打印期望值、不打印实际读到的行。这次正是因为这样，从 T216 那 4 次失败看不出第三个回合已经 `failed`。建议让它带上最后一次读到的值，约 3 行；超出本任务范围，没有改。
5. **修后连跑的结果没有写进 `notes.md`**：notes 在推送修复之前就提交了，修后连跑的结果只写在本汇报里。runner 物料和本机临时文件都已清理，runner 上的物料保留 14 天。
