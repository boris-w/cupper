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
