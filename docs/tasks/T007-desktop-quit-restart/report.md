# T007 汇报

- 执行者：子 agent（工程师；T075 的执行者接手）
- 分支：`feat/T007-desktop-quit-restart`
- 完成日期：2026-10-20

## 做了什么

T007 已完成，`cargo xtask ci pr` 全部通过。

1. **`cupper-sys` 与原生扩展**：
   - `cupper-sys` 新增 `pid_path`（`proc_pidpath`，返回解析过链接的路径）和 `pid_cdhash`（`csops(pid, CS_OPS_CDHASH)`）。`own_cdhash` 改为调用 `pid_cdhash`。`unsafe` 都在 `cupper-sys` 里，每处带 `SAFETY:` 注释。
   - 扩展导出 `processImage(pid)`（进程不存在时为 `null`）和 `fileCdhash(path)`（失败时抛 `invalid_argument`），并补上 TS 声明。
2. **启动等待**（`hostLauncher`）：500 ms 用完时，只要自己拉起的宿主还活着，就继续等它开始监听，最长 10 s。期间宿主退出则按退出码处理。宿主以退出码 3 退出（锁在别的宿主手里）后，仍按 500 ms 算。
3. **宿主环境变量白名单只留一份**：`vite.config.ts` 在构建期读 `runtime/manifest.json` 的 `env.allow`，经 `define` 交给主进程（vitest 配置调用同一个函数）。`host/index.ts` 里的副本已删掉。我核对过构建出的 `dist/main/index.js` 带着这份名单。
4. **`lifecycle`（`host/lifecycle.ts` 的 `QuitGuard`）**：
   - 挂在 `before-quit` 上，先于 `will-quit` 执行；
   - 先查 `host/status`（2 s 超时），只有运行中的回合或待处理的请求不为零时才弹原生确认框；
   - 确认后发 `host/exit`（`force: true`，3 s 超时）再退出；取消则不退出；
   - 同时触发两次退出时只弹一个确认框；
   - `powerMonitor` 的 `shutdown` 和 `autoUpdater` 的 `before-quit-for-update` 直接放行，不确认。
5. **"无法验证宿主"与"重启宿主"**（`host/restart.ts` 的 `stopLockedHost`，加上 `startControl` 返回的 `restart`）：
   - 触发条件：keyed 构建的 control 连接以 `unauthenticated` 或 `host_unverified` 失败，这时弹原生确认框。
   - 确认后读 `host.lock` 里的 pid，经扩展取它的路径和 cdhash。只有两者都与本构建的 `cupper-host` 相同（路径取 realpath，cdhash 用 `fileCdhash`）才发 SIGTERM；不同就不发，并弹出说明。
   - 发信号后最多等 10 s 让旧宿主有序退出，然后重连，launcher 会拉起新宿主。
6. **对话框和文案**：`host/dialogs.ts` 在调用时才读 `dialog.showMessageBox`；文案在 `@cupper/i18n` 新增的 `host` 命名空间（退出确认、需要重启宿主、拒绝发信号的说明）。

## 关键取舍

- **退出时的 `host/exit`**：只在确认之后发，照设计字面实现。没有要中断的东西时不发，宿主按空闲规则自行退出，这样不会断掉同时连着的 CLI。
- **查不到 `host/status`**（没连上或超时）时直接退出，不确认。
- **触发重启的确认框由主进程直接弹**：界面任务和 ShellAPI 还没有，界面上的提示页面归界面任务，以后可以改走 ShellAPI。
- **判定"本构建的宿主"**：路径和 cdhash 必须同时相符。路径相同但 cdhash 不同（例如重新构建过）也不发信号。两次读取之间 pid 被复用的话，只会导致"不发"，不会误杀别的进程。
- **单实例锁**：沿用 T074 在 `main/index.ts` 里已有的实现，没有挪进 lifecycle。
- **规模**：新增 569 行、删除 87 行，其中测试约 210 行；估计约 615 行，汇报线 800。这次每完成一个模块统计一次，逐模块明细在 `notes.md`。

## 验证方式

- [x] **UNIT-host-restart**（4 条，在 `pr` 里）：
  - 路径不符、cdhash 不符时都不发信号，两者都符才发；锁文件为空、不是数字、pid 为 1、进程不在时都算"没有宿主"；
  - 只有运行中的回合或待处理请求时才弹确认框，确认后 `host/exit` 带 `force`；查不到状态时直接退出；
  - 取消则不退出，同时两次退出只弹一次确认框；
  - 关机和更新时跳过确认。
- [x] **UNIT-launcher 补上启动等待**（虚拟时间）：
  - 宿主活着、一直不监听时，到 10,000 ms 才失败；
  - 2,000 ms 时开始监听则成功；
  - 3,000 ms 时以退出码 5 退出，给 `data_dir_unusable`；
  - 退出码 3 后仍按 500 ms 失败。
- [x] **`cupper-sys` 的新函数**：对测试自己拉起的签名探针，取到的路径和 cdhash 与磁盘上的文件一致；不存在的 pid 报错。扩展的 Node 用例（算在 UNIT-native 名下）对自己拉起的夹具核对了 `processImage` 和 `fileCdhash`，宿主退出后 `processImage` 返回 `null`。
- [x] **集成用例**（`pnpm --dir apps/desktop test:launch`），2 条都通过：
  - 新增：先用一把 key 拉起 e2e 宿主；另一把 key 连上去被拒为 `unauthenticated`；`stopLockedHost` 核对为本构建的宿主后发 SIGTERM；再拉起成功。
  - 把 bundle 复制到新位置再跑：冷启动等了约 1.9 秒，靠 10 s 的等待成功，所以去掉了 T075 加的预热。
  - 只向自己拉起的宿主发信号，没有残留进程。
- [x] **打包后的 e2e App**：重新 `build:e2e`，启动后连接到 `ready`；用 Apple event 退出时，宿主空闲，没有弹框，App 正常退出。宿主按设计不随 App 退出，我对它发了 SIGTERM。用的都是临时目录，并清空了 `dev.cupper.app.e2e` 的偏好设置域。
- [x] 代码约束：调用时才读 `dialog.showMessageBox`，文案走 `@cupper/i18n`；桌面代码里没有白名单副本。
- [x] 测试名以编号开头（`describe("UNIT-host-restart")` 等）。
- [x] 在干净的 worktree 里运行 `cargo xtask ci pr --record docs/tasks/T007-desktop-quit-restart/ci/`，全部通过：
  - rust 1481 / protocol 185 / ts 334 / license 1 / testdata-scan 3；
  - ui、e2e 尚未实现；
  - UNIT-host-restart、UNIT-launcher、UNIT-native 都是 pass；摘要为 `pr-35d720bf4cc8.json`，已提交。
  - testdata-scan 的 `sensitive-words` 步骤因本机没有词表被跳过，与本任务无关。
- [x] grep 过分支的 diff 和提交信息，没有本机路径或用户名。

## 提交列表

```
19481e7 docs(task): record the T007 pr CI summary
35d720b docs(task): record T007 sizes, trade-offs and measurements
8de9e59 test(desktop): restart a real host another key started; drop the warm-up
6e16f72 feat(desktop): confirm quitting while turns run, and restart a host the app cannot verify
06c8324 refactor(desktop): take the host's environment allowlist from the runtime manifest
c2db096 feat(desktop): wait up to 10 s for a host we started that is still starting
c9e0b02 feat(sys): read the executable and cdhash a pid runs; export them to the desktop
c54e1cc docs(task): estimate T007 by module before starting
```

## 遗留问题 / 范围外发现

1. **T090（E2E-quit-confirm）要注意**：
   - 真实地弹出退出确认，需要一个运行中的回合或待处理的请求（例如用假 agent），再在主进程里替换 `dialog.showMessageBox`；
   - 关机路径可以经 `powerMonitor` 的 `shutdown` 事件触发。
2. **请确认"空闲时退出 App 不发 `host/exit`"**：现在宿主在 App 退出后要等空闲规则（10 分钟）才退出。如果希望 App 退出时宿主立刻跟着退出（例如发 `host/exit` 且不带 `force`），只需改一处。我按设计字面没有这样做，因为那样会断掉同时连着的 CLI。
3. **更新流程任务要注意**：`before-quit-for-update` 现在只是挂好了跳过确认的钩子；更新流程实现以后，要确认它确实经 Electron 的 `autoUpdater` 发出这个事件。
4. **界面任务要注意**：重启宿主的确认目前是主进程弹的原生框。界面做"需要重启宿主"页面时，可以改为经 ShellAPI 触发 `startControl` 返回的 `restart`。
