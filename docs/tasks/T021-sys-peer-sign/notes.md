# T021 工作笔记

- 执行者：子 agent（Rust 系统工程师，macOS 安全）
- 实测环境：macOS 26.4（25E246），arm64。以下"实测"都指这台机器，不代表 macOS 14、15。

## 接口一览（`cupper-sys`）

| 接口 | 作用 | 给谁用 |
|---|---|---|
| `peer_token(fd)`、`peer_pid(fd)`、`peer_uid(fd)` | `LOCAL_PEERTOKEN`、`LOCAL_PEERPID`、`getpeereid` | T030、T073 |
| `AuditToken`：`pid()`、`euid()`、`pid_version()`、`unique_id()` | 审计令牌；`==` 就是固定令牌的比对 | T030、T073 |
| `AuditToken::verify(Requirement)` | SecCode 校验，再查动态状态字（必须有 `CS_RUNTIME`，不能有 `CS_GET_TASK_ALLOW`、`CS_DEBUGGED`） | T030、T045、T073 |
| `AuditToken::cdhash()`、`code_status()` | `csops_audittoken`，只对令牌所指的那次 exec 作答 | T045（自用构建比对宿主 cdhash）、T038 |
| `own_cdhash()`、`file_cdhash(path)` | 自检的两侧：运行中的 cdhash 与磁盘上的 cdhash | T037、T045（开发构建弱校验）、T075 |
| `Requirement::new(&'static str)`、`validate()` | 签名要求只能来自编译期常量 | T030、T045、T073 |
| `Refusal { kind, call, code }`、`RefusalKind` | 拒绝原因：5.8.1 的四类错误，加 `Unsatisfied`、`NoRuntime`、`GetTaskAllow`、`Debugged` | 审计日志、UI 语义码 |
| `recv_closing_fds(fd, buf)` | 带控制缓冲区读套接字，把随数据传来的 fd 全部关掉 | T030（网关读循环）、T073 |
| `unique_ids(pid)` → `UniqueIds` | `PROC_PIDUNIQIDENTIFIERINFO` | T030（`dev_key` 只认拉起方） |
| `redirect_stdio_to_null()` | 把 0、1、2 指向 `/dev/null`（编排者追加） | T037 |

## 实测结论

### `LOCAL_PEERTOKEN` 与 `LOCAL_PEERPID`

1. 令牌是"最后一次在对端做 I/O 的进程"的当前映像，读和写都算。另一个进程在同一端写一次，令牌就换成它的；我们再写一次，又换回来。
2. exec 之后 pid 不变，pid version 变，所以令牌不相等；用旧令牌做 SecCode 校验返回 `-67065`，做 csops 返回 `ESRCH`。
3. fork 出的子进程共用 fd、写一次，令牌就是子进程的 pid。父进程随后再做一次 I/O，令牌又变回父进程的。所以比对必须跟在每一次 read 之后，不能隔几次读才比一次；两个共用 fd 的进程交错 I/O 时，后者会盖住前者（见遗留问题）。
4. 最后一次 I/O 的进程已退出、但别的进程还握着对端时，`LOCAL_PEERTOKEN` 返回 `EINVAL`，`LOCAL_PEERPID` 却照常返回那个已退出进程的 pid（pid 可能已被复用）。`peer_token` 遇到 `EINVAL` 时再查一次 `LOCAL_PEERPID`，进程不在就记为"对端已退出"。
5. 对端完全关闭：`ENOTCONN`。不是套接字：`ENOTSOCK`，记为"其他"。

### `SCM_RIGHTS`：内核不会丢弃，而是装进读方的 fd 表

- 设计 5.8.5 写的是"只用 `read(2)` 读、内核会丢弃传来的 fd"。实测不是这样：不带控制缓冲区的 `read(2)` 和 `recv(2)`（std 的 `UnixStream::read`，也就是 tokio 最终调用的）都会把传来的 fd **装进读方的 fd 表**，不设 close-on-exec，也不告诉读方。用管道写端验证过：读完后写端仍然开着，在本进程的 fd 表里能找到这份副本，往里写的数据能从管道读出来。
- 一条消息最多带 254 个 fd，255 个时 `sendmsg` 返回 `EINVAL`。内核内部把每个 fd 存成 8 字节指针，放在 2 KiB 的簇里（12 + 254 × 8 = 2044）。
- 对策是 `recv_closing_fds`：用 4 KiB 控制缓冲区调 `recvmsg`，把 `SCM_RIGHTS` 里的 fd 逐个 `close`，返回关掉了几个。控制数据被截断（`MSG_CTRUNC`）时返回错误，因为截掉的 fd 会留在表里。按上面的上限，4 KiB 不会被截断。
- 从 `recvmsg` 装入到 `close` 之间，这些 fd 没有 close-on-exec。这段时间里别的线程如果用不带 `POSIX_SPAWN_CLOEXEC_DEFAULT` 的方式拉起子进程，子进程会继承它们。宿主只用 `cupper_sys::spawn` 拉子进程（5.8.5），不受影响。
- 另外，任何 `posix_spawn`（包括带 `CLOEXEC_DEFAULT` 的 `cupper_sys::spawn`）在内核里都会先复制父进程的整张 fd 表，再关掉多余的，close-on-exec 的 fd 也一样。所以别的线程正在拉子进程时，刚关掉的 fd 可能还被短暂持有。2026-10-19 的复测：一个线程连续拉子进程时，"刚关掉的管道写端"约 0.06% 的概率仍然开着；没有并发拉起时 20,000 次中 0 次。宿主只在关闭后不久做这种判断时才需要注意。
- 还有一种更持久的副本：std 在 macOS 上建管道和 socketpair 不是原子地设 close-on-exec，`std::process` 拉起的子进程可能把别的线程正在建的管道一直带到它退出。实测和影响见文末"修复第 2 轮"。
- 网关的用法：tokio 的 `UnixStream::try_io(Interest::READABLE, || cupper_sys::recv_closing_fds(&stream, buf))`。`closed_fds > 0` 说明对端不在说我们的协议，建议断开并记审计。

### `PROC_PIDUNIQIDENTIFIERINFO`（UNIT-sys-uniqueid）

- flavor 17，结构 56 字节：`p_uuid[16]`、`p_uniqueid`（u64）、`p_puniqueid`（u64）、`p_idversion`（i32）、4 字节保留、两个 u64 保留。公开 SDK 里没有，按 XNU 的 `proc_info_private.h` 自己声明。字段名与计划评审时的记忆一致。
- `p_idversion` 等于审计令牌第 8 个字（pid version）。`AuditToken::unique_id` 先核对两者一致再返回，所以不会读到 pid 复用后的别的进程。
- `p_uniqueid` 在 exec 前后不变，`p_idversion` 变。
- `p_puniqueid` 是"最近一次 fork 或 exec 时的父进程"：
  - 父进程退出、launchd 收养之后，**不变**，仍是已退出的拉起方的 id。所以宿主在任何时候读自己的 `p_puniqueid`，都是拉起它的那个 UI 进程，不存在竞态，设计 5.8.1 的说法成立。
  - 唯一的例外：被收养**之后又 exec**，会变成 launchd 的 id（1）。宿主由 `posix_spawn` 拉起，exec 在拉起的那一步就完成了，之后不再 exec。
  - 两种情况下它都不会指向第三个进程，unique id 也不会复用，所以只有真正的拉起方能匹配。
  - 过程中一度以为"收养会改写成 launchd"：那是测试里子进程在 fork 与 exec 之间父进程就退出了，exec 时重新取了父进程。等子进程 exec 完再让父进程退出，40 次里 0 次改写；不等，40 次里 40 次改写。
- 对 root 进程（pid 1）也能读；launchd 的 `p_puniqueid` 是 0。僵尸进程读不到（返回 `None`），所以"已退出"的判断包括僵尸。
- 不需要退回"pid 加启动时间"。
- 给 T030 的提示：判断"启动时父进程已退出"仍用 `process_info(自己).ppid == 1`；`p_puniqueid` 只用来比对对端。

### SecCode 与 csops

| 场景 | 返回 | 归类 |
|---|---|---|
| 对端满足要求、开了 hardened runtime | 通过 | — |
| identifier 不符；`anchor apple generic`（ad-hoc 永远不满足） | `SecCodeCheckValidity` -67050 | `Unsatisfied` |
| 签名时没开 runtime | 状态字 `0x22000201` | `NoRuntime` |
| 带 get-task-allow | 状态字含 `0x4` | `GetTaskAllow` |
| 磁盘上的可执行文件被删 | `SecCodeCopyGuestWithAttributes` 100002（`ENOENT`） | `CodeChanged` |
| 另一个构建改名覆盖原路径 | `SecCodeCheckValidity` -67034 | `CodeChanged` |
| 对端已退出 | `SecCodeCopyGuestWithAttributes` 100003（`ESRCH`）；csops `ESRCH` | `PeerExited` |
| 对端已 exec | `SecCodeCopyGuestWithAttributes` -67065；csops `ESRCH` | `PeerExeced` |
| 要求串写错 | `SecRequirementCreateWithString` -67052 | `Other` |
| 不是代码的文件（`/etc/hosts`）求 cdhash | 签名信息里没有 cdhash，按 -67062 返回 | `Error::Security` |

- csops 的 `ESRCH` 和 SecCode 的 -67065、100003 都表示"令牌所指的那次 exec 已经不在了"。再用 `unique_ids(pid)` 区分：进程不在（含僵尸）是"已退出"，进程在但 pid version 变了是"已 exec"。pid 已被复用时也会归为"已 exec"，反正都是拒绝。
- 用 ad-hoc 加 runtime 签名的探针，内核状态字是 `0x22011311`：有 `CS_VALID`、`CS_HARD`、`CS_KILL`、`CS_RUNTIME`，**没有 `CS_ADHOC`**。所以用例不断言 `CS_ADHOC`，代码也没有定义它。
- csops 读到的 cdhash、`file_cdhash`（SecStaticCode）和 `codesign --display --verbose=3` 打印的 `CDHash` 三者一致。测试二进制本身是链接器签名的，`own_cdhash()` 与 `file_cdhash(当前可执行文件)` 也一致。
- 文件被改名覆盖后，`token.cdhash()` 仍是运行中的旧值，`file_cdhash` 是新值，正是 SP-4b 提的"宿主自检"信号。
- 耗时（debug 和 release 相近）：`verify` 预热后中位数约 84 µs，p90 约 105 µs，第一次调用 3–20 ms（加载 Security 框架）。`peer_token` 在 3 µs 以内。比 SP-4b 的 0.9 ms 还快，可能是因为 SP-4b 每次另读了签名信息字典。

## 取舍

1. **`Requirement` 只存 `&'static str`，每次校验时编译。** 这样宿主可以写成 `const`，"要求只来自编译期"由类型保证；也不用对 `SecRequirementRef` 跨线程共享做没有文档依据的 `Send`/`Sync` 断言。编译一次只要几微秒。`validate()` 让宿主在启动时或测试里发现写错的要求。
2. **动态状态字的检查不可关闭。** 设计里所有用签名要求的地方都要求 hardened runtime、不带 get-task-allow、没被调试。状态字用 `csops_audittoken(CS_OPS_STATUS)` 读，不用 `kSecCodeInfoStatus`：它同样绑定令牌，少一层 CF 字典解析。
3. **`Refusal` 用一个扁平的枚举。** 网关拿到 `Err` 一律按 other 处理，只需按 `kind` 记审计和给语义码。前四类就是设计的四类错误，后四类是"校验成功，但不是要求的那个程序"。
4. **用 `core-foundation` 0.10.1**（设计 5.1.3 为本 crate 列的依赖），Security 部分自己声明，约 40 行 `extern`。Security 框架的 CF 对象用 `CFType` 包装，释放交给 Drop。
5. **签名用例的对端是测试二进制自己的副本。** 复制到临时目录，用 `codesign --force --sign -` 按需加 identifier、runtime、get-task-allow 做 ad-hoc 签名，再以 `--exact testutil::signed_peer_probe --ignored` 启动，环境变量打开探针模式，它把套接字上收到的字节原样回写。不碰钥匙串，也不需要证书。系统自带的二进制只有 arm64e 切片，重签后不能运行，所以不用它们。
6. **`fd_lock` 和 `alone`**：
   - `fd_lock` 串起那些子进程会长期持有本进程 fd 的用例（`fork`、`std::process`、T020 的无 CLOEXEC 变异用例），为此 T020 的 `unit_sys_spawn_mutation_without_cloexec_default_leaks_fds` 也持锁。
   - 断言"某个 fd 此刻是否还有人持有"的两个用例（`plain_reads_install_passed_fds`、`recv_closes_passed_fds`）用 `alone` 在单独的测试进程里运行：同一进程里其他用例的任何拉起都会短暂复制 fd（见上），只靠锁要让所有拉起子进程的用例都持锁，漏一个就重新出现偶发失败。最初版本只靠 `fd_lock`，在 main 的全量运行里偶发失败过一次，修复见 `fix/T021-flaky-peer-test`。
   - 把"另一端已经没人持有"当作前提、而不是检查对象的用例（`exited_peer_is_refused`），先从自己这一端读到 EOF 再断言，仍和其他用例并发运行。见文末"修复第 2 轮"。
7. **`redirect_stdio_to_null` 的用例放在单独的测试二进制里，并在子进程里执行。** 如果在测试进程里直接调用，libtest 之后打印的结果行会进 `/dev/null`，CI 的逐编号结果就看不到这个用例。子进程是同一个测试二进制，带环境变量启动，按 rdev 比对 `/dev/fd/0..2` 与 `/dev/null`，并确认重定向之后它写的内容没有到达父进程的管道、读 stdin 读到的是空。去掉 fd 2 的变异会让用例失败。用例编号沿用 UNIT-sys-spawn（名字以 `unit_sys_spawn_` 开头）。

## 没能测的

- `CS_DEBUGGED`：需要真的附加调试器，没做。代码路径与 get-task-allow 相同。
- Developer ID 证书的正向校验：按 brief 属于发布流水线和 Q35（T105）。用例里的 `anchor apple generic …` 只验证了 ad-hoc 一定不满足。
- macOS 14、15：本机替代不了，见汇报。

## 修复第 2 轮：并发拉起造成的 fd 副本（`fix/T021-fd-copy-audit`，2026-10-19）

### 复现

- 办法：在 `peer::tests` 里临时加一个被忽略的用例（不提交）。它把 `it_sys_peer_current_exited_peer_is_refused` 的用例体连跑 N 次，同时另开 0、1 或 4 个线程，不停地用 `cupper_sys::spawn` 拉起 `/usr/bin/true` 并等它退出；每次失败用 `catch_unwind` 收下断言信息。
- 修复前：

| 并发拉起的线程 | 运行次数 | 失败 | 失败信息 |
|---|---|---|---|
| 0 | 2,000 | 0 | — |
| 1 | 5,000 | 346 | 全是 `(PeerExited, 22)`，期望 `(PeerExited, 57)` |
| 4 | 5,000 | 1,572 | 1,569 次 `(PeerExited, 22)`，3 次 `(Other, 22)` |

- 失败信息与 CI 上的完全一致：`drop(client)` 之后，另一端还被正在拉起的子进程持有，`LOCAL_PEERTOKEN` 仍按"最后做 I/O 的进程已退出"返回 `EINVAL`。
- `(Other, 22)` 是同一个窗口的另一种落点：查 `LOCAL_PEERTOKEN` 时副本还在（`EINVAL`），紧接着 `peer_token` 回查 `LOCAL_PEERPID` 时副本已经关了（`ENOTCONN`），于是没能归为"对端已退出"。这是产品代码的分类问题，见汇报的遗留问题。
- 比第 1 轮测到的 0.06% 高得多：这里每次都是 drop 之后立即判断，拉起线程也一刻不停。
- 整套复现：再临时加一个用例，在同一个测试进程里用 4 个线程连续拉起 3 秒，与 `cupper-sys` 的其他用例同时运行。测试二进制连跑 40 次，失败 12 次，全是这个用例，其他用例 0 次。这从实测上印证了下面的排查结论。

### 第二种副本：std 建管道和 socketpair 不是原子地设 close-on-exec

- std 在 macOS 上建管道（`io::pipe`、`Stdio::piped`）和 socketpair（`UnixStream::pair`）时，先建、再用 `fcntl` 设 close-on-exec；`std::process` 拉起时又不带 `POSIX_SPAWN_CLOEXEC_DEFAULT`。所以某个线程正在建管道时，另一个线程用 `std::process` 拉起的子进程会把它**一直**带着，直到子进程退出，而不只是拉起期间短暂持有。
- 实测（一个不提交的小程序）：一个线程循环建了就关，主线程用 `std::process` 拉起自己 2,000 次，子进程列出 0–2 以外的 fd：

| 另一个线程在做 | 带着多余 fd 的子进程 |
|---|---|
| 什么都不做 | 0 |
| `File::open`（`O_CLOEXEC`，原子） | 0 |
| `io::pipe()` | 331 |
| `UnixStream::pair()` | 324 |

- `cupper_sys::spawn` 带 `CLOEXEC_DEFAULT`，它拉起的子进程不会这样继承。但它建管道用的也是 `io::pipe()`，所以别的线程用 `std::process` 拉起的子进程可能带走它的管道端。宿主只用 `cupper_sys::spawn`（5.8.5），不受影响；测试里用 `std::process` 的地方不少，见排查清单。
- 对等待的影响：等 EOF 的一方要一直等到那个子进程退出。只要那个子进程的退出不反过来依赖这一方，就只是变慢，不会卡死。

### 修法

- 只改了 `it_sys_peer_current_exited_peer_is_refused`：`drop(client)` 之后先从 `server` 读到 EOF，再做原来的断言。
- 为什么不放进 `alone`：这一段要验证的是"另一端没人持有时，`peer_token` 和 `peer_pid` 都报对端已退出，错误码 `ENOTCONN`"。"没人持有"是前提，不是被检查的对象。读到 EOF 说明前提已经成立，后面的断言一个字没改。`alone` 也能修，但要多起一个测试进程，用例也不再和其他用例并发运行。第 1 轮那两个用例检查的恰恰是"副本此刻是否还开着"，只能用 `alone`，本轮不动。
- 为什么 EOF 能说明前提成立：按 XNU 的 `unp_disconnect`，最后一份引用关闭时，内核在持有两端锁的情况下清掉两端的 `unp_conn`，并给两端设 `SS_CANTRCVMORE`。读返回 0 说明已经设了，此后 `LOCAL_PEERTOKEN`、`LOCAL_PEERPID` 都看到 `unp_conn == NULL`，返回 `ENOTCONN`。修复后同样的负载 0 次失败，与这个推理一致。
- 会不会一直读不到 EOF：这个用例持有 `fd_lock`。在 `cupper-sys` 里，用 `std::process` 拉起长寿子进程的用例都持锁，所以此时它们都不存在；`cupper_sys::spawn` 的子进程不会继承任何 fd。唯一不持锁的 `std::process` 拉起是 `alone` 的外层，那个子进程只跑一个很短的用例就退出。所以最多多等几十毫秒。
- 变异检查：临时把 `socket_refusal` 里的 `ENOTCONN` 归为 `Other`，用例失败（`(Other, 57)`）。说明这一段仍在检查 `ENOTCONN` 的归类。
- `fd_lock` 的文档同步改了：写明 std 建管道时的窗口，以及两种处理办法（等 EOF，或者在检查"副本是否开着"时用 `alone`）。
- 修复后，同样的负载：

| 负载 | 运行次数 | 失败 |
|---|---|---|
| 用例体，0 个拉起线程 | 2,000 | 0 |
| 用例体，1 个拉起线程 | 5,000 | 0（修复前 346） |
| 用例体，4 个拉起线程 | 5,000 | 0（修复前 1,572） |
| 整套 `cupper-sys` lib，同时 4 个线程拉起 3 秒 | 40 | 0（修复前 12） |

### 稳定性检查（CPU 负载）

- 做法同第 1 轮：18 个 `yes` 进程占满 18 个核，在提交 `62f1f8b` 上连跑。`TMPDIR` 指向一个专用的空目录，结束后检查它是否为空。
- 判定标准：退出码为 0，输出里有 `test result: ok`，并且没有 `FAILED` 或编译错误。

| 命令 | 次数 | 通过 | 备注 |
|---|---|---|---|
| `cargo test -p cupper-sys --lib --locked` | 50 | 50 | 每次 38 个用例，1.4–5.1 秒 |
| `cargo test --workspace --all-features --locked` | 30 | 30 | 每次 232 个用例 |

- `it_sys_peer_current_exited_peer_is_refused` 在这 80 次里全部通过。
- 结束后 `yes` 进程已全部退出，专用 `TMPDIR` 为空，已删除；复现工具没有提交。

### 排查清单

判断标准：关掉或丢掉一个 fd 之后，是**立即**检查"对端已关闭"（有副本时会失败），还是**阻塞等待**对端读到 EOF 或子进程退出（副本只会让它多等一会儿，不会失败）。

**`cupper-sys`（全部用例）**

| 位置 | 关掉 fd 之后做什么 | 结论 | 处理 |
|---|---|---|---|
| `peer.rs` `it_sys_peer_current_exited_peer_is_refused`（619–630 行） | 立即要 `ENOTCONN` | 会失败，就是这次 CI 的失败 | 先读到 EOF 再断言 |
| 同一用例前半段（612–617 行） | 我们自己还握着另一端，要 `EINVAL` → `PeerExited` | 副本不做 I/O，不改 `last_pid`，不受影响 | 不动 |
| `peer.rs` `plain_reads_install_passed_fds`、`recv_closes_passed_fds`（530、560 行） | `writers_gone`、非阻塞 `recv` 立即要 EOF | 第 1 轮已用 `alone`；单独的进程里没有别的拉起，上面整套复现中 40 次全过 | 不动 |
| `peer.rs` `token_names_the_latest_io_process`（332–333、346–348 行） | 关 sh 的 stdin 或套接字后 `wait()` | 阻塞等待 | 不动 |
| `peer.rs` `exec_changes_the_token`（378–379）、`forked_child_sharing_the_fd_changes_the_token`（425–426）、`unit_sys_uniqueid_peer_token_names_the_launcher`（649–650） | 同上 | 阻塞等待；另用复现工具各跑 1,000 次（4 个拉起线程），0 失败 | 不动 |
| `codesign.rs` `exited_and_execed_peers_are_refused`（526–527、556–557 行） | drop 后 `wait()`，再查令牌 | 阻塞等待；之后只看进程还在不在 | 不动 |
| `codesign.rs` 其余四个用例 | 不检查关闭 | 不受影响 | 不动 |
| `proc.rs` `exec_keeps_the_id_and_changes_the_pid_version`（382–383）、`parent_id_outlives_the_launcher_until_an_exec`（413–414） | 关 stdin 后 `wait()` | 阻塞等待 | 不动 |
| `proc.rs` 其余用例 | 等进程退出，或不涉及 fd | 不受影响 | 不动 |
| `spawn.rs` `child_inherits_only_fds_0_to_2`、`mutation_without_cloexec_default_leaks_fds`（467–468、488–489） | 检查子进程的 fd 表；关 stdin 后 `wait()` | 子进程的 fd 表不受父进程里别的拉起影响；等待是阻塞的 | 不动 |
| `spawn.rs` `child_gets_exactly_its_env`、`null_stdin_and_working_directory`（522、563） | 读 stdout 到 EOF | 阻塞等待 | 不动 |
| `group.rs` 五个用例 | 只读第一行、发信号、等进程退出 | 不涉及 fd 关闭 | 不动 |
| `tests/stdio_null.rs` | `wait_with_output` | 单独的测试二进制；阻塞等待 | 不动 |
| `testutil.rs` 的 `alone`、`signed_probe`、`codesign_cdhash`、`start_probe` | `output()`、读一个字节 | 阻塞等待 | 不动 |

**其他 crate（只排查，未改动）**

| 位置 | 关掉 fd 之后做什么 | 风险 |
|---|---|---|
| `cupper-host` `driver/transport.rs` `unit_transport_memory_close_reaches_the_other_end`（107 行） | 立即要 EOF 和 `BrokenPipe` | 无：内存里的 duplex，不是 fd |
| `cupper-host` `driver/transport.rs` `stdio_transport_reads_and_writes_pipes`（126 行） | 真管道，drop 后读到 EOF | 无：阻塞等待。同一测试二进制里 `failpoint` 的用例会用 `std::process` 拉起测试二进制，最多多等那个子进程的寿命 |
| `cupper-host` `startup/payload.rs` `reads_one_json_line`（323、328 行）、`times_out_after_two_seconds_of_clock_time`（350 行） | drop 写端后读到 EOF | 无：阻塞等待；`ManualClock` 不走，没有超时 |
| `cupper-host` `failpoint.rs` `unit_failpoint_armed_failpoint_exits_on_its_nth_hit` | `output()` | 无：阻塞等待 |
| `cupper-host` `tests/startup.rs` `unit_startup_payload_bad_payload_exits_1` 的空输入（83 行） | drop 宿主的 stdin，要宿主读到 EOF、报"stdin ended"，而不是 2 秒超时 | 低：这是限时等待。能带走 stdin 写端的只有同一二进制里并发拉起的几个宿主（`--build-info`、未知参数），它们拉起后立即退出，离 2 秒很远 |
| `cupper-host` 存储用例（SQLite） | 靠 POSIX 记录锁 | 无：记录锁属于进程，子进程里的 fd 副本既不持有也不释放它 |
| `cupper-fake-agent` `tests/common/mod.rs` `Fake::exit`（231 行） | agent 退出后读 stderr 到 EOF | 低，只会变慢：用例并行地用 `std::process` 拉起 agent，一个 agent 的 stderr 写端可能被同时拉起的另一个 agent 带走（第二种副本），读要等到那个 agent 在它的用例结束时被杀掉 |
| `cupper-fake-agent` `tests/scripts.rs` `unit_fake_scripts_probe`（242 行） | 断言 agent 继承的 fd 以 `0,1,2` 开头 | 无：带走的 fd 编号都大于 2，只会接在后面。改成严格等于 `0,1,2` 反而会偶发失败 |
| `cupper-fake-agent` `tests/scripts.rs` `unit_fake_scripts_spawn_child_and_exit`（253 行） | 杀掉 `sleep` 后 `exit()` | 无：阻塞等待 |
| `xtask` `ci/lock.rs` `unit_ci_lock_released_when_holder_is_killed`（101、144 行） | 持锁进程被杀后，立即要拿到锁（`!waited`） | 无。形状相同：`flock` 跟着打开的文件走，任何副本都会让锁留着。但锁文件只在持锁的子进程里打开，那个进程只跑这一个用例、不拉起任何进程，所以没有副本 |
| `xtask` `ci/lock.rs` `unit_ci_lock_second_holder_waits_until_first_releases`（57 行） | 第一个释放后，第二个阻塞在 `lock()` | 无：阻塞等待 |
| `xtask` 其他（git、`facts`、`ctx`、`clippy_disallowed_methods`） | `output()`、读文件 | 无 |
