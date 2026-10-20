# T008 汇报

- 执行者：子 agent（Rust 工程师；T043、T088、T097、T055 的执行者）
- 分支：`feat/T008-adapter-lifecycle`
- 完成日期：2026-10-20

T008 已做完，追加的 `SetConfig` 也一起做了。PR 套件在干净的 worktree 里全部通过，摘要已提交。规模新增 869 行：超过约 850 的上限，没到 900 的硬线。不加 `SetConfig` 是 804 行；超出的部分全来自追加项。代码已经写完，所以没有在中途停下。

## 做了什么

1. **重启退避与 `crashed`**：
   - 新文件 `driver/runtime/lifecycle.rs` 里的纯核心 `Restarts` 负责记账：一次失败后等 1 s，5 分钟内每多失败一次就翻倍，最长 30 s；5 分钟内第 5 次失败进入 `crashed`。
   - 两种情况都算一次失败：适配器自己退出（不是我们要它停的），和拉起失败。
   - 有 attach 在等时，退避到期就拉起；没有就等下次使用，同样遵守退避。
   - 进入 `crashed` 后，attach 直接失败，错误类是新加的 `AgentErrorClass::Crashed`，对应 `FailureCode::AdapterCrashed`。
   - 恢复用 `AgentRuntime::clear_crashed()`，留给 T111 的 `agents/restart`。
2. **关闭顺序（5.6.8）**：
   1. 收到 `stop` 时，趁适配器还活着先枚举所有后代，并上采样记下的进程组；
   2. 逐个 `session/close`，全部回复或 3 s 后关闭 stdin（`Instance.conn` 改成 `Option`，丢掉连接就关闭了 stdin）；
   3. 3 s 后再枚举一次，逐组 SIGTERM；
   4. 2 s 后逐组 SIGKILL，不再等连接关闭。

   `stop` 最长 3 + 3 + 2 s。孙进程拿着 stdout、实例迟迟结束不了的问题，由此解决。
3. **意外退出**：对适配器的进程组和采样记下的组直接 SIGKILL，连接最多再等 2 s，然后实例强制结束。
4. **资源采样**：
   - 每 30 s 一次，统计适配器进程组和后代进程组的常驻内存。为此在 `cupper-sys` 加了 `resident_size`，读 `PROC_PIDTASKINFO`。
   - `rss_samples` 有值时，用注入值代替测量值；它从 `Host::start` 经 `RuntimeDeps.rss` 传进来。
   - 统计 `claude` 进程数（Q43），超过会话数时写一条警告日志。
   - 后代进程组写进 `adapters.groups`；结果经 `AgentRuntime::resources()` 提供，是一个 `watch` 接收端。
5. **stderr 尾部**：`DriverEvent::AdapterExited` 加了 `stderr` 字段，内容取自 `Tail::text()`。
6. **假 agent**：
   - `spawn_group_child [程序]`：用标准库的 `process_group(0)` 把孙进程放进新进程组，孙进程继承 stdout，并回显它的 pid；
   - `--fault linger`：不理 SIGTERM，stdin 关闭后也不退出；
   - 设计 09 的剧本命令表补了这两项。
7. **时长**：登记了六项，都能被 `test_config` 覆盖：
   - `adapter_close_wait` 3 s；
   - `adapter_term_wait` 2 s；
   - `adapter_backoff` 1 s；
   - `adapter_backoff_max` 30 s；
   - `adapter_crash_window` 300 s；
   - `resource_sample` 30 s。
8. **`groups` 列**：按裁定改了 `0001_initial.sql` 和设计 04。
9. **`SetConfig`（追加）**：
   - 运行时把 cupper 的选项 ID 换成 agent 自己的 ID，发 `session/set_config_option`；
   - 回复里的全部选项以 `ConfigChanged` 送回该会话；agent 拒绝时什么也不发；
   - `AgentPort::send` 和 `DriverCommand::SetConfig` 的文档已补；为此给 `ClaudeSession` 加了 `config()`。

## 关键取舍

- **先枚举后代**：设计写的是关 stdin 之后再枚举。但适配器一旦正常退出，后代就被 launchd 收养，从进程树上找不到了。所以在 `stop` 一开始先枚举一次，第 3 步再枚举一次，两次取并集。
- **怎么数 `claude` 进程**：按可执行文件的真实路径是否等于定位到的 CLI，不按进程名。
  - 内核的短进程名跟着链接的目标走：原生安装的 CLI 文件名是版本号，经链接以 `claude` 启动时，进程名也不是 `claude`。
  - 用例里把 `/bin/sleep` 当作 CLI。
- **`stop(force)`**：强制和不强制走同一个有上限的顺序。
- **用例的时钟**：用例一律用 `ManualClock`。`Fixture::tick` 只把时钟推到给定上限以内的截止时间，所以下一次采样不会被提前触发。
  - 退避的时长精确断言：1 s、2 s，以及 1 + 2 + 4 + 8 s。
  - 关闭的时长只断言上下限：适配器还在 I/O 上等的时候，时钟可能先被推过了 `Stdin` 的截止时间。

## 验证方式

- `cargo xtask ci pr --record docs/tasks/T008-adapter-lifecycle/ci/` 在干净的 worktree 里通过，摘要是 `pr-0744b657a91b.json`：

  | 套件 | 通过 |
  |---|---|
  | rust | 1589 |
  | protocol | 185 |
  | ts | 353 |
  | license | 1 |
  | testdata-scan | 3 |

  sensitive-words 跳过，因为本机没有词表。
- 新增和迁移的用例都在 `driver/runtime/tests.rs`，共用一个 `ManualClock` 夹具：
  - **IT-K21-reap-group**，三条：
    - `stop` 清掉同组子进程，也清掉拿着 stdout 的新进程组孙进程；
    - `linger` 的假 agent 不理 SIGTERM，最后被 SIGKILL，`exit_status` 是 `signal: 9`；`stop` 在 5–8 s 的时钟内返回；`raw.db` 里有 `session/close`；
    - 采样：注入的内存、`claude` 进程数 1、记下的后代进程组写进 `adapters.groups`；之后适配器崩溃，这个组也被清掉。
  - **IT-adapter-restart**：崩溃后分别隔 1 s、2 s 重启，重启后能正常对话；连续 5 次拉起失败（1 + 2 + 4 + 8 s）后进入 `Crashed`；`clear_crashed` 之后又会尝试拉起。
  - **`SetConfig`**：`it_orch_gating_set_config_comes_back_as_all_options`。
  - T055 的 IT-process-hygiene、IT-K13、IT-K26 迁到新夹具，仍然通过。
  - 单元测试：重启策略的单测，以及 `cupper-sys` 的 `resident_size`。
- 运行时的用例连跑 8 次都通过；每次跑完都确认没有残留的 `cupper-fake-agent` 和 `sleep 3600` 进程。
- 只向用例自己拉起的进程组发信号；`ProcessGroup::signal` 会核对组号和启动时间。
- 每个提交单独跑过 clippy：默认特性和 `--all-features` 都过。`cargo doc -D warnings` 也通过。

## 提交列表

```
81b4d36 docs(task): T008 CI summary
0744b65 docs(task): T008 notes: SetConfig and the final size
014692e feat(host): the runtime serves `SetConfig`
be1ab93 docs(task): T008 notes: sizes, choices, handoffs
ccd66bf feat(host): adapter restarts, the closing order and sampling
5adca9e feat(host): restart policy, reaping and sampling for adapters
db0aa93 fix(fake-agent): `linger` stays even when serving ends with an error
3d0ad55 feat(fake-agent): a child in a new process group, and `linger`
acce453 feat(sys): read a process's resident memory
ab30ccd docs(task): T008 size estimate and split proposal
```

## 遗留问题 / 范围外发现

- **调试时残留过进程**：调试期间有一次用例先 panic 了（读 pid 列的类型不对），两个 `sleep 3600` 留在了本机。我已经用 SIGTERM 清掉；它们是这次用例自己拉起的进程。通过的路径上不会残留。
- **交接**（也写进了 `notes.md`）：
  - T111：`agents/restart` 调 `clear_crashed()`。
  - T009 / T100：`AgentStatus` 和内存显示读 `resources()`。`crashed` 目前只体现在 attach 返回的错误类上。
  - T059：宿主启动时的残留清理读 `adapters.groups`。
  - 会话侧：`AdapterExited.stderr` 已经带上，但还没有写进 `error` 条目的详情（MS-5）。
  - Codex 的回收（超过 2 GiB，或 30 分钟没有回合）可以用 `resources()`。
- **合并**：分支开得早，没有 T044 的代码。`SetConfig` 只改了运行时一侧；`props.rs`、`session_props.rs` 里给 `AdapterExited` 补 `stderr` 的两处，合并时可能和 main 有小冲突。
- **设计 09**：剧本命令表加了一行 `spawn_group_child`，故障注入那一行补了 `linger`。