# T213 汇报

- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T213-host-exit-reply`
- 完成日期：2026-11-02
- PR：https://github.com/boris-w/cupper/pull/29 。CI 运行 https://github.com/boris-w/cupper/actions/runs/41379612062 ，提交 4e753d2，8 个检查第 1 次 attempt 就全部通过。

**状态**：完成。修复前 runner 上 600 次失败 15 次，修复后 600 次全部通过。

## 做了什么

**根因**
- GOLD-B 的宿主跑在测试进程里。客户端是 CLI 库的 `Client`，和宿主共用同一个多线程运行时。
- T205 之后，`host/exit` 的应答一定在连接关闭之前进入发送队列；之后宿主发 `host/shutdown`，再关闭连接。
- 客户端每读一次就比对一次令牌。宿主关掉自己那一端之后，`getsockopt(LOCAL_PEERTOKEN)` 返回 `ENOTCONN`（57），归为 `PeerExited`，这一次读到的应答按 5.8.1 拒收。
  - 宿主进程还活着也会这样：`ENOTCONN` 只说明对端已经关闭。T205 笔记里"进程内宿主的令牌检查一直通过"这句，只在宿主关连接之前成立。
- runner 负载高时，客户端的读任务被拖到宿主关连接之后。和 T196 第 2 处是同一件事。

**本机复现，修前修后对照**（临时开关，没有提交）
- 做法：在客户端 `read_frames` 里临时加了一个开关。开着时，客户端先等到令牌查询失败（宿主那端已关），再去读。
- 修前：走 `stop()` 的 6 个场景全部失败，报错和 CI 上一字不差。
- 修后：同样开着开关，12 个用例全部通过。

**改动**
1. `cupper-cli`：新增 `CloseReason::host_gone`，判定是 `Eof`，或 `Refused` 且 `PeerExited`。
   - 原来 `run.rs` 和 IT-lifecycle 各写了一份同样的判定，现在都改用这一处。
   - CLI 的行为不变。
2. 新用例 `unit_cli_client_a_reply_read_after_the_host_closed_is_refused`（确定性的，不靠时序）：
   - 假宿主收到 `host/exit` 后，用同步的 `send_with_fds` 写出应答、关掉自己那端，再等到令牌查询失败。这期间没有 await，所以读任务一定在关闭之后才读。
   - 断言请求以 `Closed(Some(Refused(PeerExited)))` 失败，且 `host_gone()` 为真。
   - 把客户端临时改回 T196 第一版（`PeerExited` 时照样交付读到的数据），这个用例失败，读到的是 `Ok(Empty)`。所以它同时守着 5.8.1 不被放宽。
3. GOLD-B 的 `stop()`：
   - 应答，或者 `Closed(Some(r))` 且 `r.host_gone()`，都算退出已受理；
   - 其他错误照旧失败；
   - 之后照旧等宿主以 `HostExit::Ok` 退出。
4. IT-lifecycle 的两处改用 `host_gone()`。
5. 桌面 `restart.test.ts` 补一例：`host/exit` 的调用失败或超时时，`QuitGuard` 照样退出。

**依赖 `host/exit` 应答的地方**（逐处核对，详表在 `notes.md`）

| 位置 | 情况 | 处理 |
|---|---|---|
| GOLD-B 的 `stop()` | 要求读到应答 | 本任务修 |
| IT-lifecycle 的退出用例 | T196 已经处理 | 判定改用 `host_gone()` |
| IT-lifecycle 的升级用例 | T196 已经处理 | 判定改用 `host_gone()` |
| CLI | 没有发 `host/exit` 的命令；连接以宿主退出结束时报"宿主已不在"（退出码 3） | 不变 |
| 桌面退出确认 | `.catch(() => undefined)`，3 s 上限，不提示用户 | 补了用例 |
| 桌面重启宿主 | 发的是 SIGTERM | 不受影响 |
| testkit 原始连接 | 不比对令牌 | 不受影响 |
| E2E 退出确认 | 只看宿主进程树是否消失，不看应答 | 不受影响 |
| E2E 的 SEC-privileged | 请求在主进程就被拒 | 不受影响 |
| 其余宿主集成测试 | 用 SIGTERM | 不受影响 |
| `dispatch/tests.rs` 的 T205 用例 | 不经套接字 | 不受影响 |

**规模**：`cargo xtask size` 为 +89 −26，停止线 120 以内。
- 开工前估计约 30；开工后改为把判定收进 CLI 库，并补了确定性用例，估计改为约 65。
- 这个修订是事后才补写进 `notes.md` 的。

## 关键取舍

- **不放宽 5.8.1**：客户端读取的代码一行没改。修的只是调用方不再依赖读到应答。
- **判定放进 CLI 库**：不在测试的 `common` 里再放一份。CLI、IT-lifecycle、GOLD-B 用的是同一条规则，以后改规则只有一处。
- **`stop()` 照任务书也接受 `Eof`**：
  - 修了 T205 之后，进程内宿主"`Eof` 而没有应答"只会是应答没入队的回归。
  - 这个回归由 T205 的 `it_lifecycle_host_exit_starts_once_its_answer_is_queued` 守着，不靠 GOLD-B。
- **编排者问的 `Closed(None)`**：
  - **什么时候出现**：只有读任务没记下原因就结束时才会出现，也就是读任务 panic 了。
    - `read_loop` 先在锁内写 `closed = Some(..)`，再清空等待表，所以正常结束时请求拿到的一定是 `Some`。
    - 读任务 panic 时，等待表里的发送端还在共享的 `Routes` 里，`request` 会一直挂住，不会得到 `Closed(None)`。
    - 只有握手里 `next()` 返回 `None` 那一处，会在读任务 panic 后报 `Closed(None)`。
  - **stop() 不接受它是有意的**：`stop()` 调的是 `request`，正常情况下碰不到 `Closed(None)`；真碰到了就是客户端的缺陷，不是宿主退出，应当失败。
  - **`run.rs` 接受它**：这是 T049 起就有的保守映射，主要对应握手那一处，本任务没改它的行为。

## 验证方式

**runner 上的连跑**
- 参数：`--features test-harness --test gold_b`，filter `gold_b_`（整份 gold_b），200 次 × 3 份并行，macos-15。

| | 运行 | 提交 | 结果 |
|---|---|---|---|
| 修前 | https://github.com/boris-w/cupper/actions/runs/41378840292 | main d025ad4 | 600 次里失败 15 次（2.5%），共 16 个用例失败 |
| 修后 | https://github.com/boris-w/cupper/actions/runs/41379609751 | 4e753d2 | 600 次全部 ok |

- 修前的失败都在 `gold_b.rs:394:20`，报 `Closed(Some(Refused(PeerExited)))`。按场景分：claude_cancel 8、claude_approval 4、fake_approval 2、fake_normal_turn 1、claude_normal_turn 1。
- 修后的结果是下载 `results.txt` 核对的。

**本机检查**（都只跑一份）
- 通过：fmt；clippy 三组，`-D warnings` 无告警：
  - cupper-cli 全特性；
  - cupper-cli 无特性；
  - cupper-host test-harness，all-targets。
- 通过：cupper-cli 测试（带 test-harness 和不带）；gold_b 12 个；IT-lifecycle 的退出用例和升级用例；桌面 `restart.test.ts`，以及 prettier、eslint、tsc。

**验收标准**
- [x] 测试和产品里依赖 `host/exit` 应答的地方都已列出并处理。
- [x] 修复后 runner 上 `gold_b` 600 次全部通过（run 41379609751）。
- [x] PR #29 的 8 个检查全部通过（run 41379612062）。

本机测试没有留下进程，工作区干净。

## 提交列表

```
4e753d2 docs(task): T213 notes on the cause, the evidence and every host/exit caller
2febda3 test(desktop): quitting goes on when the host/exit answer never comes
fa6bee8 test(host): GOLD-B's stop takes a refused host/exit reply as the exit
31020da test(host): IT-lifecycle takes the host gone from the CLI's own test
c5f8021 test(cli): a reply read after the host closed is refused
b5e7699 refactor(cli): one test of a connection that ended as the host's exit
```

## 遗留问题 / 范围外发现

1. **设计 5.2.13 建议补的一句**，接在"`host/exit` 的回复与 `host/shutdown` 的先后"那一条后面：
   > 进入发送队列不等于客户端读得到：客户端每次读取都比对宿主的令牌（5.8.1），宿主关掉连接或退出之后读到的字节一律拒收，所以客户端可能只看到连接结束（`Eof`，或者令牌查询以 `PeerExited` 失败）。调用方把应答和这两种结束都当作退出已受理，不报成失败（T213）。
2. **桌面退出时可能再拉起一个宿主**（只是读代码得出，没有实测）：
   - 桌面的 `HostClient` 在连接断开时把命令重新排队；控制连接断开后会重连；宿主不在时，`launcher.open` 会再启动一个。
   - 所以确认退出之后，宿主一关连接（不论应答读没读到）就会触发重连，可能在应用退出前拉起一个新宿主，它会一直留到空闲退出。
   - 读不到应答时还有两种结果：要么等满 3 s 超时，要么重连之后重发同一个 `command_id`，新宿主按账本给出记下的结果。
   - E2E 退出确认只看原宿主的进程树，看不出这件事。建议另开任务核实。
3. **修复后的连跑结果没有写进 `notes.md`**，只写在本汇报里，避免为此再推送一次。
