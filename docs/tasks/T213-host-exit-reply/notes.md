# T213 笔记

## 行数估计（开工前）

停止线 120，硬停止线 160（`cargo xtask size`，含测试）。

| 模块 | 估计新增 | 内容 |
|---|---|---|
| `tests/gold_b.rs` | 8 | `stop()` 照 T196 判定：应答，或连接以 `Eof`、`Refused(PeerExited)` 结束 |
| `tests/common/mod.rs` | 10 | `host_gone` 从 `lifecycle.rs` 挪过来共用（`lifecycle.rs` 相应删掉） |
| 桌面 `restart.test.ts` | 10 | 退出请求得不到应答（被拒、超时）时照样退出 |
| 合计 | 约 30 | |

**开工后改了方案**（估计补在这里，晚于改动本身）：

- "连接以宿主退出的方式结束"这条判定，CLI 的 `run.rs` 里已经有一份，测试里又有一份。改成 CLI 库里的 `CloseReason::host_gone` 一处定义，`run.rs`、IT-lifecycle、GOLD-B 都用它，不在 `common` 里再放一份。
- 补一个确定性的 CLI 客户端单元测试：宿主写完应答、关掉自己那端之后客户端才读，应答被拒收。
- 估计改为约 65。

**实际**：`cargo xtask size` 为 +89 −26（净增 63），在停止线 120 之内。

## 现象

- main 35c713c 的 CI（run 41015350073，rust 作业的 `golden` 步骤）：`gold_b_claude_approval` 失败在 `gold_b.rs:394:20`，`Closed(Some(Refused(Refusal { kind: PeerExited, call: "getsockopt(LOCAL_PEERTOKEN)", code: 57 })))`。
- 出错的是收尾 `stop()` 里 `host/exit` 那一行的 `unwrap()`。

## 根因

- GOLD-B 的宿主跑在测试进程里，客户端是 CLI 库的 `Client`，两者在同一个多线程 tokio 运行时上。
- T205 之后，`host/exit` 的应答一定在连接关闭之前进入发送队列。宿主随后发 `host/shutdown`，停各部分，然后关掉连接。
- 客户端每读一次就比对一次宿主的令牌（`still_pinned` → `getsockopt(LOCAL_PEERTOKEN)`）。宿主关掉自己那一端之后，这个查询返回 `ENOTCONN`（57），归为 `PeerExited`，这次读到的字节随之丢弃，请求以 `Closed(Refused(PeerExited))` 失败。
  - 宿主进程还活着也是这样：`ENOTCONN` 说的是对端已经关闭，不是进程已经退出。所以 T205 笔记里"GOLD-B 的宿主跑在测试进程里，客户端对令牌的检查一直通过"只在宿主关连接之前成立。
- 平时客户端的读任务赶在宿主关连接之前读到应答；runner 负载高时，读任务被拖到宿主关连接之后，就只看到连接结束。这和 T196 第 2 处是同一件事，只是这次宿主在测试进程里。
- 按 5.8.1 这一次必须拒收（T196 的评审），所以修的是 GOLD-B 不依赖读到这个应答。

## 证据（临时改动，都没有提交）

- 在 CLI 客户端的 `read_frames` 里加了一个开关：开着时，套接字可读之后，先等到令牌查询失败（宿主那端已关闭），再去读。`stop()` 在发 `host/exit` 之前打开开关，拿到结果后关掉。单跑一份 `gold_b`（`--test-threads=1`，开关是全局的）。
- 修前：走 `stop()` 的 6 个场景全部失败，信息和 runner 上的一模一样：`Closed(Some(Refused(Refusal { kind: PeerExited, call: "getsockopt(LOCAL_PEERTOKEN)", code: 57 })))`。
- 修后（同一个开关）：12 个用例全部通过。
- 去掉开关后，修后的 `gold_b` 单跑一次，12 个全过。

## 修法

- `CloseReason::host_gone`（`cupper-cli` 的 `client.rs`）：`Eof`，或者 `Refused` 且 `PeerExited`。和原来 `run.rs`、IT-lifecycle 各自写的判定相同，现在只有这一处。
- GOLD-B 的 `stop()`：应答，或者 `Closed(Some(reason))` 且 `reason.host_gone()`，都算退出已受理；其他错误照旧失败；之后照旧等宿主以 `HostExit::Ok` 退出。
- 新用例 `unit_cli_client_a_reply_read_after_the_host_closed_is_refused`（`client.rs`）：假宿主收到 `host/exit` 后，用同步的 `send_with_fds` 写出应答、关掉自己那端，再等令牌查询失败（另一个线程上的 spawn 可能短暂持有这一端的副本）。这期间没有 await，客户端的读任务一定在关闭之后才读。断言请求以 `Closed(Some(Refused(PeerExited)))` 失败，`host_gone()` 为真。
  - 对照：把客户端临时改成 T196 第一版那样（`PeerExited` 时照样交付），这个用例失败，读到了 `Ok(Empty)`。所以它同时守住 5.8.1 不被放宽。
- 桌面 `restart.test.ts` 新增一例：`host/exit` 的调用失败（超时）时，`QuitGuard` 照样返回 `quit`。

## 依赖 `host/exit` 应答的地方（逐一核对）

| 位置 | 现状 | 处理 |
|---|---|---|
| `tests/gold_b.rs` 的 `stop()` | `unwrap()` 应答 | 本任务修 |
| `tests/lifecycle.rs` `it_lifecycle_host_exit_and_sigterm_exit_in_order` | T196 已接受 `Closed`，再断言关闭原因 | 判定换成 `CloseReason::host_gone` |
| `tests/lifecycle.rs` 升级用例 | 不发 `host/exit`；`host/shutdown`、`session/interrupt` 的应答 T196 已容忍 | 判定换成 `CloseReason::host_gone` |
| `gateway/dispatch/tests.rs` `it_lifecycle_host_exit_starts_once_its_answer_is_queued` | 进程内直接调 `Control`，不经套接字 | 不受影响 |
| CLI（`cupper` 命令） | 没有发 `host/exit` 的命令；连接以宿主退出结束时报"宿主已不在"（退出码 3） | 判定换成 `CloseReason::host_gone`，行为不变 |
| 桌面退出确认（`lifecycle.ts` 的 `QuitGuard`，`index.ts` 的 `quitHost`） | `host?.exit(true).catch(() => undefined)`，3 s 上限，读不到应答也照样退出，不提示用户 | 补用例 |
| 桌面重启宿主（`restart.ts`） | 发 SIGTERM，不发 `host/exit` | 不受影响 |
| testkit 原始连接 | 不比对令牌；没有调用 `host/exit` 的地方 | 不受影响 |
| E2E `E2E-quit-confirm`（`app.spec.ts`） | 经桌面退出确认，断言的是宿主进程树没了，不看应答 | 不受影响 |
| E2E `E2E-SEC-privileged`（`security.spec.ts`） | 窗口发的 `host/exit` 在主进程就被拒，到不了宿主 | 不受影响 |
| 其他宿主集成测试（compat、crash_concurrency、gateway_sync、perf 等） | 用 SIGTERM 收宿主 | 不受影响 |

## 桌面端的一个观察（范围外，没有改）

- 桌面的 `HostClient` 在连接断开时把命令重新排队，不会让调用失败；控制连接断开后会重连，`launcher.open` 在宿主不在时会再启动一个宿主。
- 所以退出确认读不到 `host/exit` 的应答时，要么等满 3 s 超时，要么重连到新宿主后重发同一个 `command_id`，新宿主按账本（`commands` 表）给出记下的结果。
- 无论应答读没读到，宿主关连接都会触发重连，退出过程中可能拉起一个新宿主，它会留到空闲退出。这一点只是读代码得出的，没有实测。

## 设计 5.2.13 建议补的一句

接在"`host/exit` 的回复与 `host/shutdown` 的先后"那一条后面：

> 进入发送队列不等于客户端读得到：客户端每次读取都比对宿主的令牌（5.8.1），宿主关掉连接或退出之后读到的字节一律拒收，所以客户端可能只看到连接结束（`Eof`，或者令牌查询以 `PeerExited` 失败）。调用方把应答和这两种结束都当作退出已受理，不报成失败（T213）。
