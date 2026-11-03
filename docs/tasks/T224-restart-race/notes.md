# T224 工作笔记

## 原因

- **两条消息的先后有保证**：`AgentRuntime::restart()` 和 `AgentPort::send()`（会话的命令，含 `Prompt`）都往同一个 `mpsc::UnboundedSender<Mail>` 里发（`Mail::Restart`、`Mail::Command`），运行时 actor 在一个循环里按顺序取。用例里两次发送之间没有 `.await`。所以 prompt 不会先于 `Mail::Restart` 到达，任务书里的推测不成立。
- **没有保证的是 prompt 和"重启做完"的先后**：`Mail::Restart` 开始停止之后，停止不靠时钟也能做完：假 agent 应答 `session/close`，stdin 关闭后它自己退出，连接关闭，`end()` 广播 `AdapterExited`。全程只有几次真实 I/O，本机约 2 ms 以内。用例的任务如果在两次发送之间被挂起（runner 负载高时，线程被抢占几毫秒），prompt 到达时实例已经结束：`stopping` 已清空，`link()` 为空，运行时丢弃这个 prompt。会话先收到的就是 `AdapterExited { reclaimed: false }`，与失败日志一致（`exit status 0`，不是回收）。
- **本机确定性复现**：在 `restart()` 和 `f.prompt(...)` 之间插入一次真实时间的等待（`SystemClock` 的定时器）。延迟 2 ms、10 ms、50 ms、500 ms 各跑 3 次，全部以同一条 panic 失败（`tests.rs` 的 `assert!(lost, ...)`，`AdapterExited { …, status: ExitStatus(unix_wait_status(0)), stderr: "", reclaimed: false }`）。实验代码没有提交。

## 产品行为

两种先后对会话都正确，没有发现丢消息或回合挂住：

- prompt 在停止期间到达：运行时回 `PromptFailed(SessionNotFound)`。会话的回合机按 K6 resume 后重发（只在这个 prompt 还没有产生更新时）；随后到达的旧实例的 `AdapterExited` 因为 link 已不是 `Attached` 而被忽略。
- prompt 在实例结束之后到达：运行时丢弃它。发出它的会话一定在 `end()` 时的路由表里，`AdapterExited` 已经在它的队列里排在后面，回合由这条 `AdapterExited` 以 `adapter_exited` 结束（T218）。会话自己的 resume 一定排在这个 prompt 之后（同一信道），所以旧 prompt 不会被送到新实例上的同一个会话。

用例里另外发现：假 agent 的会话 ID 是每个进程从 `fake-0-1` 数起，新实例的第一个会话和旧会话同 ID。如果用例先 attach 新会话、再发旧会话的 prompt，这个 prompt 会被当成新会话的 prompt 送出去。这是用例的先后不真实（真实的会话一定先发 prompt 再 resume）加上假 agent 的 ID 造成的，真实 agent 的会话 ID 是 UUID。

## 用例的改法

- `it_adapter_restart_backs_off_then_crashed`：重启后先等到 `AdapterExited`，再发旧会话的 prompt，确定性地走"实例结束之后"这条路径：prompt 没有回应，随后新 attach 不退避，新会话说的是自己的内容。
- 新增 `it_adapter_restart_a_prompt_during_the_stop_is_sent_again`：假 agent 带 `--fault linger`，stdin 关闭后不退出、忽略 SIGTERM，停止要等时钟推进（关闭 stdin 后 3 s 发 SIGTERM，再 2 s SIGKILL），而时钟只由用例推进。所以无论用例的任务被挂起多久，prompt 到达时都还在停止中，确定性地得到 `PromptFailed(SessionNotFound)`；之后推进时钟、等到 `AdapterExited`，resume 后重发成功。本机在 `restart()` 后插入 200 ms 真实等待，它照样通过。

## runner 连跑

参数：`kind=cargo`、`package=cupper-host`、`args=--features test-harness --lib`、`filter=it_adapter_restart`、`times=200`、`parallel=4`（4 份同时跑，给 runner 加负载）、`runner=macos-15`。修前在 main，修后在本分支。
