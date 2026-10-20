# T129 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增约 318 行、删除 12 行（测试约 164），在约 400 的停止线以内。
- **CI**：执行者的摘要 `pr-e92ce9141465.json` 通过。合并 main（含 T029、T046、T086、T164）没有冲突，clippy 干净；之后在本 worktree 重跑 `cargo xtask ci pr --record`，`pr-45f5ec22a78e.json` 通过（rust 1860、protocol 185、ts 436、license、testdata-scan；ui 被路径过滤跳过）。

## 逐项意见

1. **只在 App 包里运行安装器**（`BundleDir::is_app()`）：开发和测试用的 `target/debug/cupper-host` 不在包里，不启动安装器，否则这类宿主会因为"包里缺文件"立刻进入待升级并退出。判断合理。
2. **监护接线**：
   - 有组件在 `Installing` 时 `installer_busy` 为真，空闲退出要等它；
   - 任一组件 `Failed(BundleChanged)` 时转入待升级；
   - 适配器的 `runtime`、`failure` 由各组件状态合成。
   - 用的正是 T059 留的接入点。
3. **静止屏障用 actor 种类**：包进 `Participant::idle`，等下载、子进程、定时器时算空闲。用 `Hold` 会让 `test/clock_advance` 一直等到下载结束。理由成立。
4. **测试清单注入点**（`test_config.runtime.manifest`）：只在 test-harness 构建里生效，与已有的 `registry_url` 属同一类。内嵌清单钉的是真实 Node 的哈希，测试造不出，这个注入点是必要的。发布构建不受影响。
5. **下载大小上限**：按块读取，超过 64 MiB 按网络失败处理；有用例（64 MiB 加 1 字节）。关闭了 T115 评审的遗留。
6. **IT-runtime-startup**：用假的 `CupperHost.app`（宿主副本、假 Node、esbuild-wasm、锁文件、打包脚本）加本地 registry，装到 `ready`；包内文件被改时转入待升级并以 0 退出。守卫保证不留宿主进程；连跑 10 次全绿。

## 遗留（记交接）

- **`in_use` 为空**：启动时没有适配器在跑。以后要在运行期间重新安装时，需要传入运行中适配器的版本。
- **安装器没有运行期间的重启入口**：运行时坏了只在下一次宿主启动时重新核对。
