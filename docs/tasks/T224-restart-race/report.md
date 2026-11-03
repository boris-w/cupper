# T224 汇报

- 执行者：子 agent（Rust 工程师，驱动层与运行时）
- 分支：`fix/T224-restart-race`（头提交 `7d2ec35`，基于 main `10f2a8d`）
- 完成日期：2026-11-03
- PR：https://github.com/boris-w/cupper/pull/45 。`7d2ec35` 上 8 个检查全部通过：https://github.com/boris-w/cupper/actions/runs/41594435351
- runner 连跑，参数都是：kind=cargo，package=cupper-host，args=`--features test-harness --lib`，filter=`it_adapter_restart`，times=200，parallel=4，runner=macos-15。
  - 修前，main 10f2a8d：https://github.com/boris-w/cupper/actions/runs/41589780790 ，750 次通过，50 次失败。
  - 修后第一版，a91abb7（只改了先后）：https://github.com/boris-w/cupper/actions/runs/41589784125 ，800 次全部通过。
  - 修后最终版，7d2ec35：https://github.com/boris-w/cupper/actions/runs/41594431160 ，800 次全部通过。

## 做了什么

**修前那次连跑失败了 50 次，都是 `it_adapter_restart_backs_off_then_crashed`**（从构件里逐次的日志核实）。原因有两个。

1. **49 次在 `tests.rs:823`：先到的是 `AdapterExited { …, status: ExitStatus(unix_wait_status(0)), stderr: "", reclaimed: false }`。**
   - 任务书的推测不成立。`AgentRuntime::restart()` 和会话的命令（`AgentPort::send`）往同一个 `mpsc::UnboundedSender<Mail>` 里发，运行时 actor 在一个循环里按顺序取。用例的两次发送之间也没有 `.await`。所以 prompt 不会先于 `Mail::Restart` 到达。
   - 真正没有保证的，是 prompt 到达和"重启做完"的先后。重启的停止不靠时钟也能做完：假 agent 应答 `session/close`，stdin 关闭后自己退出。整个过程只有几次真实 I/O，本机约 2 ms。如果用例的任务在两次发送之间被挂起（runner 负载高时线程被抢占），prompt 到达时实例已经结束，运行时就把它丢弃，会话先收到的是 `AdapterExited`。
   - 本机确定性复现：在两次发送之间插入 2、10、50、500 ms 的真实等待，每种跑 3 次，全部以同一条 panic 失败。

2. **1 次在 `tests.rs:828`（copy 3 第 23 次）：重启之后的 attach 在时钟上用了 2 s，期望是 0。**
   - 原因：运行时在一步的中间（`end()` 里）就向会话广播 `AdapterExited`，而这一步取消的 SIGKILL 定时器（`Due::Kill`，2 s）在 `ManualClock` 上的登记，要到这一步结束、`Alarm::follow` 换掉闹钟时才去掉。`ManualClock::next_deadline` 会把这条还没去掉的登记算进去。用例一听到事件就 `tick`，如果运行时的线程还没走到 `follow`，时钟就被推到那个已经取消的截止时间。
   - 2 s 正是 `ADAPTER_TERM_WAIT`。重启已经清掉了失败记录，不会有退避，所以排除了退避的可能。
   - 本机确定性复现：在 `end()` 广播之后注入 50 ms 的真实等待，原用例每次都失败（崩溃那一段先失败，`left: 3s, right: 1s`）；改完之后，运行时的全部用例在同样的注入下都通过。
   - 这个问题在第一版 a91abb7 里仍然可能出现，所以又补了一个提交。

**用例的改法**（只改 `driver/runtime/tests.rs`）：
- `it_adapter_restart_backs_off_then_crashed`：重启后先等到 `AdapterExited`，再发旧会话的 prompt，确定性地走"实例结束之后"这条路径。断言三点：这个 prompt 没有任何回应；新的 attach 不退避；新会话说的是自己的内容。
- 新增 `it_adapter_restart_a_prompt_during_the_stop_is_sent_again`：假 agent 带 `--fault linger`，stdin 关闭后不退出、忽略 SIGTERM，停止只能等用例推进时钟。所以无论用例的任务被挂起多久，prompt 到达时都还在停止中，确定性地得到 `PromptFailed(SessionNotFound)`；之后推进时钟到 `AdapterExited`，resume 后重发成功。本机在 `restart()` 后插入 200 ms 真实等待，它照样通过。
- `Fixture::settled()`：做一次 `status()` 往返。运行时按顺序处理信箱，所以应答回来时，广播 `AdapterExited` 的那一步（包括它的 `follow`）已经做完。`Fixture::crash` 和重启那一段在读时钟之前都先调用它。

## 产品行为

查下来没有问题，两种先后都不丢消息，回合也不会挂住：
- **prompt 在停止期间到达**：运行时回 `SessionNotFound`，会话按 K6 resume 后重发；之后到达的旧实例的 `AdapterExited` 会被忽略。
- **prompt 在实例结束之后到达**：运行时丢弃它，但发出它的会话一定已经收到了 `AdapterExited`，回合由这条事件以 `adapter_exited` 结束（T218）。会话自己的 resume 一定排在这个 prompt 之后（同一个信道），所以旧 prompt 不会被送到新实例上。
- **第二个原因**只影响 `ManualClock` 上的用例：真实时钟不会被推进，取消的定时器只是晚一点被丢弃。
- **用例里顺带发现**：假 agent 的会话 ID 是每个进程从 `fake-0-1` 数起，新实例的第一个会话和旧会话同 ID。如果用例先 attach 新会话、再发旧会话的 prompt，这个 prompt 会被当成新会话的送出去。这是用例的先后不真实造成的（真实的会话一定先发 prompt 再 resume），真实 agent 的会话 ID 是 UUID。

## 验证方式

- 本机单份运行：`it_adapter_restart` 的 3 个用例和运行时的全部用例都通过；clippy（test-harness）没有警告。上面两个原因的复现实验做完都已撤掉，没有提交。
- runner 上的连跑和 PR 上的 CI 结果见文首。

## 规模

增 39、删 8，停止线是 80。

## 提交列表

```
7d2ec35 docs(task): record the second failure of T224's runs
017d582 test(host): let the runtime re-arm its alarm before ticking on
a91abb7 docs(task): add the T224 notes
3b365fb test(host): cover both orders of a restart and a prompt
```

## 遗留问题 / 范围外发现

1. **同一类时序在别处可能还有**：凡是用 `ManualClock` 推进时钟的用例，只要在听到运行时某个中途发出的事件后立刻 `tick`，都可能碰到"已经取消、但闹钟还没换掉"的截止时间。这次只修了运行时用例里在 `AdapterExited` 之后读时钟的两处。建议以后写这类用例时都先等运行时把手头这一步做完；也可以考虑在 `Alarm` 或 `TimerTable` 取消定时器时立刻撤掉时钟上的登记，这属于产品代码，需要另行决定。
2. **假 agent 的会话 ID 每个进程都从 1 数起**：要是以后有用例需要区分新旧实例上的会话，可以让 `--segment-file` 或进程号参与生成 ID。
3. **worktree 保留着**，等编排者合并后再清理。
