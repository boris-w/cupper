# T007 工作笔记

- 执行者：子 agent（工程师；T075 的执行者接手）

## 开工前的规模估计

行数指 rustfmt、Prettier 展开后的行，含注释和空行，不含锁文件。实现按我近几个任务的实际偏差乘 1.3，另列 1.7 倍作对照。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| `cupper-sys` | `pid_path`（`proc_pidpath`）、`pid_cdhash`（`csops(pid, CS_OPS_CDHASH)`），`unsafe` 带 `SAFETY:` | 40 |
| 原生扩展 | napi 的 `processImage(pid)`（路径和 cdhash，进程不在时为 `null`）、`fileCdhash(path)`；TS 声明 | 45 |
| `hostLauncher` 的启动等待 | 500 ms 用完而自己拉起的宿主还活着时继续等，最长 10 s | 8 |
| 环境变量白名单只留一份 | `vite.config.ts` 在构建期读 `runtime/manifest.json` 的 `env.allow`，经 `define` 给代码；vitest 配置同样；删掉 `host/index.ts` 里的副本 | 20 |
| `host/lifecycle.ts` | `before-quit` 时：关机或 `before-quit-for-update` 直接放行；否则查 `host/status`，有运行中的回合或待处理的请求时弹原生确认框（调用时才取 `dialog.showMessageBox`），确认后 `host/exit`（`force`），取消则不退出 | 70 |
| `host/restart.ts` | 读 `host.lock` 的 pid；经扩展取它的路径和 cdhash，与本构建的宿主（App 里的 `cupper-host`，realpath 后比较；cdhash 用 `fileCdhash`）都相符才发 SIGTERM；等它退出后重连 | 60 |
| `@cupper/i18n` | 新的 `host` 命名空间：退出确认、需要重启宿主、不是本构建的宿主 | 17 |
| `main/index.ts` 接线 | `before-quit`、`powerMonitor` 的 `shutdown`、`before-quit-for-update`；keyed 构建 control 失败码为 `unauthenticated` 或 `host_unverified` 时提示重启宿主 | 40 |
| **实现小计** | | **300**，乘 1.3 约 **390**（乘 1.7 约 510） |

**测试**

| 用例编号 | 内容 | 草估 |
|---|---|---|
| `cupper-sys` | 对测试自己拉起的签名探针取路径和 cdhash，与磁盘上的文件一致；不存在的 pid 报错 | 35 |
| 原生扩展的 Node 用例 | `processImage` 对自己拉起的夹具、`fileCdhash` 与 `codesign` 一致 | 20 |
| UNIT-launcher 补充 | 宿主活着时等到 10 s；期间开始监听则成功；期间退出按退出码；不是自己拉起的（退出码 3）仍按 500 ms | 40 |
| UNIT-host-restart | 路径不符、cdhash 不符都不发信号，两者都符才发；关机和 `before-quit-for-update` 跳过确认；只有运行中的回合或待处理请求时才弹确认框；取消则不退出 | 130 |
| **测试小计** | | **225** |

**合计约 615 行**（实现乘 1.7 时约 735），在约 800 行的汇报线以内。每完成一个模块用 `git diff --numstat` 统计一次。

## 实际行数（逐模块）

`git diff --numstat`，不含 `pnpm-lock.yaml` 和本目录。

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计（实现 + 测试） | 累计新增 |
|---|---|---|---|---|
| `cupper-sys` 的 `pid_path`、`pid_cdhash`；扩展的 `processImage`、`fileCdhash`（`feat(sys)`） | 97 / 13 | 27 | 85 + 55 | 97 |
| 启动等待（`feat(desktop)`） | 43 / 9 | 34 | 8 + 40 | 140 |
| 环境变量白名单只留一份（`refactor(desktop)`） | 26 / 25 | — | 20 | 166 |
| `lifecycle`、重启宿主、对话框、`host` 命名空间、接线（`feat(desktop)`） | 350 / 16 | 97 | 187 + 130 | 516 |
| 集成用例：重启真实宿主、去掉预热（`test(desktop)`） | 53 / 24 | 53 | — | 569 |

**合计新增 569 行、删除 87 行**，其中测试约 210 行。估计是约 615 行，在约 800 行的汇报线以内。

## 实现时的取舍

**按 pid 取路径和 cdhash**：
- `pid_path` 用 `proc_pidpath`，返回解析过链接的路径；
- `pid_cdhash` 用 `csops(pid, CS_OPS_CDHASH)`，`own_cdhash` 改为调用它。
- 扩展的 `processImage(pid)` 在进程不存在时返回 `null`；`fileCdhash(path)` 失败时抛 `invalid_argument`。
- 路径和 cdhash 是先后两次读取，pid 在两次之间被复用的可能性极小；两者都要与本构建的宿主相符才发信号，所以复用了也只会"不发"。

**重启宿主的判定**：
- 本构建的宿主是 App 里的 `Contents/Helpers/CupperHost.app/Contents/MacOS/cupper-host`：路径取 realpath，cdhash 由扩展的 `fileCdhash` 读。
- `host.lock` 里的 pid 先过 `processImage`，路径和 cdhash 都相同才发 SIGTERM。
- 没有锁文件、内容不是数字、pid 不大于 1、进程已不存在，都算 `no_host`，直接重连。
- 发了信号以后，每 50 ms 查一次，最多等 10 s 让旧宿主有序退出（它要先关适配器，5.2.13），然后重连；launcher 会用本实例的 key 拉起新宿主。
- 触发条件：keyed 构建的 control 连接以 `unauthenticated` 或 `host_unverified` 失败。确认框由主进程直接弹（原生对话框），界面任务以后可以改走 ShellAPI。

**退出确认**：
- `QuitGuard` 挂在 `before-quit`，在 `will-quit` 之前：没有定论时先拦下，查 `host/status`（2 s 超时），有运行中的回合或待处理的请求才弹确认框；确认后 `host/exit`（`force: true`，3 s 超时），然后再 `app.quit()`；取消则留下。
- 查不到状态（没连上、超时）时直接退出：宿主会自己决定。
- 没有要中断的东西时不发 `host/exit`，宿主按空闲规则退出，这样不会断掉同时连着的 CLI。设计只写了"确认后发 `host/exit`"，我照字面实现。
- 同时触发两次退出时只弹一个确认框。
- `powerMonitor` 的 `shutdown` 和 Electron `autoUpdater` 的 `before-quit-for-update` 调用 `skip()`。更新流程还没有实现，所以后者现在只是挂好了钩子。
- 单实例锁沿用 T074 在 `main/index.ts` 里已有的实现，没有挪进 lifecycle。

**对话框**：`host/dialogs.ts` 在调用时才读 `dialog.showMessageBox`；文案是 `@cupper/i18n` 新增的 `host` 命名空间（中文）。

**启动等待**：
- 只要自己拉起的宿主还活着，就把截止时间从 500 ms 延到 10 s。
- 宿主以退出码 3 退出（锁在别的宿主手里）后，仍按 500 ms 算。

**环境变量白名单**：`vite.config.ts` 在构建期读 `runtime/manifest.json` 的 `env.allow`，经 `define` 成为 `__CUPPER_HOST_ENV_ALLOW__`；vitest 配置调用同一个函数。核对过构建出的 `dist/main/index.js` 带着这份名单，桌面源码里不再有副本。

## 实测

- **集成用例**（`pnpm --dir apps/desktop test:launch`），2 条都通过：
  - 原来的"拉起、握手、`ping`、被杀后再拉起"；
  - 新增的"用另一把 key 连上去被拒为 `unauthenticated`，按 `host.lock` 核对为本构建的宿主后停掉，再拉起"。
  - 把 bundle 复制到新位置再跑：第一条用例用了 1.9 秒（冷启动等系统检查），靠 10 s 的等待成功，所以去掉了 T075 加的预热。
  - 结束后没有残留宿主进程。
- **打包后的 e2e App**：重新 `build:e2e`，启动后 `main.log` 到 `ready`；用 Apple event 让它退出。宿主空闲，没有弹确认框，App 正常退出，control 连接关闭。
  - 宿主按设计不随 App 退出（空闲 10 分钟后自己退出），我对它发了 SIGTERM。
  - 清空了 `dev.cupper.app.e2e` 的偏好设置域，临时目录已删。
