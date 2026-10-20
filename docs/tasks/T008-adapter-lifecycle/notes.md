# T008 工作笔记

- 执行者：子 agent（Rust 工程师；T043、T088、T097、T055 的执行者）

## 开工前读过的

- 任务书；设计 5.6.1、5.6.8，5.2.13 的资源采样和关闭顺序；`handoff-notes.md` 里给 T055 的关闭顺序一行（T020），T055 的 `notes.md` 里给 T008 的交接；
- 现有代码：
  - `driver/runtime/`（T055 的 actor、`launch.rs`）；
  - `durations.rs`；`startup/test_config.rs` 的 `rss_samples`；`clock/manual.rs`（`next_deadline`）；
  - `cupper-sys` 的 `group.rs`、`proc.rs`、`spawn.rs`；
  - 假 agent 的 `main.rs`、`config.rs`、`script.rs`（`spawn_child`）；
  - 迁移 `0001_initial.sql` 和设计 04 的 `adapters` 表。

## 开工前发现的几点（影响做法，需要裁定的列在最后）

1. **常驻内存没有现成接口**：`cupper-sys` 只有进程信息（pid、pgid、sid、启动时间、名字），没有常驻内存。需要在 `cupper-sys` 里加一个 `proc_pidinfo(PROC_PIDTASKINFO)` 的读取函数（`unsafe` 只能在这里）。
2. **后代的进程组记在哪里**：设计 5.6.8 说采样时把后代的进程组"写进 `adapters` 表"，但设计 04 和 `0001_initial.sql` 的 `adapters` 表都没有对应的列。
3. **`setsid` 孙进程**：
   - 假 agent 不在"必须经 `cupper_sys::spawn`"的范围里（那条规则只管宿主、CLI 和原生扩展），但它也不能用 `unsafe`（`pre_exec` 里调 `setsid` 就要 `unsafe`）。
   - `cupper_sys::spawn` 的标准流只有 `Null` 和 `Pipe`，孙进程拿不到适配器的 stdout，复现不了"孙进程拿着 stdout、实例结束不了"的情形。
   - 标准库的 `CommandExt::process_group(0)` 是安全的 API：孙进程进入一个新的进程组（不是新的会话），并继承 stdout。对宿主的清理来说，两者的处理完全相同：都不在适配器的进程组里，都要靠枚举后代来找。
4. **"有会话在运行回合时立即重启"**：运行时不知道会话有没有在跑回合。实例一退出，会话就收到 `AdapterExited`，回合以 `adapter_exited` 结束，之后要等下一次 attach 才会再拉起适配器。
5. **`crashed` 之后的手动重启**：`agents/restart` 不在任何一个任务的"做"里（T009 一行只有 `agents/capabilities`、`agents/refresh`）。
6. **用例的时间**：
   - 宿主库的用例在 `host-release` 特性组里也会跑，这时没有 `test-harness`，`test_config.durations` 用不了。
   - 所以运行时的用例一律用 `ManualClock`：先用 `next_deadline()` 等运行时把定时器设好，再推进时钟。
   - 进程本身仍然是真实时间，例如 KILL 之后要真的等进程退出。

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释。实现按 T055 的实测比例放大 1.15 倍，测试放大 1.1 倍。

**实现**

| # | 模块 | 内容 | 草估 |
|---|---|---|---|
| 1 | `cupper-sys` 常驻内存 | `resident_size(pid)`（`PROC_PIDTASKINFO`） | 20 |
| 2 | 假 agent | `spawn_group_child [程序]`：新进程组，继承 stdout，回显 pid；`--fault linger`：忽略 SIGTERM，stdin 关闭后不退出；tokio 加 `signal` 特性 | 25 |
| 3 | 时长登记 | 关闭时等 stdin 关闭后的退出 3 s、SIGTERM 后 2 s；退避上限 30 s；失败窗口 5 min；采样周期 30 s | 15 |
| 4 | 重启策略的纯核心 | 窗口内的失败记录；下一次拉起的最早时间（1、2、4…30 s）；5 分钟内 5 次即 `crashed`（`FailureCode::AdapterCrashed` 已有） | 50 |
| 5 | 运行时接线：重启 | 意外结束时记一次失败；有等待中的 attach 时按退避定时拉起；`crashed` 时 attach 直接失败 | 60 |
| 6 | 关闭顺序 | 逐个 `session/close` → drop 连接 → 限时等退出 → 枚举后代（`with_descendants()` 并上采样记下的组） → 逐组 TERM → 限时、`is_alive()` 核对 → KILL；`stop` 限时返回；actor 里加一个定时器分支 | 110 |
| 7 | 意外退出的清理 | 退出时清理适配器进程组的残留和采样记下的组；进程已退出但连接迟迟不关（孙进程拿着 stdout）时，限时后强制结束实例 | 25 |
| 8 | 资源采样 | 30 s 定时；`with_descendants()`；各组成员的常驻内存之和（有 `rss_samples` 时用注入值）；`claude` 进程数，超过会话数时写日志（Q43）；记下后代的组；采样接口（`watch` 接收端）；`rss_samples` 经 `RuntimeDeps` 从 `Host::start` 传入 | 90 |
| 9 | 后代组的持久化 | 见待裁定 1 | 0–5 |
| 10 | stderr 尾部 | `AdapterExited` 加 `stderr`（`Tail::text()`），改几处构造点（含两处性质测试） | 15 |
| | 实现小计 | | **约 415**，×1.15 ≈ **480** |

**测试**

| 用例 | 内容 | 草估 |
|---|---|---|
| `cupper-sys` | 自己的常驻内存大于 0 | 8 |
| 重启策略单测 | 表驱动：退避序列、窗口滑出、第 5 次进入 `crashed` | 30 |
| 夹具 | 运行时用例的夹具改成可带参数（`--fault`）和 `rss_samples`；等 `next_deadline` 的辅助函数 | 30 |
| IT-K21-reap-group | 同组子进程和新组孙进程（拿着 stdout）在 `stop` 之后都不在了；只向用例自己拉起的组发信号 | 40 |
| IT-adapter-restart | 崩溃（`exit 1`）后按 1、2、4 s 退避重启；连续 5 次进入 `crashed`，attach 失败 | 60 |
| 关闭顺序 | `session/close` 先于 stdin 关闭（看 `raw.db` 的帧）；`linger` 的假 agent 不理 TERM，被 KILL；`stop` 在 3 s + 2 s 的时钟内返回 | 50 |
| 资源采样 | 注入的 `rss_samples`；用名为 `claude` 的链接拉起一个子进程，计入 `claude` 进程数；后代的组被记下 | 35 |
| 测试小计 | | **约 255**，×1.1 ≈ **280** |

**合计约 760 行**。按 T055 的实测与估计之比（约 1.1）推算，会到 830 左右，贴着或超出约 800 的线。按任务书，开工前停下，没有写代码。

## 拆分建议

**方案一（建议）：拆成两份**

| 拆分 | 内容（模块编号） | 用例 | 估计 |
|---|---|---|---|
| T008（收窄）重启与关闭 | 2、3、4、5、6、7、10 | IT-K21-reap-group、IT-adapter-restart、关闭顺序 | 约 560 |
| T008b 资源采样 | 1、8、9，加上 3 里的采样周期 | 资源采样的用例 | 约 230 |

- 拆开以后，T008 收窄版的意外退出清理只能清理适配器自己的进程组；采样记下的组要等 T008b 合并后才接上。
- 关闭时的后代枚举不依赖采样，`stop` 一定能清理干净。

**方案二**：不拆，把上限提到约 850、硬线 900。

## 待裁定

1. **后代组的持久化**，二选一：
   - 给 `adapters` 加一列 `groups TEXT`（JSON：`[[pgid, start_us], …]`），同时改 `0001_initial.sql` 和设计 04，同 T043 时 `reply_to` 的做法，约 5 行；
   - 先只记在内存里，等 T059 做"启动时按 `adapters` 表清理残留"时再定表结构。
2. **孙进程用新的进程组还是新的会话**：建议用标准库的 `process_group(0)`（见发现 3）。如果一定要 `setsid`，需要给 `cupper-sys::spawn` 加一种继承 fd 的标准流，或者让假 agent 依赖 `cupper-sys`，规模另计。
3. **"立即重启"的含义**：建议是有 attach 在等时按退避定时拉起，否则等下次使用再拉起；两种情况都遵守退避。
4. **`crashed` 之后怎么恢复**：需要 `agents/restart`。建议另派，或者归 T009。本任务只留一个清除 `crashed` 的方法。

## 裁定之后（方案二：不拆，上限约 850，硬线 900）

### 实际规模

按 `git diff --numstat`（相对估计提交 ab30ccd，不含 `Cargo.lock` 和文档）：新增 804 行，删除 138 行。实现约 520，测试约 285。

| 模块 | 新增 |
|---|---|
| `cupper-sys` 常驻内存（含测试 6） | 24 |
| 假 agent：`spawn_group_child`、`linger` | 33 |
| 时长登记 | 16 |
| `lifecycle.rs`：重启策略、并集、发信号、采样（含单测 22） | 146 |
| 运行时接线（`mod.rs`）：定时器、重启、关闭顺序、意外退出、采样、接口 | 280 |
| `launch.rs`：连接可关闭、`record_groups`；`api.rs`：`Crashed`、`stderr`；宿主传入时长和 `rss_samples` | 39 |
| 迁移的 `groups` 列 | 2 |
| 运行时用例（夹具改写，T055 的三条用例迁到新夹具） | 265 |

估计 760、推算 830，实际 804。

之后按协调追加了 `SetConfig`（T044 的"回合中越权先改回"在生产路径上要用）：新增 65 行，删除 13 行，包括运行时、`api.rs` 的文档、`ClaudeSession::config()` 和一条用例。合计新增 869 行，超过约 850 的上限，在 900 的硬线以内。

### 做法与取舍

- **定时器**：运行时用 `TimerTable` 加 `Alarm`，和会话 actor 一样，键有五种：`Start`、`Sample`、`Stdin`、`Term`、`Kill`。重启策略 `Restarts` 是纯核心，单测覆盖退避序列、窗口滑出和第 5 次进入 `crashed`。
- **关闭顺序**：
  1. 收到 `stop` 时**先**枚举后代：这时适配器还活着，枚举的结果并上采样记下的组；
  2. 逐个 `session/close`，全部回复或 3 s 后关闭 stdin；
  3. 3 s 后再枚举一次，并上第 1 步的结果，逐组 SIGTERM；
  4. 2 s 后逐组 SIGKILL，不再等连接关闭，只等进程退出。

  设计里的第 3 步（关 stdin 之后再枚举）在适配器正常退出时已经找不到后代了，所以在第 1 步就先记下。`stop(force)` 和不强制走同一个顺序，最长 3 + 3 + 2 s。
- **意外退出**：
  - 适配器自己退出时，对它的进程组和采样记下的组直接 SIGKILL，连接最多再等 2 s（`Kill` 定时器）；
  - 算一次失败。有 attach 在等时按退避定时拉起，否则等下次使用；
  - 拉起失败（`Started(Err)`）也算一次失败。
- **`claude` 进程数（Q43）**：按"可执行文件的真实路径等于定位到的 CLI 的真实路径"来数，不按进程名。
  - 内核的短进程名跟着链接的目标走：原生安装的 CLI 文件名是版本号，经链接以 `claude` 启动时，进程名也不是 `claude`；
  - 用例里把 `/bin/sleep` 当作 CLI。
- **采样**：
  - 每 30 s 一次，统计适配器进程组和后代进程组所有成员的常驻内存，`rss_samples` 有值时用注入值；
  - 记下的组取并集，并去掉已经不在的；写进 `adapters.groups`；
  - 经 `AgentRuntime::resources()` 的 `watch` 接收端提供；`claude` 进程数超过会话数时写一条警告日志。
- **用例的时钟**：
  - `Fixture::tick` 在等待期间把时钟推到运行时设的、不超过给定上限的截止时间。下一次采样在 30 s 之后，所以等重启时不会被推过去。
  - 退避用例的时间是精确的：1 s、2 s，以及 1 + 2 + 4 + 8 s。
  - 关闭用例只断言上下限：适配器在 I/O 上等待时，时钟可能先被推到 `Stdin` 的截止时间。
- 假 agent 的 `linger`：stdin 关闭后 `serve` 可能以 "transport closed" 出错，这时也要留下来，否则就测不到 SIGKILL 那一步（已修，见 `fix(fake-agent)` 提交）。
- 用例都在通过的路径上确认没有残留进程：子进程和孙进程在 `stop` 或崩溃之后都不在了。只向用例自己拉起的进程组发信号，`ProcessGroup::signal` 会先核对组号和启动时间。

- **`SetConfig`**：
  - 会话发来的是 cupper 的选项 ID（如 `permission_mode`），按会话当前的选项换成 agent 自己的 ID 后发 `session/set_config_option`；
  - 回复里的全部选项经 `ClaudeSession::set_config` 变成 `ConfigChanged` 送回该会话；
  - agent 拒绝时什么也不发，由 T044 的守卫按自己的期限处理；
  - 用例名用 IT-orch-gating 开头（T044 的编号）。

### 交接

- **T111**：`agents/restart` 调 `AgentRuntime::clear_crashed()`。
- **T009 / T100**：`AgentStatus` 和设置页的内存显示读 `AgentRuntime::resources()`；`crashed` 状态目前只体现在 attach 返回的错误类 `Crashed` 上（`FailureCode::AdapterCrashed`）。
- **T059**：宿主启动时按 `adapters` 表清理残留，`groups` 列里是采样记下的 `[[pgid, start_us], …]`。
- **会话侧**：`AdapterExited.stderr` 已经带上最后的 stderr（脱敏），但会话 actor 还没有把它写进 `error` 条目的详情（MS-5）。
- **Codex 的回收**（5.6.8：常驻内存超过 2 GiB，或 30 分钟没有回合）：可以用 `resources()`，归 Codex 接入的任务。
