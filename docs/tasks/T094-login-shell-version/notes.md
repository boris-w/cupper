# T094 工作笔记

- 执行者：子 agent（工程师；T004、T005 的执行者接手）

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行，不含 `Cargo.lock`。模块放在 `cupper-host` 的 `driver/probe/` 下，与 T054 草图里的 `driver/locate/` 错开，两边不互相 import；`driver/mod.rs` 各加一行 `pub mod`，合并时可能在相邻行上有一处小冲突。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| `probe/run.rs` | 经 `cupper_sys::spawn` 拉起一次性子进程，stdin、stderr 为 `/dev/null`，读 stdout（有上限）；在 `spawn_blocking` 里 `wait`，与 `Clock` 的定时器赛跑；超时先记下进程组及后代的组，逐组 SIGKILL，再回收 | 80 |
| `probe/login_shell.rs` | `$SHELL -l -i -c` 跑带随机标记的脚本，`/usr/bin/env -0` 输出；按标记截取、按 NUL 切分；只留固定的几项和调用方给的名字（支持 `LC_*` 这种前缀）；失败退回固定 PATH 并给语义码；`Debug` 只打印变量名 | 110 |
| `probe/version.rs` | 取第一个 `X.Y.Z`；比较；已验证范围（版本号字符串）和四种分类；`--version` 探测 | 80 |
| `probe/git.rs` | git 最低 2.39；`/usr/bin/git` 先跑 `xcode-select -p` 并确认开发者目录下的 `usr/bin/git`；git-lfs；语义码 | 70 |
| `probe/mod.rs`、`driver/mod.rs`、`durations.rs` | 模块入口；登记 `login_shell`、`version_probe` 两个 5 秒的时长 | 20 |
| **实现小计** | | **360**，按以往的偏差（T004 约 1.1 倍、T005 约 1.25 倍）计约 **420** |

**测试**

| 用例编号 | 内容 | 草估 |
|---|---|---|
| UNIT-login-shell | 假 shell 脚本：正常、rc 文件有输出、超时（`ManualClock` 推进，假 shell `exec sleep`，断言进程被结束）、没有标记、shell 不存在；取到的值不出现在 `Debug` 输出和错误文本里 | 100 |
| UNIT-version-classify | 版本号提取（`2.1.296 (Claude Code)`、`codex-cli 0.160.1`、`git version 2.39.5 (Apple Git-154)`、`git-lfs/3.4.0 (...)`）；四种分类；读不出版本；假 CLI 的 `--version` | 60 |
| git 检测 | 版本低于 2.39；git-lfs 不存在；把假脚本当作"占位程序路径"，假 `xcode-select` 报开发者目录不存在或目录下没有 git 时不运行它（假 git 一运行就留下标记文件） | 60 |
| **测试小计** | | **220** |

**合计约 640 行**（实现约 420，测试约 220），在约 700 的汇报线以内。每完成一个模块用 `git diff --numstat` 统计一次；推算超过约 700 行时停下汇报，硬停止线 800。

## 实际行数（逐模块）

`git diff --numstat`，不含本目录。

| 模块 | 合计 | 其中测试 | 草估（实现 + 测试） |
|---|---|---|---|
| `probe/run.rs` | 100 | — | 80 |
| `probe/login_shell.rs` | 239 | 90 | 110 + 100 |
| `probe/version.rs` | 211 | 84 | 80 + 60 |
| `probe/git.rs` | 205 | 93 | 70 + 60 |
| `probe/mod.rs`（含测试用的建脚本小工具）、`driver/mod.rs`、`durations.rs` | 32 | 14 | 20 |
| **合计** | **787** | **281** | **580** |

**超出了约 700 行的汇报线**，离 800 的硬停止线只差 13 行。实现约 506 行，是草估 360 的 1.4 倍；测试 281 行，是草估 220 的 1.3 倍。偏差主要来自两处：

- 结构体字面量和 `match` 被 rustfmt 展开（`MIN_GIT`、`VerifiedRange`、`Stub`、`LoginEnv` 的构造各占 4–6 行）；
- 每个模块都有不少文档注释（为什么不运行占位程序、为什么值不进日志等）。

**过程上的问题**：这次我把四个模块都写完才统计，没有做到"每完成一个模块统计一次"。如果写完 `login_shell.rs` 时就统计，那时已经约 365 行，按同样的偏差可以推算出总数会过 700，应当在那时停下汇报。代码已经全部完成、测试全绿，所以我没有为了压行数去挤格式（比如 `#[rustfmt::skip]`），只删掉了只有测试用到的 `Display`。要不要再精简、精简哪部分，请编排者定。

## 实现时的取舍

**模块位置**：放在 `cupper-host/src/driver/probe/`（`run`、`login_shell`、`version`、`git`）。T054 草图里的 `driver/locate/` 不会和它重名，两边不互相 import。T054 的 `notes.md` 目前只在它自己的分支上，main 上还没有，我是用 `git show` 只读地看的。`driver/mod.rs` 两边各加一行 `pub mod`，合并时可能有一处相邻行的小冲突。

**拉起辅助 `run`**：
- 经 `cupper_sys::spawn` 拉起，stdin、stderr 接 `/dev/null`，stdout 接管道，用 tokio 的 `pipe::Receiver` 读，上限 1 MiB；
- 读到 EOF 后，在 `spawn_blocking` 里 `wait`，与同一个 `Clock` 定时器赛跑；
- 超时、读失败或输出过多时：先用 `with_descendants` 记下整棵进程树的组，再逐组 SIGKILL，最后等 `wait` 返回，不留僵尸进程（交接 T020）。

**登录 shell**：
- 命令是 `<shell> -l -i -c "printf '%s' <开始标记>; /usr/bin/env -0; printf '%s' <结束标记>"`。标记是 `cupper-env-` 加 32 位随机十六进制，按标记截取后按 NUL 切分，所以 rc 文件的任何输出都不影响解析（用例里在 rc 的位置故意打印了带 NUL 的内容）。
- shell 的路径和 shell 自己的初始环境（`Env`）都由调用方给，T055 从宿主自己的环境取。
- 固定保留 PATH、代理变量的大小写两组、`SSH_AUTH_SOCK`、`CLAUDE_CONFIG_DIR`、`CODEX_HOME`；另外加调用方给的名字，支持 `LC_*` 这种前缀，T055 传内嵌清单的 `env.allow` 加放行名单。
- 失败时退回固定 PATH，`failure` 带语义码：`login_shell_missing`、`login_shell_timeout`、`login_shell_no_environment`、`login_shell_failed`。shell 不存在由 `spawn` 失败判出；`SHELL` 为空时，T055 直接用 `LoginEnv::fallback(Failure::ShellMissing)`。
- **不进日志**：模块里没有任何日志调用；`LoginEnv` 的 `Debug` 只打印变量名；错误类型不带输出内容。用例里给假 shell 一个像令牌的值，断言它和 PATH 的值都不出现在 `Debug` 输出里。宿主目前还没有通用的日志模块（`host.log` 归 T037），以后接日志的任务只要不手动打印 `vars` 的值即可。
- **`TestConfig.login_env` 怎么接**（核对 `startup/test_config.rs`：它是 `Option<BTreeMap<String, String>>`，注释写"代替运行登录 shell"）：有值时，T055 直接构造 `LoginEnv { vars: login_env, failure: None }`，不运行 shell。`LoginEnv` 的字段是公开的，不需要另加构造函数。

**超时用例**：
- 假 shell 先把自己的 pid 写进文件，再 `exec /bin/sleep 600`；
- 用例等 `ManualClock` 上出现定时器、且 pid 文件写完后，推进 5 秒；
- 断言结果是 `login_shell_timeout`，并且那个 pid 已经不存在（`cupper_sys::process_info` 返回 `None`）。
- 不真的等 5 秒。

**版本**：
- `Version::find` 取第一个前面不是数字或点的 `X.Y.Z`；只从 ASCII 数字处切字符串，输出里有任意 UTF-8 也不会出错。
- 分类：`unsupported_below` 之下为 `unsupported`（不含边界）；`min`、`max` 为闭区间；
- 范围字符串不是 `X.Y.Z` 时返回 `BadVersion`，不猜。
- `probe` 要求退出码为 0；读不出版本时返回 `None`，由 T055 显示为"无法读取版本"。

**git**：
- 占位程序和 `xcode-select` 的路径放在 `Stub` 里，生产用 `Stub::system()`（`/usr/bin/git`、`/usr/bin/xcode-select`），用例换成假脚本。
- 只有当调用方给的 git 路径等于 `Stub.git` 时，才先跑 `xcode-select -p`：要求退出码为 0、输出是绝对路径、其下有 `usr/bin/git`，否则判为 `NeedsCommandLineTools`。
- 用例里的假 git 一运行就留下标记文件，断言三种"没装"的情形下都没有运行它：`xcode-select` 报错、目录里没有 git、`xcode-select` 本身不存在。
- git 低于 2.39 给 `git_too_old`；git-lfs 没找到给 `git_lfs_missing`。git 检测的用例算在 UNIT-version-classify 名下（本任务只登记了两个编号）。

**用例耗时**：macOS 第一次执行新写出的脚本要多花约 0.15–0.35 秒（系统对新可执行文件的检查，第二次执行几乎为 0），所以 8 条用例并行约 2.5–3 秒，主要花在这上面。
