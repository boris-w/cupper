# T129 汇报

- 执行者：子 agent（Rust 工程师）
- 分支：`feat/T129-installer-startup`（T054 worktree）
- 完成日期：2026-10-20

运行时安装器已接进宿主启动：在 app 包里运行的宿主，启动后在后台安装运行时，状态写进索引流的 `agents`，包内文件被改时转入待升级。验收全部满足，IT-runtime-startup 连跑 10 次全绿、没有残留进程，`cargo xtask ci pr` 全部通过。改动新增约 318 行、删除 12 行，在约 400 的停止线以内。

## 做了什么

1. **构造与启动**：在 `Host::start` 里、索引启动之后构造 `Installer`，参数照 T115 的交接：
   - `plans` 取内嵌清单；`test-harness` 构建里有 `test_config.runtime.manifest` 时改用这份测试清单；
   - `bundle` 取 `BundleDir::of_this_host()`，`dir` 取 `runtime_dir`；
   - `in_use` 为空（启动时还没有适配器在跑）；
   - `registry` 取 `test_config.runtime.registry_url`，没有就用 `npm::REGISTRY`；
   - `proxy` 取登录 shell 的 `HTTPS_PROXY`，另加 `durations`。

   位置在 `Watch::new()` 和自检设置之后，在后台运行。
2. **只在 app 包里运行**：新增 `BundleDir::is_app()` 判断宿主是否在 `Contents` 目录下。开发和测试用的 `target/debug/cupper-host` 不在包里，不启动安装器。否则这类宿主都会因为"包里缺文件"立刻进入待升级并退出。`--self-test` 的行为不变。
3. **状态与监护**：
   - 有组件处于 `Installing` 时，监护观察到的 `installer_busy` 为真，空闲退出要等它。
   - 任一组件 `Failed(BundleChanged)` 时调 `Watch::bundle_changed()`，转入待升级。
   - `runtime::adapter_state` 把各组件的状态合成 Claude 适配器的 `runtime` 和 `failure`。索引记下这个结果，合并 Claude 的 `AgentStatus` 时覆盖这两个字段。
   - `AgentRuntime` 启动时报一次状态，这样索引从一开始就有 Claude 这一项，覆盖才有地方写。
4. **下载大小上限**：`npm::download` 改为按块读取，超过 `MAX_TARBALL`（64 MiB）就按网络失败处理。
5. **静止屏障**：登记了 actor 种类 `runtime_installer`，把整个 `run` 包进 `Participant::idle`。理由见下一节。

## 关键取舍

- **静止屏障用 actor 种类，不用 `Hold`**：`Hold` 会让 `test/clock_advance` 一直等到下载结束。包进 `Participant::idle` 后，安装器在下载、打包子进程、文件工作和重试定时器上等待时算空闲，被唤醒后到下一次等待之间算忙，符合"等外部 I/O 不在屏障范围内"。这个种类无论宿主在不在包里都会登记。
- **进程级用例需要测试清单**：内嵌清单钉的是真实 Node 等文件的哈希，测试造不出与之相符的包。所以新增了只在 `test-harness` 构建里生效的 `test_config.runtime.manifest`，和 `registry_url` 属同一类注入点。包的位置照旧从宿主可执行文件的路径推出，用例把宿主复制进一个假的 `Contents/MacOS/`。

## 验证方式

- **IT-runtime-startup**（进程级）：用例造一个假 `CupperHost.app`，内含宿主副本、假 Node、esbuild-wasm 文件、锁文件和打包脚本，再从真实清单改出一份测试清单，并在测试进程里起一个本地 registry。
  - 第一段：宿主装好，索引流最新的 `agents.updated` 里 `adapter.runtime` 为 `ready`；安装出的 `adapter.mjs` 内容正确；SIGTERM 后退出码 0。
  - 第二段：启动前改掉包里的一个文件，宿主转入待升级，`host.updated.pending_upgrade` 为真，`adapter.failure` 为 `bundle_changed`；因为没有要完成的事，以退出码 0 退出。
  - 守卫 `Running` 在用例结束时（无论成败）杀掉还在的宿主。连跑 10 次全绿，之后没有残留进程。
- **下载上限**：IT-runtime-download 新增一条用例，64 MiB 加 1 字节的包按网络失败处理。
- **其他**：不开 feature 和 `--features test-harness,dev-auth` 两种 clippy 都无告警，host 全部测试通过。
- **CI**：在干净的工作区里 `cargo xtask ci pr --record docs/tasks/T129-installer-startup/ci/` 通过：rust 1851、protocol 185、ts 424、license、testdata-scan 全部通过，跳过了 sensitive-words 和截图回归。第一次运行时 license 作业因为拉取 RustSec 漏洞库遇到 HTTP/2 网络错误而失败，重跑后通过。摘要是 `pr-e92ce9141465.json`，里面有 IT-runtime-startup。

## 提交列表

```
c927702 docs(task): T129 CI summary
e92ce91 docs(task): T129 notes
174edf5 feat(host): the runtime installer runs at startup
dea55a5 feat(host): runtime downloads stop past 64 MiB
a32b949 docs(task): T129 size estimate
```

规模（相对分支起点，不含 docs）：新增约 318、删除 12，其中测试约 164。

## 遗留问题 / 范围外发现

- **`in_use` 目前为空**：启动时还没有适配器在跑，所以没有"运行中的版本"要保留。以后如果要在运行期间重新安装，需要把运行中适配器的版本传进来。
- **安装器没有重启入口**：运行时坏掉后，安装器只在下一次宿主启动时重新核对；运行期间如何重新触发，不在本任务范围。