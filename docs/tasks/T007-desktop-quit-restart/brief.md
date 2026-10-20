# T007 桌面主进程 II-b：退出确认与重启宿主

- 派发日期：2026-10-20
- 执行者：子 agent（工程师）。T075 的执行者接手
- 分支：`feat/T007-desktop-quit-restart`
- 依赖：T075（合并中）。T090（e2e 基础设施，E2E-quit-confirm）依赖本任务

## 背景

T075 开工前拆成两部分（计划 v1.3.17，见 `docs/tasks/T075-desktop-host-launch/brief.md` 末尾的补充裁定）：T075 做了拉起宿主与 control 连接；本任务做退出确认与"重启宿主"，以及 T075 交回后编排者定下的两处改动。

## 范围

**做**：
1. **`cupper-sys` 与原生扩展的新函数**：`cupper-sys` 加按 pid 取可执行文件完整路径（`proc_pidpath`）和按 pid 取 cdhash（`csops(pid, CS_OPS_CDHASH)`）；扩展导出 `processImage(pid)`、`fileCdhash(path)`，补上 TS 声明。`unsafe` 只在 `cupper-sys`，每处带 `SAFETY:` 注释。
2. **`lifecycle`**（设计 5.7.2）：单实例锁；退出前经 control 连接查 `host/status`，有运行中的回合或待处理的请求时弹原生确认框（NFR-8），确认后发 `host/exit`；注销或关机（`powerMonitor` 的 `shutdown`）和 `before-quit-for-update` 时跳过确认。挂在现有的 `will-quit` 之前（T075 的交接）。`ControlConnection.client` 就是 control 连接上的 `HostClient`。
3. **"无法验证宿主"与"重启宿主"**（退化模式，5.8.1）：新的 UI 实例连不上还在运行的宿主（失败码 `unauthenticated` 或 `host_unverified`）时提示"需要重启宿主"；确认后向 `host.lock` 里记录的 pid 发 SIGTERM。**发信号之前核对这个 pid 的可执行文件路径和 cdhash 属于本构建的宿主**（安全 #9），不属于就不发，给出提示。
4. **启动等待**（T075 实测、编排者裁定，已写进设计 5.7.2、5.5.1）：新装的宿主二进制第一次执行要等系统检查约 2 s，超过 500 ms 的连接预算。改 `hostLauncher`：预算用完时，只要自己拉起的宿主还活着，就继续等它开始监听或退出，最长 10 s；宿主退出就按退出码处理，10 s 到了才给 `host_unreachable`。
5. **宿主的环境变量白名单只留一份**：T075 在桌面代码里抄了一份 5.6.6 的固定白名单。改为构建时从 `runtime/manifest.json` 的 `env.allow` 读取（例如 Vite 在构建期导入这个 JSON），代码里不再另抄。

**不做**：`windowBridge`、数据连接（T076）；界面上的提示页面（界面任务，本任务只给语义码，文案走 `@cupper/i18n`）。

## 验收标准

- [ ] 新增 UNIT-host-restart：pid 的路径不符、cdhash 不符时都不发信号；两者都符时才发；关机和 `before-quit-for-update` 跳过退出确认；有运行中的回合或待处理请求时才弹确认框。
- [ ] UNIT-launcher 补上启动等待：预算用完、宿主还活着时继续等，宿主随后开始监听则成功，宿主退出则按退出码，10 s 到了才失败（虚拟时间）。
- [ ] `cupper-sys` 的新函数有用例（对测试自己拉起的进程取路径和 cdhash，与磁盘上的文件一致）。
- [ ] 代码约束：主进程在调用时才取 `dialog.showMessageBox` 这类函数，不在模块加载时解构保存；界面文案一律走 `@cupper/i18n`。
- [ ] 桌面代码里不再有宿主环境变量白名单的副本。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T007 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T007-desktop-quit-restart/ci/` 通过，摘要随任务提交。

## 注意

- 发信号的用例只能向测试自己拉起的进程发，绝不向测试之外的进程发信号；用例结束后确认没有残留进程。
- 用例里的数据目录、日志目录都用临时目录，不碰真实的 `~/Library` 目录；Chromium 改写的 `~/Library/Preferences/dev.cupper.app.e2e.plist` 按 T074 的交接处理。
- 依赖锁精确版本，许可证在 ADR-11 的 allowlist 内。

## 规模

计划估为 M，T075 的执行者估约 540 行，加上第 4、5 项。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 800 行（包括贴线）时，必须先停下来汇报**。

## 参考资料

- 设计：`07-desktop.md` 5.7.2（`lifecycle`、`hostLauncher`）；`05-flows.md` 5.5.1；`08-security.md` 5.8.1（退化模式的局限）；`02-host.md` 5.2.13（`host/status`、`host/exit`）。
- `docs/tasks/T075-desktop-host-launch/`、`T073-desktop-native/`、`T074-desktop-main-skeleton/`。

## 工作方式

T075 合并之后，在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b feat/T007-desktop-quit-restart main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
