# T075 工作笔记

- 执行者：子 agent（工程师；T004、T005、T094、T028 的执行者接手）

## 开工前的规模估计

行数指 Prettier、rustfmt 展开后的行，含注释和空行，不含锁文件。"草估"是逐模块的估计；按我前几个任务的实际偏差（T004 1.1 倍、T005 1.25 倍、T094 1.4 倍、T028 约 1.3 倍）实现乘 1.3，另列计划 1.1 的 1.7 倍作对照。

先核对了现有代码，有三处是任务书没写、但不做就跑不起来的：

1. **原生扩展缺两个函数**："重启宿主"要核对 `host.lock` 里 pid 的可执行文件路径和 cdhash，扩展现在只有 `spawnHost`、`connectHost` 等，`cupper-sys` 也没有"按 pid 取完整路径"（`process_info` 只有内核的 16 字节短名）和"按 pid 取 cdhash"（只有 `own_cdhash`）。要在 `cupper-sys` 加 `proc_pidpath` 和 `csops(pid, CS_OPS_CDHASH)`（`unsafe` 只在 sys），在扩展里导出 `processImage(pid)` 和 `fileCdhash(path)`，再补 TS 声明。
2. **App 里还没有原生扩展和宿主**：dev、e2e 两份 electron-builder 配置都没有把 `cupper_native.node` 放到 asar 外、也没有把 `target/host-bundle/<kind>/CupperHost.app` 放进 `Contents/Helpers/`；主进程也没有加载扩展的代码。计划里没有任务明确负责这一步（T160 只管自用构建和发布构建的配置）。
3. **主进程要一个基于 `net.Socket` 的 `HostPort`**：host-client 的握手和 `HostClient` 都以 `HostPort` 为接口，现在只有 mock 和渲染进程的实现。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| `cupper-sys` 与原生扩展 | `pid_path`、`pid_cdhash`；napi 的 `processImage`、`fileCdhash`；TS 声明 | 80 |
| `host/native.ts` 与打包配置 | 按构建种类找到并加载 `cupper_native.node`；`buildInfo()` 与 flavor 核对；dev、e2e 的 electron-builder 配置加上扩展（asar 外）和 `CupperHost.app`（`Helpers/`） | 60 |
| `host/socket-port.ts` | `net.Socket` 上的 `HostPort`：切 JSON 帧、`0x02` 帧、按块回调、关闭 | 50 |
| `host/launcher.ts` | 拼命令（宿主路径、`--data-dir`、环境变量白名单）；dev 和 e2e 的载荷（`HostKey`，e2e 另有 `CUPPER_E2E_DIR` 下的 harness 密钥和 `test_config`）只留内存、被杀后原样重传；每 10 ms 重连、总预算 500 ms（时钟可注入）；退出码 3 改为直接连接，4–6 并入 `failed.reason`；更新空窗期间不拉起的开关 | 150 |
| `host/connection.ts` | `connectHost` → `takeFd` → `SocketPort` → host-client 的 `handshake`；`auth`、`verifyHostProof` 的胶水；失败时 `conn.close()`；`connectHost` 的 `code` 并入 `failed.reason`；发布构建拒绝 `weak_auth`；`AbortSignal.timeout` 加上套接字关闭时 abort | 100 |
| `host/control.ts` | control 连接：`HostClient` 挂在上面，断线后经 launcher 和 connection 重连 | 70 |
| `host/lifecycle.ts` | 退出前查 `host/status`，有运行中的回合或待处理的请求时弹原生确认框（调用时才取 `dialog.showMessageBox`），确认后 `host/exit`；`powerMonitor` 的 `shutdown`、`before-quit-for-update` 跳过 | 90 |
| `host/restart.ts` | "无法验证宿主"的提示；读 `host.lock` 的 pid，经扩展核对路径和 cdhash 属于本构建的宿主才发 SIGTERM，不属于就给提示 | 60 |
| `@cupper/i18n` | 语义码和中英文案：退出确认、数据来自更新的版本、数据目录不可用、迁移失败（带备份位置）、无法验证宿主、需要重启宿主、pid 不属于本构建 | 40 |
| `main/index.ts` 接线 | 启动时拉起并连接；生命周期事件 | 50 |
| **实现小计** | | **750**；乘 1.3 约 **975**（乘 1.7 约 1,275） |

**测试**

| 项 | 内容 | 草估 |
|---|---|---|
| UNIT-launcher（假扩展、假时钟） | 载荷只在内存、被杀后原样重传且复用同一个 `HostKey`；500 ms 预算；退出码 3–6；`weak_auth` 在发布构建被拒；关机跳过确认；pid 的路径不符、cdhash 不符时不发信号 | 300 |
| `SocketPort` 与握手胶水 | 对 mock 宿主或 socketpair：帧、关闭时 abort、`host_proof` 缺失时 `close()` | 60 |
| 原生扩展的新函数 | Rust 用例 + Node 用例（自己的进程、自己拉起的进程） | 45 |
| 真实拉起 | 用 `cargo xtask host-bundle --kind e2e` 的产物，经扩展拉起并握手（Node 集成用例，不进 `pr`） | 80 |
| **测试小计** | | **485** |

**合计约 1,460 行**（实现按 1.3 倍）到 **1,760 行**（实现按 1.7 倍），明显超出约 1,000 行的汇报线。按任务书和守则第 8 条，开工前停下来汇报，没有写代码。

## 拆分建议

按"拉起与连接"和"退出与重启"拆。前一半挡在 T076（windowBridge 要用 launcher 和 hostConnection 建数据连接）前面，后一半不挡关键路径，可以和 T076 并行。

| 拆分 | 模块 | 用例 | 估计 |
|---|---|---|---|
| **T075a 拉起宿主与 control 连接** | `native.ts` 与 dev、e2e 打包配置；`socket-port.ts`；`launcher.ts`；`connection.ts`；`control.ts`；`index.ts` 里启动时的接线；拉起失败的语义码 | UNIT-launcher 的拉起部分（载荷与 `HostKey`、500 ms、退出码 3–6、`weak_auth`）；真实拉起的集成用例 | 实现草估约 475，乘 1.3 约 620；测试约 280；合计约 **900**（实现乘 1.7 时约 1,090） |
| **T075b 退出确认与重启宿主** | `cupper-sys` 与扩展的 `processImage`、`fileCdhash`；`lifecycle.ts`；`restart.ts`；对应的语义码和接线 | UNIT-launcher 的其余部分（关机跳过确认；路径不符、cdhash 不符时不发信号），或者另起一个编号，例如 UNIT-host-restart | 实现草估约 275，乘 1.3 约 360；测试约 180；合计约 **540** |

T075a 按 1.7 倍算仍会贴线。如果要再留余量，可以把"dev、e2e 打包配置加上扩展和宿主"连同真实拉起的集成用例挪进 T075b（约 140 行）。这样 T075a 约 760 行，T075b 约 680 行。代价是 T075a 只能用 Node 集成用例验证拉起，App 本身要到 T075b 才能真正拉起宿主。

## 收窄后的估计（补充裁定之后）

T075 只做"拉起宿主与 control 连接"，lifecycle 和"重启宿主"归 T007。e2e 启动载荷按设计 5.9.9 的约定：读 `CUPPER_E2E_DIR` 下的 `harness_key` 和 `test_config.json`。

| 模块 | 内容 | 草估 |
|---|---|---|
| `host/native.ts` | 加载 `cupper_native.node`，`buildInfo()` 与 flavor 核对 | 45 |
| `host/socket-port.ts` | `net.Socket` 上的 `HostPort` | 35 |
| `host/connection.ts` | `connectHost` 到握手的胶水；拒绝码和握手失败并入 `failed.reason`；失败时 `close()`；只有发布构建拒绝 `weak_auth`；超时加套接字关闭时 abort | 90 |
| `host/launcher.ts` | 先连；没人监听才拉起；`HostKey` 和载荷只留内存、重拉时原样再传；每 10 ms 重连、500 ms 总预算（时钟可注入）；退出码 3 继续连、4–6 并入 `failed.reason`；更新空窗不拉起 | 140 |
| `host/control.ts` | `HostClient` 挂在 control 连接上，断开后重连（宿主没了就重拉） | 60 |
| `host/index.ts` 与 `main/index.ts` | App 内的路径（`Helpers/CupperHost.app`、`Resources/native/`）、环境变量白名单（5.6.6 的固定白名单）、e2e 载荷文件、启动时接线、状态写 `main.log` | 80 |
| 打包配置 | dev、e2e 的 electron-builder：扩展放 `Resources/native/`（asar 外），宿主放 `Helpers/` 并 `signIgnore`；`build:dev`、`build:e2e` 先编扩展和 `host-bundle`；依赖与 tsconfig 引用 | 30 |
| **实现小计** | | **480**，乘 1.3 约 **625**（乘 1.7 约 815） |

| 测试 | 内容 | 草估 |
|---|---|---|
| UNIT-launcher（假扩展、假时钟、mock 宿主） | 载荷只在内存、重拉时原样再传且复用同一个 `HostKey`；500 ms 预算；退出码 3–6；发布构建拒绝 `weak_auth`，其他构建接受 | 220 |
| 真实拉起的集成用例 | 用真实的 `dev-auth` 扩展和 `cargo xtask host-bundle --kind e2e --debug` 的产物，拉起、握手、`ping`、再结束宿主；单独的脚本，不进 `pnpm test` | 80 |
| **测试小计** | | **300** |

**合计约 925 行**（实现乘 1.7 时约 1,115）。每完成一个模块统计一次；推算超过约 1,000 行就停下汇报。

## 实际行数（逐模块）

`git diff --numstat`，不含 `pnpm-lock.yaml` 和本目录。

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计（实现 + 测试） | 累计新增 |
|---|---|---|---|---|
| 扩展加载、`SocketPort`、`hostConnection`、`hostLauncher`、UNIT-launcher（`feat(desktop)`） | 475 / 1 | 191 | 310 + 220 | 475 |
| `controlClient`、宿主的组装与启动接线、dev 和 e2e 打包配置（`feat(desktop)`） | 168 / 5 | — | 170 | 643 |
| 真实拉起的集成用例（`test(desktop)`） | 89 / 1 | 87 | 80 | 732 |

**合计新增 732 行、删除 7 行**，其中测试 278 行。估计是约 925 行，实际少两成：主要是 lifecycle 一类代码挪出去以后，组装比预想的短，`HostClient` 也直接拿来做 control 客户端。

## 实现时的取舍

**分层**：
- `connection.ts` 只管"一个扩展校验过的连接 → `SocketPort` → 握手"。
- `launcher.ts` 管拨号、拉起、重试、退出码，以及 `HostKey` 和载荷。
- `control.ts` 把 `HostClient` 挂上去，连接断开后重连。
- launcher 的拨号由它自己调用扩展的 `connectHost`：`connect_failed`（没人监听）才拉起，其余拒绝码直接并入 `failed.reason`。单测里把"握手"这一步换成假的；真实的套接字和握手由集成用例覆盖。

**`failed.reason` 新增的语义码**：
- `data_too_new`、`data_dir_unusable`、`migration_failed`（退出码 4–6）；
- `host_exited`（其他退出码或信号）；
- `host_unreachable`（500 ms 预算用完）；
- `updating`（安装更新期间不拉起）；
- `spawn_failed`；
- 另外照原样并入扩展的拒绝码和握手失败码。

**`HostKey` 和载荷**：
- 每个 launcher 一个 `HostKey`，在构造时生成（keyed 构建）。
- 载荷在第一次拉起时读一次，之后一直用内存里的同一个对象。
- 宿主被杀后再拉起，传的是同一个对象和同一个 key（单测断言同一引用、读载荷只读了一次；集成用例真的 `SIGKILL` 宿主后再拉起）。

**发布构建与 `weak_auth`**：
- 只有 `release` 拒绝 `weak_auth`。自用构建（`local`）走 `dev_key`，按 5.7.9 本来就是退化模式，界面提示"本地鉴权较弱"，所以不拒绝。
- 发布构建握手用 `{type: "codesign"}`，没有 `host_proof` 要核对。

**打包**：
- `build:dev`、`build:e2e` 先编 `dev-auth` 的扩展，release 配置，编到 `target/host-bundle/build`，与 `host-bundle` 共用依赖产物；再跑对应种类的 `host-bundle`；最后才是 Vite 和 electron-builder。
- 扩展放在 `Resources/native/cupper_native.node`，用 `extraResources`，在 asar 外。设计 5.10.1 写的是 `app.asar.unpacked/…`；`extraResources` 少一步暂存复制，也同样在 asar 外。
- 宿主用 `extraFiles` 放进 `Contents/Helpers/CupperHost.app`，并加 `signIgnore`。实测打包后宿主的 cdhash 与 `host-bundle` 的产物相同，整个 App 的 `codesign --verify --strict --deep` 通过。
- 前提：先跑过一次 `cargo xtask runtime-pack`（`host-bundle` 要它的暂存目录）。

**宿主的环境变量**：用 5.6.6 的固定白名单，从 App 自己的环境里取（PATH、HOME、USER、LOGNAME、SHELL、TMPDIR、LANG、`LC_*`、TERM、代理变量两组、SSH_AUTH_SOCK）。和 `runtime/manifest.json` 的 `env.allow` 是同一份名单。桌面没有读清单，在代码里抄了一份，注释注明出处。

## 实测

- **集成用例**（`pnpm --dir apps/desktop test:launch`）：
  - 用真实的 `dev-auth` 扩展和 `host-bundle --kind e2e --debug` 的产物，经 launcher 拉起、握手（`weak_auth` 为 true，`hostPid` 与 `host.lock` 一致）、`ping`；
  - `SIGKILL` 宿主后，launcher 用同一个 key 和载荷再拉起、再握手。
  - 连跑 3 次都通过，结束后没有残留宿主进程；临时目录在用例结束时删除。
- **打包后的 e2e App**：设好 `CUPPER_E2E_DIR`（夹具文件由我写），启动 App 两次。
  - 两次 `main.log` 里都是 `host connection` → `ready`，`host.lock` 里的 pid 是 `cupper-host`。
  - 每次结束时都对 App 和那个宿主发了 SIGTERM，结束后没有残留进程。
  - 真实的 `~/Library/Application Support`、`~/Library/Logs` 下没有新建任何东西。
  - Chromium 照例改写了 `~/Library/Preferences/dev.cupper.app.e2e.plist`（T074 已交接给 T090），我用 `defaults delete` 清空了这个域。
- **首次执行新二进制要等系统检查**：一份新复制的 `cupper-host`（debug，约 21 MB）第一次执行 `--build-info` 用了 1.9 秒，第二次几乎为 0。
  - 集成用例第一次跑时，就因为这个超过了 500 ms 预算，报 `host_unreachable`，而那个宿主随后仍会起来。用例里先跑一次 `--build-info` 预热。
  - 用户装好新版本后第一次启动，可能同样超过 500 ms，见汇报的遗留问题。
