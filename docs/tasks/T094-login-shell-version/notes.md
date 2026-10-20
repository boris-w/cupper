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
