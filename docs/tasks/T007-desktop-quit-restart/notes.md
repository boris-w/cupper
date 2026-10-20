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
