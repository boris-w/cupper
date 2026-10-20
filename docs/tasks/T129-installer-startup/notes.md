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
