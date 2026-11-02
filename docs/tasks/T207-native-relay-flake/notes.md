# T207 工作笔记

- 执行者：子 agent（Rust 工程师）

## 开工前的规模估计

| 项 | 估计 |
|---|---|
| 诊断探针：用例照原样判定，另把每次的计时写进 `test-repeat/` 目录，供 runner 上修前的连跑；连跑后整个 revert，净增 0 | 0（过程中约 +120） |
| 修复（根因在测试）：`relay.rs` 的测试 | 10–30 |
| 修复（根因在中继）：`relay.rs` 本体，加一条稳定复现的用例 | 60–120 |
| **合计** | **10–120** |

在停止线 150 之内。

## 失败现场

- PR #22，run 40661950733 第 1 次运行，rust 作业的 `test-native-release` 步骤：`cargo test -p cupper-desktop-native --locked --no-fail-fast`，不带特性。
- 二进制刚链接好（`Compiling cupper-desktop-native`，16.68 s），这是它的第一次运行；步骤共 26.7 s，等于编译加 10.01 s，所以启动二进制本身没有慢。
- 7 个用例，完成的先后：`launch_without`、`launch_writes`、`fds`、`closing`、`token`、`check`，最后是 `forwards ... FAILED`。`finished in 10.01s`。
- panic：`relay.rs:182:39`（`drain` 里的 `read_to_end`），`Os { code: 35, kind: WouldBlock }`，也就是读满了 10 s 的 `SO_RCVTIMEO`。
- 同一作业第 2 次运行，整个二进制 `finished in 0.02s`。
- 所以失败的那一次，用例在二进制开始后约 10 ms 内就走到了 `drop(host)` 和 `drain`，之后 10 s 里主进程一端没有读到 EOF。
- 翻了近期失败的 19 次 CI（含 nightly）的 `--log-failed`，没有别的中继失败。

## 排除过的解释

- **别的进程拿着宿主一端的 fd**（std 的 `UnixStream::pair` 在 macOS 上先 `socketpair` 再设 close-on-exec，中间有窗口；token 用例用 std 的 `Command` 拉起 `/bin/sh`，不带 `POSIX_SPAWN_CLOEXEC_DEFAULT`）：说不通。所有拉起的子进程都在各自用例结束前被收掉，而失败现场里其余 6 个用例都先于转发用例报完，持有者在 10 s 到期之前就已经放手了。
- **中继里有别的路径没有关**：`read_host` 的每条出路都调 `close`，`close` 只是两次 `shutdown`，不会阻塞。

## 探针 be0e7cf：runner 上修前的连跑

探针照 main 判定（10 s 读不到 EOF 就失败），另外：记下 `read_host` 出循环和 `close` 返回的时刻；超时后再读一次（上限 120 s），看 EOF 什么时候来；旁边开一个每 1 ms 醒一次的看门狗线程，记下超过 20 ms 的间隔。参数和 `test-native-release` 一样：不带特性，不加过滤，7 个用例一起跑。

| | A：run 41002979245，`times=3000`、`parallel=1` | B：run 41002987191，`times=1500`、`parallel=4` |
|---|---|---|
| 结果 | 3000 次里失败 19 次（0.63%） | 6000 次里失败 36 次（0.60%） |
| 失败的用例 | 转发 18 次，`closing` 1 次（`drain(&host)`） | 转发 35 次，`closing` 1 次 |
| 负载（1 分钟） | 6.4–17.8，中位数 8.3 | 5.2–15.8，中位数 8.2 |

转发用例失败的 53 次，探针的数一模一样：

- `drop(host)` 之后 0.005–0.046 ms，`read_host` 读到 EOF 出了循环；0.009–0.085 ms，`close` 已经把两端都 `shutdown` 了。
- 主进程一端阻塞的 `read_to_end` 仍然等满 10 s（10000.2–10005.2 ms），报 `WouldBlock`。
- 紧接着再读一次，0.004–0.111 ms 就读到 EOF：EOF 早就在那里，只是阻塞在读里的线程没有被叫醒。
- 看门狗在等 EOF 的这 10 s 里，最长的间隔 88 ms，累计不到 0.32 s；所有 9000 次里最长的间隔也只有 88 ms。没有整机或整个进程的停顿。

通过的 8947 次：`close` 在 `drop` 之后 0.005–6.4 ms，主进程读到 EOF 在 0.009–17.4 ms（中位数 0.05–0.07 ms）。

结论：中继没有关得慢；是主进程一端阻塞的读丢了一次唤醒。负载高低对失败率几乎没有影响。

## 探针 fd9a638：变体与"进入等待"的竞争（runner）

C2：run 41005984276（`times=10`、`parallel=1`，10 次全过）；D2：run 41005992943（`times=5`、`parallel=4`）。D2 有 15 次在第二个实验里 panic（`shutdown` 报 `ENOTCONN`）：负载下主线程晚于 100 ms 才去关，读的一方已经超时、丢掉了 socket，是探针自己 100 ms 等待的问题；这些次的第二个实验没有写出数据，第一个实验的数据都在。

**实验一：中继的形状，换着结束我们这端的方式和主进程等待的方式**（每种 5000 次一组，等 100 ms）

| 变体 | C2 | D2 |
|---|---|---|
| 真正的 `Relay`，测试的步骤，阻塞读 | 50000 次漏 495（0.99%） | 100000 次漏 1067（1.07%） |
| 迷你中继，照 `Relay::close` 关，阻塞读 | 0 / 50000 | 0 / 100000 |
| 迷你中继，只 `shutdown(Write)`，阻塞读 | 漏 60（0.12%） | 漏 86（0.09%） |
| 迷你中继，其余各种（poll、kqueue、关一次、关 fd、先关再 close） | 0 | 0 |

- 真正的中继复现得很稳，约 1%，和 A、B 量到的 0.6% 同一量级（A、B 里每次只有一个用例走到这一步）。
- 迷你中继照同样的方式关却一次都没有漏：它的时序和真正的中继不一样，赶不上那个窗口。所以实验一里 poll、kqueue 的 0 次不能算数，要看实验二。

**实验二：读的一方开始等待，对端在 0–30 µs 后（每档 100 ns）结束连接**（每种 20000 次一组，等 100 ms）

| 对端 | 读的方式 | C2 | D2（5 组） |
|---|---|---|---|
| `shutdown(Both)` | 阻塞 `recv_closing_fds` | 200000 次漏 30（0.015%） | 100000 次漏 44（0.044%） |
| `shutdown(Both)` | 先 `poll(POLLIN)` 再读 | 0 / 200000 | 0 / 100000 |
| `shutdown(Both)` | 先 kqueue `EVFILT_READ` 再读 | 0 / 200000 | 0 / 100000 |
| 关 fd | 三种都是 | 0 / 200000 | 0 / 100000（另有 12–26 次是负载下对端 100 ms 内还没关，不是漏） |

- 漏只出在"阻塞读 + 对端 shutdown"。同样的竞争条件下，先 poll 或 kqueue 再读的从没漏过；对端整个关掉 fd 的也从没漏过。
- 每次"漏"之后再读一次，EOF 都已经在那里。

## 探针 33249f2：中继自己的读，宿主照网关的方式结束连接（runner）

E：run 41006408074（`times=5`、`parallel=1`，5 次全过）；E4：run 41006415634（`times=3`、`parallel=4`，12 次全过）。

用真正的 `Relay`。宿主写一帧，0–40 µs 后（每档 200 ns）`shutdown(Write)`，也就是网关的 `OwnedWriteHalf` 被丢掉时做的事；5 ms 后再关 fd（读的那一半被丢掉）。主进程照 libuv 的方式等：kqueue 加非阻塞读。量的是从宿主第一次结束到主进程读到 EOF 的时间；对照组是宿主直接关 fd。每组 4000 次。

| | 先半关闭、5 ms 后关 fd | 直接关 fd |
|---|---|---|
| E（20000 次） | 中位数 26–59 µs，p99 0.2–0.4 ms；**≥4 ms 的 8 次（0.04%）**，最长 13.9 ms | 中位数 28–61 µs，p99 0.08–0.17 ms；≥4 ms 的 0 次，最长 1.4 ms |
| E4（48000 次） | 中位数 72–94 µs，p99 0.5–0.8 ms；**≥4 ms 的 70 次（0.15%）**，最长 45 ms | 中位数 22–43 µs；≥4 ms 的 2 次（负载），最长 8.5 ms |

- 关 fd 在半关闭之后 5 ms，所以 ≥4 ms 的那一档就是"中继的阻塞读漏了半关闭，等到宿主关 fd 才交出 EOF"。E4 里最长 45 ms，是 5 ms 的 `sleep` 在负载下被拉长了。
- 直接关 fd 的一组在不加负载时没有一次到 4 ms，可见这一档不是调度的抖动。
- 结论：中继自己的两个阻塞读偶尔会漏掉对端的半关闭，EOF 晚到对端关 fd 的那一刻。宿主的网关在 conn 任务返回时就关 fd，所以总会交出来，只是晚了。

## 成因：xnu 源码里的这条路径

以下是从源码推出来的，版本是 xnu-11417.140.69（macOS 15）和 xnu-12377.121.6（macOS 26），两个版本这几段相同；行号按 11417 / 12377。上面的数据和它对得上，但内核里的时序没有直接观测。

1. `bsd/kern/uipc_socket.c` 的 `soreceive`（3330 / 3202）：缓冲区里没有数据时，先查 `SS_CANTRCVMORE`（3572 / 3444），有就交出 EOF；没有就 `sbunlock`，然后对要求 `PR_WANTRCVD` 的协议调 `pru_rcvd`（3629 / 3501）。紧挨着的注释承认"这可能放掉 socket 的锁再拿回来"。之后只用 `so_should_wait`（3259 / 3131）决定是否进 `sbwait`（3634 / 3506），而它只看缓冲区里有没有 mbuf，**不再看 `SS_CANTRCVMORE`**。
2. `bsd/kern/uipc_usrreq.c` 的 `uipc_rcvd`（425 / 427）是 Unix 流套接字的 `pru_rcvd`：它调 `unp_get_locks_in_order`（231 / 233）按地址顺序拿两端的锁。本端地址比对端高时，先 `socket_unlock(so)`（241 / 243），再拿对端的锁，再拿回本端的锁。
3. 对端的 `shutdown` 走 `soshutdownlock_final`（`uipc_socket.c` 4498 / 4370）→ `uipc_shutdown`（`uipc_usrreq.c` 774 / 776）→ `unp_shutdown`（2222 / 2224），后者拿锁之后调 `socantrcvmore(so2)`（2231 / 2233），给读的一方置上 `SS_CANTRCVMORE`，再 `sorwakeup`。
4. `bsd/kern/uipc_socket2.c`（12377）：`sorwakeup`（2671）只在 `sb_notify`（2285）为真时唤醒，即 `sb_waiters > 0`，或者缓冲区上挂着 `SB_SEL`、`SB_KNOTE` 等标志；`sb_waiters` 要到 `sbwait` 里（580）才加一。
5. 所以：读的一方在第 2 步放掉了自己的锁、正在等对端的锁（对端的 `shutdown` 正拿着它），这时对端置上 `SS_CANTRCVMORE`、发现没人在等，就不唤醒；读的一方拿回锁后不再看这个标志，进 `sbwait` 睡下，直到来数据或者超时。这正是 A、B 里看到的：EOF 早就在，阻塞的读却睡满了 10 s。测试里读的一方等的正是对端 `shutdown` 拿着的那把锁，两边自然撞在一起，所以真正的中继漏得最多（约 1%）。
6. 为什么 poll、kqueue 不受影响：`bsd/kern/sys_socket.c` 的 `soo_select` 在 socket 锁里先置 `SB_SEL`、再查 `soreadable`（含 `SS_CANTRCVMORE`），不可读才 `selrecord`；kqueue 的 `EVFILT_READ` 挂上时置 `SB_KNOTE`。两种都让第 4 步的 `sb_notify` 为真，唤醒不会丢；之后的非阻塞读或可读之后的读，在第 1 步就看到 EOF，不会走到 `pru_rcvd`。
7. 为什么关 fd 不受影响：关 fd 走 `unp_disconnect`（1603 / 1605）。第 2 步放锁之前，读的一方给自己的 pcb 置上 `UNP_DONTDISCONNECT`；`unp_disconnect` 见到它就睡下等（1667 / 1669），等读的一方拿齐两把锁之后才断开，那时读的一方已经在 `sbwait` 里等着了。

## 修法

- **测试**：`drain` 像主进程（libuv）和宿主（tokio）那样，先等可读、再读，仍以 10 s 为防护；不再用阻塞读去等 EOF。等数据的 `read_exact` 不受这个竞争影响（数据进缓冲区之后 `so_should_wait` 看得见），照旧。
- **中继**：`read_host`、`write_host` 每次 `recv_closing_fds` 之前先等可读，宿主的半关闭当场就能交出去。别的行为不变。
- **`cupper-sys::wait_readable`**：`poll(POLLIN)` 的安全封装，EINTR 时重试；`unsafe` 只能在 `cupper-sys` 里。
- **回归用例**：宿主写一帧就 `shutdown(Write)`，fd 一直开着，直到主进程读到 EOF；中继漏掉半关闭的话，主进程就等满 10 s 失败。竞争在内核里，用例是概率性的：照 E、E4 的失败率（每次 0.04–0.15%），20000 次里修复前几乎必然至少漏一次。
