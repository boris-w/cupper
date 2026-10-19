# T020 汇报

- 执行者：子 agent（Rust 系统工程师，macOS）
- 分支：`feat/T020-sys-process`
- 完成日期：2026-10-19

## 做了什么

新建了 `crates/cupper-sys`，把 SP-4a `sys.rs` 里与进程有关的部分迁移过来，并按设计 5.8.5、5.6.8、5.2.13 和 T003 的结论做了调整。现在它是工作区里唯一允许写 `unsafe` 的 crate。

- **拉起子进程**：入口是 `spawn(&Command) -> Result<Child>`。
  - 固定使用 `POSIX_SPAWN_CLOEXEC_DEFAULT`、`SETSIGDEF`（所有信号恢复默认处置）和 `SETSIGMASK`（空信号掩码）。
  - 子进程默认自成进程组（`SETPGROUP`），调用 `new_session()` 后改为自成会话（`SETSID`）。两种情况下子进程都是组长，pgid = pid。
  - stdin、stdout、stderr 都要显式指定，可选 `/dev/null` 或管道。
  - 程序路径必须是绝对路径，不搜索 `PATH`。工作目录可选。
  - 子进程刚起来就读出它的启动时间，记在 `Child::start()` 里。
  - `Child` 提供 `wait`、`try_wait` 和 `group()`。
- **环境变量白名单**：子进程只拿到 `Env` 里的变量，不继承宿主自己的环境。
  - `Env::from_allowed(来源, &["PATH", "LC_*"])` 支持精确名和"前缀*"两种写法。
  - `CUPPER_*` 一律拒绝，不管白名单怎么写（ADR-05）。
  - 具体放行哪些名字由调用方决定（5.6.6，归 T054）。
- **进程信息**：`process_info`、`parent_chain`、`descendants`，返回 `ProcessInfo { pid, ppid, pgid, sid, uid, start, name }`。父进程链能经过 root 进程（如 `login`）一直走到 launchd。
- **进程组清理**：`ProcessGroup { pgid, start }`，也就是设计里说的"组号加启动时间"。
  - `with_descendants()` 在发任何信号之前，记下进程树里每个进程组。
  - `signal(Term | Kill)` 只对仍是当初记录的那个组发信号；`members()` 和 `is_alive()` 用来核对。
  - 组号 0、组号 1 和宿主自己的组永远不会被发信号。
- **系统信息**：`boot_session_uuid()` 读 `kern.bootsessionuuid`，`user_temp_dir()` 取 `DARWIN_USER_TEMP_DIR`。另外保留了 SP-4a 的 `current_uid()`。
- **过程记录**：`docs/tasks/T020-sys-process/notes.md`，含接口一览、取舍和手工变异检查的结果。

## 关键取舍

1. **进程信息不用 SP-4a、SP-4b 用的 `PROC_PIDTBSDINFO`。** 实测它只能读同一用户的进程，父进程链到 `login`（root）就断了，到不了 launchd，而 5.8.4 要靠"链是否断在 launchd"判断来源。
   - 现在的做法：pid、ppid、pgid、uid、进程名从 `PROC_PIDT_SHORTBSDINFO` 取，它能读任何用户的进程。
   - 启动时间从 `sysctl(KERN_PROC_PID)` 取：`kinfo_proc` 的第一个字段就是 `p_starttime`，结构体长度 648 字节用常量断言固定。实测与 `PROC_PIDTBSDINFO` 的结果逐微秒一致。
2. **僵尸进程。** 实测两种 libproc 记录都读不到僵尸，`getsid` 也失败，只有 `sysctl` 还能读到启动时间；列组成员和列子进程的调用会把僵尸列出来。
   - 所以 `process_info` 对僵尸返回 `None`，组成员和后代的列表里因此不含僵尸。
   - `spawn` 用 `sysctl` 读启动时间，子进程即使立刻退出也读得到。
3. **SIGTERM 和 SIGKILL 之间的等待不在本 crate 里做。** 设计要求"限时等待"，而定时只能经 `Clock`（`disallowed-methods` 对本 crate 同样生效）。所以 `cupper-sys` 只提供记录、核对、发信号三个原语，等待和升级由宿主的 Supervisor 用 `Clock` 编排。
4. **怎么判断组号没被别的组重用**（沿用 SP-4a 的规则）：
   - 组长如果还活着，启动时间必须与记录一致；
   - 所有成员都不能早于记录的启动时间；
   - 组长已退出时，记录取最早成员的启动时间。
   - 剩余风险写在了 `members` 的文档里：组号被重用、新组长又退出、而新组仍有成员时，认不出来。
5. **怎么在不跑真实 CLI 的情况下复现 T003 的场景。** 用 `sh -c 'set -m; sleep 30 & …'`：`set -m` 让后台作业自成进程组，相当于 Claude Code 给工具命令新建的组。用例的步骤：
   - 先记录整棵树的组；
   - 只杀"适配器"所在的组，确认 `sleep` 仍在运行，并已被 launchd 收养；
   - 再用先前的记录把它清理掉。
6. **怎么检查子进程继承了哪些 fd。** 由父进程用 `PROC_PIDLISTFDS` 读子进程的 fd 表，而不是让 shell 自己探测（bash 内部会占用 10 号以上的 fd，结果不可靠）。子进程用 `cat`，先让它回显一行，确认它已经启动完毕再读，避开 dyld 临时打开的文件。
7. **依赖写在 crate 自己的 `Cargo.toml` 里**，没有加到根 `[workspace.dependencies]`，避免和并行任务改同一个文件。T022 也是这么做的。
8. **没有加 `cfg(target_os = "macos")` 门控。** 本 crate 只在 macOS 上编译，ubuntu 上的作业不会编译它。

## 验证方式

- **UNIT-sys-spawn**，共 8 个用例，都以 `unit_sys_spawn_` 开头：
  - 子进程只有 fd 0、1、2，而父进程此时确实开着一个没有 `FD_CLOEXEC` 的 fd。
  - 变异检查：测试专用开关去掉 `CLOEXEC_DEFAULT` 后，同一检查能看到泄漏的 fd。另外手工改源码做了 4 次真正的变异：去掉 `CLOEXEC_DEFAULT`、去掉 `SETSIGMASK`、把 `SETSID` 换成 `SETPGROUP`、把宿主的环境变量传给子进程。每次对应的用例都失败，现象记在 notes 里；改回之后全部通过。
  - `SETSID` 生效：sid = pgid = pid，且与宿主的会话不同。不带 `new_session` 时只新建进程组。
  - 环境变量：`/usr/bin/env` 的输出与 `Env` 完全一致；白名单过滤和 `CUPPER_*` 的拒绝有各自的用例。
  - 另有信号掩码、`/dev/null` stdin 加工作目录、非法参数各一个用例。
- **UNIT-sys-reap-group**，共 5 个用例：
  - 能清理整个进程组；
  - 后代在另一个进程组里时，也能被枚举并清理到（T003 的情形）；
  - 记录与当前的组对不上时，不发信号；
  - 不对组号 0、1 和自己的组发信号；
  - 组长退出后，记录取最早成员的启动时间。
- **其他**：另有 7 个用例覆盖进程信息、父进程链（走到 launchd）、僵尸进程、boot UUID 和临时目录（0700，属主是当前用户）。
- **`unsafe`**：只出现在 `cupper-sys`。crate 级 `#![allow(unsafe_code, reason = …)]` 写明了原因，同时打开 `#![deny(clippy::undocumented_unsafe_blocks)]`，每个 unsafe 块都有 `SAFETY:` 注释。
- **稳定性**：测试二进制 3 个一组并发跑了 30 轮（共 90 次），全部通过。每次跑完用 `ps` 检查，没有残留的 `sleep`、`cat`、`sh`。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T020-sys-process/ci/`，通过，摘要是 `pr-d0a337769228.json`：
  - rust 36 个通过、0 个失败；ts、license 通过；
  - 逐编号结果里 `UNIT-sys-spawn` 和 `UNIT-sys-reap-group` 都是 pass。
- **其他检查**：`RUSTDOCFLAGS=-D warnings cargo doc -p cupper-sys` 无警告；提交和文件里没有本机路径或用户名。

## 提交列表

```
9dbaced docs(task): record the T020 pr CI summary
d0a3377 docs(task): add T020 notes on cupper-sys trade-offs and mutation runs
c4ce319 feat(sys): record and signal the process groups of a process tree
f53b226 feat(sys): spawn children with posix_spawn and explicit stdio
8191c6c feat(sys): add the allow-listed child environment
eb17a03 feat(sys): read process start time, session and parent chain
178b089 feat(sys): add cupper-sys with boot session UUID and user temp dir
```

## 遗留问题 / 范围外发现

1. **合并会在 `Cargo.lock` 上冲突**：main 已合入 T022 和 T070。用 `git merge-tree` 只读预演过，只有这一处冲突。处理办法：取 main 的版本，再跑一次 cargo 重新生成（本分支新增的是 `cupper-sys`、`libc 0.2.191`、`thiserror 2.0.21` 三项）。根配置里除 `Cargo.lock` 外，没有改 `Cargo.toml`、`clippy.toml`、`deny.toml`。
2. **规模**：全部 1,625 行，其中测试和测试辅助约 600 行，产品代码约 1,000 行。略超约 1,500 行的上限，不算明显超出，所以没有中途停下。
3. **`SETSIGDEF` 没有自动化用例**：信号处置是进程级的，在测试里改它会影响并行运行的其他用例。只测了线程级的信号掩码。
4. **给后续任务的提醒**（建议写进交接表）：
   - **T037、T055**：关闭顺序是先 `child.group().with_descendants()` 记下所有组，再逐组 `signal(Term)`，用 `Clock` 限时等待并检查 `is_alive()`，最后对仍在的组 `signal(Kill)`。持久化时存 `pgid` 和 `start.as_micros()`。适配器如果在第 2 步自行退出，它的后代就已经找不到了，只能靠资源采样时记下的组。
   - **T021**：`PROC_PIDTBSDINFO` 读不了其他用户的进程，而 `PROC_PIDT_SHORTBSDINFO` 和 `PROC_PIDUNIQIDENTIFIERINFO`（flavor 17，56 字节）对 root 进程也能读，实测 pid 1 可以。libc 0.2.191 没有定义 `POSIX_SPAWN_SETSID` 和 `proc_bsdinfowithuniqid`，需要自己声明。
   - **T038**：`parent_chain` 能经过 root 进程到达 launchd；进程名是内核的 16 字节短名，要完整路径另取。
   - **T054**：直接用 `Env::from_allowed` 加 `set`。`CUPPER_*` 已在本 crate 里拒绝，那边不必再处理。
   - **使用约束**：`spawn` 不搜索 `PATH`，调用方要传解析好的绝对路径。`Child` 不会在 drop 时自动回收，必须 `wait`，否则子进程会一直是僵尸。