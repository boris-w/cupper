# T021 汇报

- 执行者：子 agent（Rust 系统工程师，macOS 安全）
- 分支：`feat/T021-sys-peer-sign`
- 完成日期：2026-10-19

## 做了什么

SP-4b 的 Security FFI 已经并进 `cupper-sys`，`PROC_PIDUNIQIDENTIFIERINFO` 也实测过了，`xtask ci security-macos14` 已接上。另外有两项：一项是实测推翻了设计里一条前提后补的原语（下文第 3 点），一项是编排者追加的 `redirect_stdio_to_null()`。

1. **对端身份**（`peer.rs`）
   - `peer_token`、`peer_pid`、`peer_uid` 分别读 `LOCAL_PEERTOKEN`、`LOCAL_PEERPID` 和 `getpeereid`。
   - `AuditToken` 提供 `pid`、`euid`、`pid_version`、`unique_id`；两个令牌用 `==` 比较，就是固定令牌的比对。
   - 所有失败都是 `Refusal { kind, call, code }`。`RefusalKind` 的前四种就是 5.8.1 的四类错误：对端已退出、已 exec、磁盘代码已变、其他。另有四种表示"校验能做，但对端不是要求的那个程序"：`Unsatisfied`、`NoRuntime`、`GetTaskAllow`、`Debugged`。
2. **代码签名**（`codesign.rs`）
   - `AuditToken::verify(Requirement)`：先 `SecCodeCopyGuestWithAttributes` 和 `SecCodeCheckValidity`，再查动态状态字（必须有 runtime，不能有 get-task-allow，不能被调试）。
   - `Requirement::new` 只接受 `&'static str`，所以要求只能写在编译期，本任务只提供校验接口。
   - csops：`cdhash()`、`code_status()` 用 `csops_audittoken`，只对令牌指向的那次 exec 作答。自检的两侧另有 `own_cdhash()`（运行中）和 `file_cdhash(path)`（磁盘上）。
   - 依赖新增 `core-foundation 0.10.1`，与设计 5.1.3 为这个 crate 列的一致。
3. **`recv_closing_fds(fd, buf)`**：带控制缓冲区读套接字，把随数据传来的 fd 全部关掉。原因见"关键取舍"第 1 条。
4. **`unique_ids(pid)` 和 `UniqueIds`**：读 `PROC_PIDUNIQIDENTIFIERINFO`，可行，不需要退回"pid 加启动时间"。
5. **`redirect_stdio_to_null()`**（编排者追加）：打开 `/dev/null`，`dup2` 到 0、1、2，出错时返回 `Result`。宿主里的调用留给 T037。
6. **`cargo xtask ci security-macos14`**：只跑 IT-sys-peer-current 和 UNIT-sys-uniqueid，打印系统版本，某个过滤词一个用例都没匹配到就判失败。另按 5.1.6 补了 `.github/workflows/security-macos14.yml`：macos-14、每晚跑、改动 cupper-sys 的 PR 跑、每晚失败就开 issue。和其他 workflow 一样，在有远端之前不会启用。
7. 接口一览、各场景的实测返回码、耗时和取舍都记在 `docs/tasks/T021-sys-peer-sign/notes.md`。

## 关键取舍

1. **实测推翻了设计 5.8.5 的前提："只用 read(2)，内核会丢弃 SCM_RIGHTS 传来的 fd"。**
   - 实测结果：不带控制缓冲区的 `read(2)` 和 `recv(2)`（std 和 tokio 最终调用的就是它）都会把传来的 fd 装进读方的 fd 表，不设 close-on-exec，也不通知读方。
   - 一条消息最多带 254 个 fd，255 个时 `sendmsg` 返回 `EINVAL`。
   - 网关没法自己调 `recvmsg`，因为 `unsafe` 只能出现在 `cupper-sys`，所以由本任务提供 `recv_closing_fds`。它用 4 KiB 控制缓冲区，按上面的上限不会被截断；万一被截断（`MSG_CTRUNC`）就返回错误。
   - 网关的用法：`stream.try_io(Interest::READABLE, || recv_closing_fds(&stream, buf))`。`closed_fds > 0` 时，建议断开并记审计。
   - 剩余窗口：从装入到关闭之间，这些 fd 没有 close-on-exec。宿主只用 `cupper_sys::spawn` 拉子进程（`CLOEXEC_DEFAULT`），不受影响。
2. **`p_puniqueid` 的语义**：记录的是最近一次 fork 或 exec 时的父进程。
   - 父进程退出、子进程被 launchd 收养后，它**不变**，所以设计里"没有竞态"的说法成立。
   - 唯一的例外：被收养**之后又 exec**，会变成 launchd 的 id（1）。
   - 两种情况下它都不会指向第三个进程。
   - 中途我一度误判为"收养时会改写"，原因是测试里父进程退出时，子进程还处在 fork 和 exec 之间。已修正，第一个提交的说明也在推送前 amend 改正了。
   - `AuditToken::unique_id` 先核对 pid version 与令牌一致再返回，不会读到 pid 复用后的别的进程。
3. **签名用例的对端是测试二进制自己的副本**：拷到临时目录，用 `codesign -s -` 做 ad-hoc 签名，按需加 identifier、runtime、get-task-allow，然后以被忽略的探针用例 `testutil::signed_peer_probe` 启动。不碰钥匙串，也不需要证书。系统自带的二进制只有 arm64e 切片，重签后不能运行，所以不用它们。
4. **动态状态字检查不可关闭**；状态字用 `csops_audittoken` 读，与令牌绑定。`Requirement` 只存字符串，每次校验时编译（几微秒）。这样不用对 `SecRequirementRef` 跨线程共享做没有文档依据的 `Send`/`Sync` 断言。
5. **测试锁 `fd_lock`**：有些用例会在进程里留下没有 CLOEXEC 的 fd，有些用例的子进程会继承这类 fd（`std::process`、`fork`、T020 的无 CLOEXEC 变异用例）。两类同时跑会互相干扰，所以用锁串起来。为此 T020 的 `unit_sys_spawn_mutation_without_cloexec_default_leaks_fds` 加了一行取锁。
6. **`redirect_stdio_to_null` 的用例**放在单独的集成测试二进制 `crates/cupper-sys/tests/stdio_null.rs` 里，并在子进程中执行。如果在测试进程里直接调用，libtest 之后打印的结果行会进 `/dev/null`，CI 的逐编号结果就看不到它。
   - 子进程是同一个测试二进制，按 rdev 比对 `/dev/fd/0`、`/dev/fd/1`、`/dev/fd/2` 与 `/dev/null`，并确认重定向之后它写的内容没有到达父进程的管道。
   - 去掉 fd 2 的变异会让用例失败。
   - **用例编号沿用 UNIT-sys-spawn**（用例名 `unit_sys_spawn_redirect_stdio_to_null`），没有新增 UNIT-sys-stdio-null。

## 验证方式

- **IT-sys-peer-current**，共 11 个用例，全部以 `it_sys_peer_current_` 开头，在 macOS 26.4 上验证：
  - 最后一次 I/O 的进程：`token_names_the_latest_io_process`。读和写都算；我们再做一次 I/O，令牌就变回我们的。
  - exec 后被拒：`exec_changes_the_token`，以及 `exited_and_execed_peers_are_refused` 的后半段。pid 相同、pid version 不同；旧令牌做 SecCode 校验返回 -67065，做 csops 返回 `ESRCH`。
  - fork 出的子进程共用 fd：`forked_child_sharing_the_fd_changes_the_token`，令牌变成子进程的 pid。
  - SCM_RIGHTS：前提不成立，改为两个用例。`plain_reads_install_passed_fds` 记录 fd 被装入、没有 CLOEXEC；`recv_closes_passed_fds` 验证 255 个 fd 全部被关掉、254 是上限、`WouldBlock` 和 EOF 原样返回。
  - csops：`csops_reads_the_running_cdhash`。csops 读到的 cdhash、`file_cdhash` 和 `codesign` 命令行打印的 CDHash 三者一致，状态字有 runtime；`own_cdhash` 等于自身文件的 cdhash。
  - 四类错误（括号里是触发它的用例）：
    - 已退出（`exited_peer_is_refused`、`exited_and_execed_peers_are_refused`）：`ENOTCONN`；最后一次 I/O 的进程已退出时 `EINVAL`；SecCode 返回 100003（`ESRCH`）；csops 返回 `ESRCH`。
    - 已 exec：同上，-67065。
    - 磁盘代码已变（`changed_code_on_disk_is_refused`）：删除文件时 100002，改名覆盖时 -67034。
    - 其他（`malformed_requirement_is_other`）：-67052。
  - 非错误的拒绝（`verify_checks_requirement_and_status`）：`Unsatisfied` -67050，包括 `anchor apple generic` 一定不满足 ad-hoc；另有 `NoRuntime`、`GetTaskAllow`。
- **UNIT-sys-uniqueid**，共 6 个用例：
  - 子进程记录的父进程 id 就是拉起方；
  - exec 前后 unique id 不变、pid version 变；
  - 父进程退出、被收养后 id 不变，直到再次 exec；
  - 能读 root 进程，读不到不存在的进程；
  - 僵尸进程读出 `None`；
  - 经套接字取得的对端 `unique_id` 等于"宿主"记录的父进程 id，其他进程的不等。
- **稳定性**：整个 crate 的测试连续跑 30 轮和 10 轮，全部通过。跑完检查过，没有残留进程，也没有残留的临时目录。
- **`unsafe`**：只在 `cupper-sys` 的 `src/` 里，crate 已开 `deny(clippy::undocumented_unsafe_blocks)`。集成测试和 xtask 里都没有 `unsafe`。
- **CI**：在干净的 worktree、同一提交 `6aa8ad0` 上跑了两次，摘要都已提交：
  - `cargo xtask ci pr --record docs/tasks/T021-sys-peer-sign/ci/` 通过：rust 71、ts 26、license 1；逐编号结果里 IT-sys-peer-current、UNIT-sys-uniqueid、UNIT-sys-spawn 都是 pass。
  - `cargo xtask ci security-macos14 --record ...` 通过：17 个用例，摘要的 `os` 是 "macOS 26.4"。
  - 摘要文件：`docs/tasks/T021-sys-peer-sign/ci/pr-6aa8ad0a85de.json`、`security-macos14-6aa8ad0a85de.json`。
- **其他检查**：`RUSTDOCFLAGS=-D warnings cargo doc -p cupper-sys` 无警告；分支 diff 和提交信息里没有本机路径、用户名，也没有链接。
- **耗时**：`verify` 预热后中位数约 84 µs，第一次调用 3–20 ms；`peer_token` 在 3 µs 以内。

## 提交列表

```
351782e docs(task): record the T021 pr and security-macos14 CI summaries
6aa8ad0 docs(task): add T021 notes on peer identity, passed fds and unique ids
effa27e feat(xtask): run the cupper-sys peer tests as security-macos14
d917210 feat(sys): point the process's stdio at /dev/null
9a31980 feat(sys): verify peers' code signatures and read cdhashes
399cafe feat(sys): identify socket peers by audit token and close passed fds
6a8a41d feat(sys): read unique process ids with PROC_PIDUNIQIDENTIFIERINFO
```

workflow 的 YAML 起初没过 prettier。我在推送前用 `git reset --soft` 把修正并回了 xtask 提交，所以每个提交都能单独通过。没有执行 merge、rebase 或 push。

## 遗留问题 / 范围外发现

1. **需要改设计（请编排者确认）**：5.8.5 "宿主自身"一行和 5.1.6 `security-macos14` 一行都写着"SCM_RIGHTS 被丢弃"，与实测不符。
   - 建议改成：宿主读套接字一律经 `recv_closing_fds`，收到 fd 就断开并记审计。
   - T030（网关读循环）和 T073（原生扩展读宿主套接字）必须用它，建议写进交接表。
2. **给 T030、T073 的提示**：
   - `LOCAL_PEERPID` 在最后一次 I/O 的进程退出后，仍返回它的旧 pid，而那个 pid 可能已被复用。`dev_key` 证明里的 `pid_c` 建议取自固定令牌的 `pid()`，不要另读一次。
   - 固定令牌只反映最后一次 I/O：两个共用 fd 的进程交错 I/O 时，后者会盖住前者。比对必须跟在每一次 read 之后，并依赖正版客户端从不共享 fd（CLOEXEC），这与 5.8.5 一致。
   - 判断"启动时父进程已退出"用 `process_info(自己).ppid == 1`；`p_puniqueid` 只用来比对对端。
3. **缺口登记**：T002 的 `tests/ids.toml` 还没合并，请编排者转交：
   - IT-sys-peer-macos14、IT-sys-peer-macos15：状态"缺口：本机替代不了，里程碑退出时由用户确认"；
   - 本机跑出的结果只记作 IT-sys-peer-current。
4. **没能测的**：
   - `CS_DEBUGGED` 需要真的附加调试器，代码路径与 get-task-allow 相同；
   - Developer ID 的正向校验属于 T105 和发布流水线；
   - macOS 14、15 本机替代不了。
   - 另外，ad-hoc 签名的进程在内核状态字里**没有** `CS_ADHOC`（实测 `0x22011311`），代码里没有依赖它。
5. **改动了根目录的文件**：
   - `Cargo.lock` 新增 core-foundation 0.10.1 和 core-foundation-sys 0.8.7；
   - 新增 `.github/workflows/security-macos14.yml`；
   - 根 `Cargo.toml`、`clippy.toml`、`deny.toml` 都没有改。
   - main 已经前进到 `11d0db9`。合并时如果 `Cargo.lock` 冲突，取 main 的版本，再让 cargo 重新解析即可。
6. **规模**：约 1,815 行，超过约 1,500 行的上限。其中测试和测试辅助约 880 行，产品代码约 760 行。超出主要来自两块：没有预料到的 SCM_RIGHTS 对策（`recv_closing_fds` 和它的两个用例），以及编排者追加的 stdio 重定向（约 95 行）。超出是在工作基本完成时才发现的，所以没有中途停下。
7. **T020 的用例改了一行**：`unit_sys_spawn_mutation_without_cloexec_default_leaks_fds` 加了 `fd_lock()`，原因见"关键取舍"第 5 条。

## 修复轮汇报（`fix/T021-flaky-peer-test`，2026-10-19）

两个偶发失败的用例都已修好，在 CPU 打满的情况下连跑 80 次，全部通过。

**原因**
- 推测只对了一半：确实是拉起子进程造成的，但子进程并不是继承了 fd。
- 任何 `posix_spawn`，包括带 `CLOEXEC_DEFAULT` 的 `cupper_sys::spawn`，都会在内核里先复制整张 fd 表，再关掉多余的；close-on-exec 的 fd 也一样。
- 所以只要别的用例正在拉子进程，刚关掉的管道写端就可能还被短暂持有，`writers_gone` 断言随之失败。T020 和 UNIT-sys-uniqueid 的用例拉子进程时都不取 `fd_lock`。
- 实测：一个线程连续拉子进程时，20,000 次检查里有 10–15 次写端仍开着；没有并发拉起时是 0 次。

**负载下又发现一个偶发失败，也是 T021 的**
- 失败的用例：`unit_sys_uniqueid_parent_id_outlives_the_launcher_until_an_exec`，在未改动的 main 上 50 次失败 1 次。
- 原因：`SIGUSR1` 可能在后台子 shell 执行 `trap` 之前就到达，信号按默认处置把它杀掉，它也就不会 exec。

**怎么修**
- 断言"fd 此刻是否还有人持有"的两个用例（`plain_reads_install_passed_fds`、`recv_closes_passed_fds`）改用新加的 `testutil::alone`，放到单独的测试进程里跑：
  - 做法：用 `--exact` 重新运行同一个测试二进制，只跑这一个用例，在那个进程里没有别的用例拉子进程。
  - 外层会核对子进程里恰好跑了 1 个用例且通过；测试路径写错或用例本身失败，外层都会失败，两种情况都实际验证过。
- 为什么不选"所有拉子进程的用例都取 `fd_lock`"：受影响的是所有拉子进程的用例，不只是用 `std::process` 的那些。现在和以后每个这样的用例都得记得取锁，漏一个就会重新出现偶发失败。
- 为什么不选单独的集成测试二进制：两个用例都要用 `unsafe`，而工作区禁止 crate 外的 `unsafe`，还得把辅助函数复制一份过去。
- 子 shell 设好 `trap` 后先输出一行 `ready`，测试读到这一行才发信号，结果是确定的。
- `notes.md` 里讲 `fd_lock` 的两处说明同步改了。

**验证**（18 个 `yes` 进程占满 18 个核）
- `cargo test -p cupper-sys --lib`：50 次，全部通过；
- `cargo test --workspace --all-features`：30 次，全部通过；
- 判定标准：输出里有 `test result: ok`，并且没有 FAILED 或编译错误。
- 结束后没有残留的负载进程和临时目录。
- `cargo xtask ci pr --record`（干净 worktree）通过：rust 255、ts 64、license 1，IT-sys-peer-current 和 UNIT-sys-uniqueid 都是 pass。摘要是 `docs/tasks/T021-sys-peer-sign/ci/pr-8f9ea44d9812.json`。

**提交**：`63d3bb0`（用例单独进程运行）、`be38187`（等 trap 就绪再发信号）、`8f9ea44`（notes）、`773243c`（CI 摘要）。

**给宿主的提示**：内核在拉起子进程时会短暂复制全部 fd，这一点宿主也会碰到。凡是"关掉 fd 后立刻判断对端是否已关闭"的逻辑都要考虑这个窗口，已写进 notes。

## 修复第 2 轮汇报（`fix/T021-fd-copy-audit`，2026-10-19）

- 执行者：子 agent（Rust 工程师，系统接口）
- 分支：`fix/T021-fd-copy-audit`
- 完成日期：2026-10-19

## 做了什么

本轮查清了：`cupper-sys` 里需要处理的同类断言，只剩 CI 失败的那一处，已修好；其他 crate 只排查，没有明显会失败的地方。验收标准都已满足。

**复现，确认根因**
- 办法：临时加一个被忽略、不提交的用例，把 `it_sys_peer_current_exited_peer_is_refused` 的用例体连跑 N 次，同时另开线程不停地用 `cupper_sys::spawn` 拉起 `/usr/bin/true`。
- 没有并发拉起时，2,000 次失败 0 次。
- 1 个拉起线程时，5,000 次失败 346 次；4 个拉起线程时，失败 1,572 次。
- 失败信息与 CI 上的完全一致：`(PeerExited, 22)`，期望 `(PeerExited, 57)`。

**整套复现**
- 在同一个测试进程里，用 4 个线程连续拉起 3 秒，与 `cupper-sys` 的其他用例同时运行，连跑 40 次。
- 失败 12 次，全部是这一个用例；其他用例 0 次。这和静态排查的结论一致。

**发现第二种 fd 副本**
- std 在 macOS 上建管道和 socketpair 时，是先建、后设 close-on-exec；`std::process` 拉起时又不带 `CLOEXEC_DEFAULT`。
- 所以一个线程正在建管道时，另一个线程用 `std::process` 拉起的子进程会把这个管道**一直带到它退出**，不只是拉起期间短暂持有。
- 实测：拉起 2,000 次，有 331 个子进程带走了管道（`io::pipe`），324 个带走了 socketpair（`UnixStream::pair`）。`File::open` 是原子地设 close-on-exec 的，0 个。
- 宿主只用 `cupper_sys::spawn` 拉子进程，它拉起的子进程不会这样继承，所以宿主不受影响；测试里用 `std::process` 的地方不少，这一点用于下面的排查。

**修复**
- 改动在 `crates/cupper-sys/src/peer.rs`：`drop(client)` 之后，先从 `server` 读到 EOF，再做原来的断言，断言本身一个字没改。
- `crates/cupper-sys/src/testutil.rs` 里 `fd_lock` 的文档同步改了：写明 std 建管道时的窗口，以及两种处理办法（等 EOF；或者在检查"副本是否开着"时用 `alone`）。

**排查清单**
- 完整清单写在 `docs/tasks/T021-sys-peer-sign/notes.md` 的"修复第 2 轮"一节，含位置、结论和处理方式。
- `cupper-sys`：只有上面这一处是"关掉 fd 后立即检查"。第 1 轮的两个用例在 `alone` 里没有问题，没动。其余各处关掉 fd 后都是阻塞等待（`wait()`、读到 EOF、`output()`），副本只会让它多等一会儿，不会失败。
- 其他 crate（只排查，未改动），没有明显会失败的：
  - `cupper-host` `tests/startup.rs` 的 `unit_startup_payload_bad_payload_exits_1`，空输入那一项（第 83 行）：风险低。它要宿主在 2 秒的真实超时内读到 EOF；能带走 stdin 写端的，只有同一二进制里并发拉起、随即退出的那几个宿主。
  - `cupper-fake-agent` `tests/common/mod.rs` 的 `Fake::exit`（第 231 行）：风险低，只会变慢。并行拉起的另一个 agent 可能带走这个 agent 的 stderr 写端，读到 EOF 要等那个 agent 被它的用例杀掉。
  - `xtask` `ci/lock.rs` 的 `unit_ci_lock_released_when_holder_is_killed`：断言的形状相同，但锁文件只在持锁的子进程里打开，那个进程不拉起任何进程，所以不会有副本。无风险。
  - 其余（内存 duplex、`ManualClock` 下的读、SQLite 记录锁、各处 `output()`）：无风险。

## 关键取舍

- **为什么先读到 EOF，而不是用 `alone`。** 这一段验证的是"另一端没人持有时，报 `ENOTCONN` 和对端已退出"。"没人持有"是前提，不是被检查的对象；读到 EOF 正好说明前提已经成立。
  - 这样用例仍和其他用例并发运行，也不用多起一个测试进程。
  - 第 1 轮那两个用例检查的恰恰是"副本此刻是否还开着"，只能用 `alone`，本轮不动。
- **EOF 能当依据。** 按 XNU 的 `unp_disconnect`，最后一份引用关闭时，内核在同一把锁下清掉两端的连接，并标记读端结束。所以读返回 0 之后，`LOCAL_PEERTOKEN` 和 `LOCAL_PEERPID` 都返回 `ENOTCONN`。修复后同样的负载 0 次失败，与这个推理一致。
- **不会一直卡住。** 这个用例持有 `fd_lock`。此时 `cupper-sys` 里用 `std::process` 拉起长寿子进程的用例都在等锁，所以都不存在。唯一不持锁的 `std::process` 拉起是 `alone` 的外层，它的子进程只跑一个很短的用例就退出。
- **变异检查。** 临时把 `ENOTCONN` 归为 `Other`，用例失败，报 `(Other, 57)`。说明改动后这一段仍在检查 `ENOTCONN` 的归类。

## 验证方式

| 验收项 | 结果 |
|---|---|
| 复现 | 办法见上。修复前：1 个拉起线程时 5,000 次失败 346 次，4 个时失败 1,572 次，整套 40 次失败 12 次。修复后同样的负载：都是 0 次 |
| CPU 负载下的稳定性（18 个 `yes` 占满 18 个核，`TMPDIR` 指向专用空目录） | `cargo test -p cupper-sys --lib` 50/50 通过；`cargo test --workspace --all-features` 30/30 通过，每次 232 个用例。判定标准：退出码为 0，输出里有 `test result: ok`，没有 `FAILED` 和编译错误 |
| 理由和排查清单写进 `notes.md` | 已写（第 2 个提交） |
| `cargo xtask ci pr --record`（干净 worktree） | 通过：rust 444、ts 64、license 1，IT-sys-peer-current 为 pass。摘要是 `docs/tasks/T021-sys-peer-sign/ci/pr-2fe9402fa9d5.json` |
| 不留负载进程和临时目录 | `yes` 进程已全部退出；专用 `TMPDIR` 为空，已删除。复现工具和小实验程序都没有提交 |

- 另外跑过 `cargo fmt --check`、`cupper-sys` 的 clippy（`-D warnings`），都通过。
- 没有新增 `unsafe`，没有个人信息，产品代码没有改动。
- 规模：代码 +11/−5 行（2 个文件），`notes.md` +105 行，合计约 120 行，在 200 行的上限以内。

## 提交列表

```
0ab68ee docs(task): record the T021 fd-copy audit pr CI summary
2fe9402 docs(task): record the T021 fd-copy audit and its reproduction
62f1f8b test(sys): wait for end of file before checking a closed peer
```

## 遗留问题 / 范围外发现

1. **产品代码里一处很小的分类竞态，没有改。** 位置：`peer_token`，`crates/cupper-sys/src/peer.rs` 第 134–136 行。
   - 现象：另一端的最后一份副本，恰好在 `LOCAL_PEERTOKEN`（返回 `EINVAL`）和随后的 `LOCAL_PEERPID`（返回 `ENOTCONN`）之间关掉时，拒绝原因会记成 `Other`，而不是 `PeerExited`。负载下 5,000 次出现 3 次。
   - 影响：连接照样被拒绝，只是审计种类和语义码不对。
   - 一种改法：`peer_pid(socket).map_or_else(|r| r.kind == RefusalKind::PeerExited, |pid| unique_ids(pid).is_none())`。属于产品代码，按任务书不在本轮范围，请编排者决定是否另开任务。
2. **建议补进 `handoff-notes.md`。** 对象是 T030、T037、T043 及以后的 actor 任务。
   - 关掉 fd 之后要判断对端是否已关闭，应以读到 EOF 为信号，不要关闭后立刻查 `getsockopt`。
   - 第二种副本是 `std::process` 和 `tokio::process` 才有的问题，再次说明"拉子进程一律经 `cupper_sys::spawn`"这条约束是必要的。
3. **其他 crate 的两处低风险项**（宿主启动测试的空输入、`Fake::exit` 变慢），我判断不需要处理，由编排者决定。
   - 另外，`unit_fake_scripts_probe` 现在只要求继承的 fd 以 `0,1,2` 开头。不要把它改成严格等于 `0,1,2`：带走的 fd 会接在后面，严格比较就会偶发失败。
4. **一个不是本轮留下的临时目录。** 用户临时目录里有一个 `xtask-test-<pid>-4-lock-killed`，创建于今天 12:33，早于本任务，对应的进程已经不在。我没有删它，请编排者确认它的来历。

### 追加：`peer_token` 的分类竞态（同一分支）

T021 已就绪。

`peer_token` 的分类竞态已在 `fix/T021-fd-copy-audit` 上修好。worktree 干净，所有改动都已提交，旧的 CI 摘要都保留了。

- **修复**：`3adc4bd`。`EINVAL` 之后回查的 `peer_pid` 如果也报 `PeerExited`（即 `ENOTCONN`），这次就记为 `PeerExited`。注释写明了两次 `getsockopt` 之间的窗口。
- **实测**：复现工具改成直接测这个窗口，4 个拉起线程。`(Other, 22)` 修复前 10,000 次里 40 次（两轮 5,000，各 21、19 次），修复后 0 次。
  - 比上一轮 5,000 次里 3 次多，`notes.md` 里如实写了：原因没有深究。
  - 复现工具没有提交。
- **为什么没加用例**：写在 `notes.md` 里。这个窗口在用例里没法确定地触发；两条确定的路径（`EINVAL` 和 `ENOTCONN` 都记为 `PeerExited`）已经由 `exited_peer_is_refused` 覆盖。
- **notes**：`95706fb`，"修复第 2 轮"一节新增"`peer_token` 的分类竞态"。
- **CI**：在干净 worktree 里重跑 `cargo xtask ci pr --record`，通过：rust 444、ts 64、license 1，IT-sys-peer-current 为 pass。新摘要是 `docs/tasks/T021-sys-peer-sign/ci/pr-95706fbfc0fa.json`，提交 `39a088a`。
- **自检**：fmt、`cupper-sys` 的 clippy（`-D warnings`）和测试都通过；没有新增 `unsafe`，提交内容里没有个人信息。

```
39a088a docs(task): record the T021 pr CI summary after the peer_token fix
95706fb docs(task): record the T021 peer_token classification race
3adc4bd fix(sys): count a peer that closes during the token query as exited
0ab68ee docs(task): record the T021 fd-copy audit pr CI summary
2fe9402 docs(task): record the T021 fd-copy audit and its reproduction
62f1f8b test(sys): wait for end of file before checking a closed peer
```

等你合并后通知我，再开始 T074 修复第 1 轮。
