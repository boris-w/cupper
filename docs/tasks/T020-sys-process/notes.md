# T020 过程记录

- 执行者：子 agent（Rust 系统工程师，macOS）
- 日期：2026-10-19
- 环境：macOS 26.4，arm64，Rust 1.96.0；`libc` 0.2.191、`thiserror` 2.0.21

## 对外接口（`cupper-sys`）

| 项 | 接口 | 说明 |
|---|---|---|
| 拉起 | `spawn(&Command) -> Result<Child>`；`Command::new(绝对路径)`、`arg(s)`、`env(Env)`、`current_dir`、`stdin/stdout/stderr(Stdio::Null \| Pipe)`、`new_session()` | `POSIX_SPAWN_CLOEXEC_DEFAULT`、`SETSIGDEF`（全部信号）、`SETSIGMASK`（空集）；默认 `SETPGROUP`（自成进程组），`new_session()` 改为 `SETSID`。两种情况子进程都是组长，pgid = pid |
| 子进程 | `Child { stdin, stdout, stderr: Option<OwnedFd> }`、`pid()`、`start()`、`wait()`、`try_wait()`、`group()` | 拉起后立即读出启动时间；读不到时杀掉并回收子进程、返回错误，不交出无法记录的进程 |
| 环境变量 | `Env::new()`、`Env::from_allowed(来源, &["PATH", "LC_*"])`、`set`、`get`、`iter` | 子进程只拿到 `Env` 里的变量，不继承宿主的环境；`CUPPER_*` 无论白名单怎么写都拒绝（ADR-05）。具体名单由调用方定（5.6.6，T054） |
| 进程信息 | `process_info(pid)`、`parent_chain(pid)`、`descendants(pid)`；`ProcessInfo { pid, ppid, pgid, sid, uid, start, name }`、`StartTime`（微秒） | 只返回活着的进程，僵尸进程返回 `None` |
| 进程组 | `ProcessGroup { pgid, start }`、`with_descendants()`、`members()`、`is_alive()`、`signal(Signal::Term \| Kill) -> Result<bool>` | 见下文"清理" |
| 系统 | `boot_session_uuid()`、`user_temp_dir()`、`current_uid()` | `kern.bootsessionuuid`；`confstr(_CS_DARWIN_USER_TEMP_DIR)`；`getuid` |

## 试错与取舍

1. **进程信息不用 `PROC_PIDTBSDINFO`。** SP-4a、SP-4b 都用它，但实测它只能读同一用户的进程：父进程链走到 `login`（root）就断了，到不了 launchd，而 5.8.4 的确认框要据"链是否断在 launchd"判断来源。`PROC_PIDT_SHORTBSDINFO` 能读任何用户的进程，但没有启动时间；启动时间改从 `sysctl(KERN_PROC_PID)` 取：`struct kinfo_proc` 的第一个字段 `extern_proc` 以一个联合体开头，其中一支就是 `p_starttime`。只读这一个字段，结构体长度 648 字节用常量断言固定，与 `PROC_PIDTBSDINFO` 的结果逐微秒一致（实测本进程）。
2. **僵尸进程。** 实测短记录和完整记录对僵尸都读不到，`getsid` 返回 -1；`sysctl` 仍能读到启动时间；`proc_listchildpids`、`proc_listpgrppids` 会列出僵尸。所以 `process_info` 对僵尸返回 `None`，列组成员、列后代时自然过滤掉僵尸；`spawn` 用 `sysctl` 读启动时间，子进程即使瞬间退出也能读到。
3. **清理的分工。** 设计要求 SIGTERM 和 SIGKILL 之间"限时等待"，而定时只能经 `Clock`（clippy `disallowed-methods` 对本 crate 同样生效）。所以 `cupper-sys` 只提供原语：记录（`with_descendants`）、核对（`members`）、发信号（`signal`）；等待和升级由宿主的 Supervisor 用 `Clock` 编排（T037、T055）。测试里的轮询用 `#[expect(clippy::disallowed_methods)]` 标明。
4. **进程组记录与重用。** 记录是"组号加启动时间"（设计 5.6.8、5.2.13 的 `adapters` 表）：组长活着时取组长的启动时间，组长已退出时取最早成员的。判定"还是记录的那个组"：组长若在，启动时间必须相同；所有成员的启动时间都不早于记录。依据是内核在旧组还有成员时不会把这个号分给新进程。沿用 SP-4a 的规则，剩余风险写在 `members` 的文档里：组号被重用、新组长又退出、而新组还有成员时认不出来。
5. **不发信号的对象。** 组号 0（等于发给自己的组）、1（launchd）和宿主自己的组一律拒绝；`with_descendants` 也跳过它们。用纯函数测试这个检查，不去真的对自己的组发信号。
6. **T003 的情形怎么测。** 不运行真实 CLI。用 `sh -c 'set -m; sleep 30 & ...'` 模拟：`set -m` 让后台作业自成进程组，相当于 Claude Code 给工具命令新建的组。测试先记录整棵树的组，再只杀"适配器"的组，确认 `sleep` 仍在并被 launchd 收养（复现 T003 的现象），最后用之前的记录清理掉它。
7. **fd 检查放在父进程一侧。** 先试了在 `sh` 里逐个探测 fd，bash 会把保存的 fd 放在 10 以上，结果不可靠。改为父进程用 `proc_pidinfo(PROC_PIDLISTFDS)` 读子进程的 fd 表；子进程用 `cat`，先让它回显一行，确认它已经启动完毕、停在 `read`，再读 fd 表，避免赶上 dyld 临时打开文件。测试里再故意留一个没有 `FD_CLOEXEC` 的 fd，证明检查确实有对象可查。
8. **变异检查。** `Command` 有一个只在 `cfg(test)` 下存在的开关，去掉 `POSIX_SPAWN_CLOEXEC_DEFAULT`；`unit_sys_spawn_mutation_without_cloexec_default_leaks_fds` 用它证明同一检查能看到泄漏。另外手工改源码做了四次真正的变异，确认对应的用例失败，改回后全部通过：

   | 变异 | 失败的用例 | 现象 |
   |---|---|---|
   | 去掉 `POSIX_SPAWN_CLOEXEC_DEFAULT` | `unit_sys_spawn_child_inherits_only_fds_0_to_2` | 子进程 fd 为 `[0, 1, 2, 4]` |
   | 去掉 `POSIX_SPAWN_SETSIGMASK` | `unit_sys_spawn_resets_the_signal_mask` | 子进程继承了阻塞的 SIGTERM，10 秒后正常退出 |
   | `SETSID` 换成 `SETPGROUP` | `unit_sys_spawn_new_session_or_new_group` | sid 不等于 pid |
   | 把宿主的环境追加进 envp | `unit_sys_spawn_child_gets_exactly_its_env` | 子进程多出宿主的变量 |

9. **信号处置。** `SETSIGDEF` 没有自动化用例：要验证就得改测试进程的信号处置，而处置是进程级的，会影响并行的其他用例。信号掩码是线程级的，所以测了掩码。
10. **stdio 的边界。** 管道两端都由 `std::io::pipe()` 创建（带 close-on-exec），子进程一端只经 dup2 文件动作进入子进程。若父进程关掉了 0–2，新管道可能正好落在 0–2，dup2 源和目标相同会被跳过，所以先把这类 fd 挪到 3 以上。工作目录用 `posix_spawn_file_actions_addchdir_np`（macOS 10.15 起），程序路径必须是绝对路径，不搜索 `PATH`。
11. **依赖写在 crate 自己的 `Cargo.toml` 里**，没有加到根 `[workspace.dependencies]`，避免和并行任务改同一个文件。根配置只有 `Cargo.lock` 变化（新增 `cupper-sys`、`libc`、`thiserror` 三项）。
12. **没有加 `cfg(target_os = "macos")` 门控。** 这个 crate 只在 macOS 上编译（设计 5.1.6：依赖它的作业都在 macOS 上），ubuntu 上的作业不编译它。
13. **没有并入的 SP-4a 内容。** `spawn_adapter` 的 stderr 追加写日志改为由调用方选 `Stdio::Pipe`（设计 5.2.15：stderr 改为管道）；`wait_blocking` 变成 `Child::wait`；`kill_group`、`group_members`、`reap_group` 变成 `ProcessGroup`；`current_uid` 原样保留，供数据目录和套接字的属主检查使用。

## 稳定性

`cupper-sys` 的测试二进制 3 个一组并发跑 30 轮（共 90 次），全部通过；每次结束后 `ps` 里没有残留的 `sleep`、`cat`、`sh`。
