# T225 重连途中或包被换掉后退出 App 会卡住

- 派发日期：2026-11-03
- 执行者：子 agent（桌面主进程工程师）
- 分支：`fix/T225-quit-during-reconnect`
- 依赖：T160（发现）。**用户安装 Cupper Local 之前合并**

## 背景

T160 在 runner 上跑 E2E-selfuse-replace 时发现（`docs/tasks/T160-selfuse-build/report-r2.md` 以及之后的汇报，nightly https://github.com/boris-w/cupper/actions/runs/41586879127 ）：
- 宿主因待升级有序退出后，旧界面的 control 连接和窗口连接立刻重连，从磁盘拉起宿主；
- 测试在这次重连途中给 App 发 SIGTERM（相当于 ⌘Q）；
- 30 s 后 App 仍在：主程序、gpu-process、network service，以及重连拉起的宿主；control 连接一直开着，直到第二个 SIGTERM 把进程杀掉（Electron 只把第一个 SIGTERM 当优雅退出）。
- 第一次运行里重连拉起的宿主都以退出码 3 失败，界面停在 `failed: host_unreachable`，App 同样没退出。所以不只是"重连成功后没关"。

T160 在代码上看到的缺口（没有改，也没有证实是卡住的原因）：
- `ControlConnection.end()` 不取消进行中的 `#connect()`；
- `launcher.open()` 返回后，`#connect()` 不检查 `#ended` 就 `attach`；
- `HostClient.attach()` 对已经 `close()` 的客户端，也会把状态改回 ready。

这与升级无关：宿主崩溃后正在重连时按 ⌘Q，也会碰上。

**第 3 次运行之后，范围扩大**（2026-11-03，T160 的分析，nightly https://github.com/boris-w/cupper/actions/runs/41589556442 ）：
- 那次退出时**没有任何重连在进行**（control 连接 2 s 前已经 failed），旧 App 照样 30 s 退不出。
- 三次卡住的共同点是：App 包在运行中被换掉，宿主也已经不在（第 2 次是重连后接上了新宿主）。正常的 App（三次 coexist）都能退出。
- 卡住时主进程的 JS 还在跑：原窗口已经被退出流程关掉，`second-instance` 来时 `showMainWindow` 又新建了一个窗口；main.log 显示 launcher 已经 `close()`。也就是卡在关窗之后，具体在 `will-quit` 前后还是之后的收尾，日志分辨不出来。
- 现有证据分不出是"包被换掉"还是"宿主不在"在起作用。

## 范围

**做**：
1. 查清退出卡在哪一步：`before-quit`、`will-quit` 里在等什么，是重连、某个 promise、没关掉的连接让事件循环不结束，还是包被换掉后 Electron 或主进程读不到包内的文件。main.log 里给 `before-quit`、`will-quit`、quit 各记一行，方便以后定位。给出证据（日志、确定性复现）。
2. 修主进程，必要时也改 host-client：退出开始后丢弃或取消进行中的重连，不再拉起宿主；已经关闭的客户端不再回到 ready；保证退出走完。
3. 新增回归用例 **E2E-quit-during-reconnect**（已登记，挂在 T225 下），断言 App 在限时内退出，并且没有残留由它拉起的宿主，覆盖四种情形：
   - (a) 重连途中退出（成功前、失败后）；
   - (b) 宿主被杀、包没换，然后退出；
   - (c) 包被换掉、宿主还在，然后退出；
   - (d) 包被换掉、宿主已经退出，然后退出（T160 第 3 次运行的情形）。
   涉及换包的情形要用打包后的 App，Playwright 驱动不了时照 T160 的 `apps/desktop/selfuse/` 的做法；放在 nightly 能跑的地方，只在托管 runner 上跑。
4. 顺带：退出流程开始后收到 `second-instance`，不再新建窗口。
5. 在 runner 上连跑验证，修后至少 30 次。

**不做**：改升级流程本身（收到 `host/shutdown(upgrade)` 后界面是否还自动拉起宿主，见 T160 遗留第 7 条，另议）。

## 验收标准

- [ ] 原因有证据。
- [ ] E2E-quit-during-reconnect 通过；修前在 main 上应当失败（或者说明为什么复现不了）。
- [ ] 修后 runner 上连跑全部通过，附运行链接。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：300；硬停止线：380（2026-11-03 范围扩大，原为 200 / 260）

## 参考资料

- 设计：5.7.1（进程与连接）、5.5（时序：退出、重连）、5.2.13（宿主退出）、5.8.1。
- 卷宗：T160（report-r1.md、report-r2.md）、T214（决定退出后控制连接不再重连、launcher 不再拉起宿主）、T215（握手被拒后的重启）。

## 注意

- 不运行真实的 Claude Code、Codex CLI。
- 本机只单份跑（守则第 10 条）；连跑放到 runner 上。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
