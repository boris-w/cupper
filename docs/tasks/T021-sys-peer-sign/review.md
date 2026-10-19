# T021 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，已合并
- 核对：
  - 在任务 worktree 里重跑 `cargo xtask ci pr`（rust 71 项）和 `cargo xtask ci security-macos14`（17 项），都通过；
  - `unsafe` 只出现在 `cupper-sys`，并带 `SAFETY:` 注释；
  - 没有个人信息。
- **安全发现：设计的一条前提被实测推翻。** 设计 5.8.5、5.1.6 和 ADR-05 都写着"只用 read(2) 时，内核会丢弃 SCM_RIGHTS 传来的 fd"。实测结果是：不带控制缓冲区的 `read`/`recv` 照样会把 fd 装进读方，并且不设 CLOEXEC。执行者在 `cupper-sys` 里补了 `recv_closing_fds`：用控制缓冲区读，读到的 fd 全部关闭。编排者裁定：
  - 宿主、原生扩展、CLI 读 cupper 套接字时，一律经 `recv_closing_fds`；
  - 一旦收到 fd，就断开连接并记审计；
  - 设计和 ADR 中的相关写法都已改正。
- 规模：约 1,815 行，超出上限。超出的部分来自两处：这条没有预料到的安全对策，以及编排者追加的 `redirect_stdio_to_null`。接受。
- 接受的取舍：
  - `Requirement` 只接受 `&'static str`，保证签名要求只能在编译期写定；
  - 动态状态字的检查不能关闭；
  - 用测试锁 `fd_lock` 串起会互相干扰的用例（T020 的一个用例加了一行取锁）；
  - stdio 重定向的用例放在子进程里跑。
- 转交事项已写进 `docs/tasks/handoff-notes.md`：
  - T030、T045、T073 使用 `recv_closing_fds`；
  - `pid_c` 取自固定令牌；
  - 父进程已退出的判断方法；
  - 缺口编号交 T002 登记。

## 合并后发现的问题（2026-10-19，编排者）

- 合并 T017 后，在 main 上跑 `cargo xtask ci pr`，`it_sys_peer_current_plain_reads_install_passed_fds` 失败了一次，断言为 `writers_gone(&pipe)`。单独连跑 `cupper-sys` 的测试 12 次，都没有复现。
- 推测原因：这个用例故意让 fd 不带 CLOEXEC 装进本进程；同一进程里如果有其他用例在不取 `fd_lock` 的情况下用 `std::process` 拉子进程，子进程会继承这些 fd，导致写端没有全部关闭。
- 处理：由 T021 的执行者在修复分支 `fix/T021-flaky-peer-test` 上修复。正在运行的其他任务碰到这个失败时，重跑即可，不要去改 `cupper-sys`。

## 第 2 轮：偶发失败修复（2026-10-19，编排者）

- 结论：通过，合并 `fix/T021-flaky-peer-test`。
- 核对：
  - CI 摘要 `ci/pr-8f9ea44d9812.json`：在干净的 worktree 里跑，rust 255、ts 64、license 1，全部通过；
  - 没有新增 `unsafe`，没有个人信息；
  - 只改了测试代码和 `testutil`，产品路径没有改动。
- 原因确认：之前推测"子进程继承了 fd"，不对。真正的原因是任何 `posix_spawn`（包括带 `CLOEXEC_DEFAULT` 的）都会在内核里先复制整张 fd 表，再关掉多余的。执行者给出了实测数据：并发拉起子进程时，约 0.06% 的概率写端仍然开着；没有并发拉起时是 0。
- 修法：
  - 断言"fd 此刻是否还有人持有"的两个用例，用 `testutil::alone` 放到单独的测试进程里跑；外层核对子进程里恰好跑了 1 个用例并且通过。
  - 不采用"所有拉子进程的用例都取 `fd_lock`"：漏掉一个，偶发失败就会回来。接受执行者的这个判断。
- 顺带修了第二个偶发失败 `unit_sys_uniqueid_parent_id_outlives_the_launcher_until_an_exec`：`SIGUSR1` 可能在 `trap` 设好之前就到达。改为子 shell 先输出 `ready`，测试读到这一行才发信号。
- 已知的小代价：`alone` 的外层用 `std::process` 拉子进程，不取 `fd_lock`。拉起期间，如果同进程里有 `fork` 用例，管道写端可能被短暂复制，最多让外层多等到那个 fork 子进程退出，不影响结果。接受。
- 转交事项：内核在拉起时会短暂复制全部 fd。宿主里凡是"关掉 fd 后立刻判断对端是否已关闭"的逻辑，都要考虑这个窗口。已写进 `handoff-notes.md`，交给 T037（进程监护）和 T043 及以后的 actor 任务。

