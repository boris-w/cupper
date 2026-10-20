# T129 工作笔记

- 执行者：子 agent（Rust 工程师）
- 日期：2026-10-20

## 读到的现状（开工前）

- `Installer` 的字段与 `run(&clock, publish)` 都是现成的（T056、T115）；`publish` 每次状态变化给出全部组件的 `Statuses`。
- 索引流的 `agents` 由 `AgentRuntime` 经 `AgentNotes` 报来的 `AgentStatus` 合并；其中 `adapter.runtime` 现在由拉起计划推出（文件在不在）。安装器的状态要叠加上去：索引记下安装器给出的 `runtime`、`failure`，写进 Claude 那一项，之后 `AgentRuntime` 再报来时照样覆盖这两个字段。
- 进程级用例的难点：宿主的计划来自编译进去的清单，哈希是真实的 Node 等文件；测试造不出与之相符的包。要给 `test_config.runtime` 加一个只在 `test-harness` 构建里有的"测试清单"文件路径，宿主用它代替内嵌清单算计划（和 `registry_url` 同一类注入点）。包的位置照旧由 `BundleDir::of_this_host()` 从可执行文件推出：用例把宿主复制进一个假的 `Contents/MacOS/`，旁边放假 Node（T115 的 shell 脚本）、esbuild-wasm 的两个小文件、锁文件和打包脚本。
- 静止屏障：安装器的等待主要是外部 I/O（下载、打包子进程），任务书说不在屏障范围内。打算登记一个 actor 种类 `runtime_installer`，把整个 `run` 包进 `Participant::idle`：在等待（定时器、下载、`spawn_blocking` 的文件工作、子进程）时算空闲，被唤醒后到下一次等待之间算忙。不用 `Hold`：`Hold` 会让屏障一直等到下载结束。
- `npm::download` 用 `response.bytes()`；改成按块读，累计超过 64 MiB 就按网络失败。

## 开工前的规模估计

实现按以往偏差乘 1.2，测试按写出来的估。

| 模块 | 实现 | 测试 |
|---|---|---|
| 下载大小上限 | 15 | 20 |
| 安装器接线：构造参数、后台运行与 actor 种类、发布到索引与监护（忙闲、`BundleChanged`） | 60 | — |
| 索引：安装器状态叠加进 `agents` 里 Claude 的 `adapter.runtime`、`failure` | 35 | — |
| 监护：`installer_busy` 改取安装器 | 10 | — |
| `test_config.runtime.manifest`（`test-harness`） | 15 | — |
| IT-runtime-startup：假包与测试清单、本地 registry 与 tarball、装到 `ready`、包内文件被改时转入待升级、宿主守卫 | — | 150 |
| **合计** | **约 135** | **约 170** |

合计约 **305 行**，在约 400 的停止线以内，开工。

## 实现记录

- **构造与启动**（`supervisor/mod.rs`）：在索引启动之后（`Watch::new()` 与自检的设置都在它之前）构造 `Installer`：
  - `plans` 取内嵌清单的计划；`test-harness` 构建里有 `test_config.runtime.manifest` 时改用那份清单；
  - `bundle` 取 `BundleDir::of_this_host()`；
  - `dir` 取 `runtime_dir(data_dir)`；
  - `in_use` 为空：启动时还没有适配器在跑；
  - `registry` 取 `test_config.runtime.registry_url`，没有就用 `npm::REGISTRY`；
  - `proxy` 取登录 shell 的 `HTTPS_PROXY`；
  - `durations`。
- **只在 app 包里运行**：`BundleDir::is_app()` 判断 `Contents` 目录。开发与测试用的 `target/debug/cupper-host` 不在包里，不启动安装器；否则每个这样的宿主都会因"包里缺文件"进入待升级并立即退出。`--self-test` 的行为不变（它在包外照样报告 `missing`）。
- **静止屏障**：登记 actor 种类 `runtime_installer`，把整个 `run` 包进 `Participant::idle`。在等待（下载、打包子进程、`spawn_blocking` 的文件工作、重试的定时器）时算空闲，被唤醒后到下一次等待之间算忙。没有用 `Hold`：`Hold` 会让 `test/clock_advance` 等到下载结束，而任务书说等外部 I/O 不在屏障范围内。种类无论宿主在不在包里都登记，"宿主登记了每一种 actor"的用例照旧成立。
- **发布**：每次状态变化：
  - 忙闲（有组件 `Installing` 即忙）写进共享的 `AtomicBool`，监护观察时读它（`Observed.installer_busy`）；
  - 有组件 `Failed(BundleChanged)` 就调 `Watch::bundle_changed()`；
  - `runtime::adapter_state` 把各组件的状态合成 Claude 适配器的 `runtime` 和 `failure`（有失败取第一个失败，否则有安装中就是安装中，全部就绪才算就绪，其余为缺失），交给索引。
- **索引**：记下安装器给的状态，合并 Claude 的 `AgentStatus` 时覆盖 `adapter.runtime` 与 `adapter.failure`；收到新状态时，若视图里已有 Claude，就重新合并一次。`AgentRuntime` 启动时报一次状态，索引从一开始就有 Claude 这一项，覆盖才有处可写。
- **下载上限**：`npm::MAX_TARBALL` 为 64 MiB，按块读取，超过即按网络失败处理（之后照常重试）。用例是 IT-runtime-download 里新加的一条（64 MiB 加 1 字节的包）。
- **IT-runtime-startup**（`tests/lifecycle.rs`）：
  - 用例在临时目录里造 `CupperHost.app/Contents`：复制宿主到 `MacOS/`，放假 Node（照 T115 的脚本，把入口复制成打包结果）、esbuild-wasm 的 `lib/main.js`、锁文件和打包脚本；由真实的 `runtime/manifest.json` 改出一份只列这些文件的清单；在本进程里起一个只返回那个 tarball 的 registry。
  - 第一段：宿主装好，索引流最新的 `agents.updated` 里 Claude 的 `adapter.runtime` 为 `ready`，`claude-agent-acp/0.9.0/adapter.mjs` 是入口的内容；SIGTERM 后退出码 0。
  - 第二段：启动前改掉包里的 `main.js`，宿主进入待升级，没有要完成的事就以退出码 0 退出；`host.updated.pending_upgrade` 为真，`adapter.failure` 为 `bundle_changed`。
  - 守卫 `Running` 在用例结束时（无论成败）杀掉还在的宿主。连跑 10 次全绿，之后没有残留进程。

## 行数（不含 notes）

| 部分 | 新增 / 删除 | 估计 |
|---|---|---|
| 下载上限与用例 | 27 / 5 | 35 |
| 安装器接线、索引覆盖、监护忙闲、测试清单、`AgentRuntime` 启动时报一次 | 约 125 / 7 | 120 |
| IT-runtime-startup | 154 / 1 | 150 |
| **合计** | **约 318 / 12** | 约 305 |
