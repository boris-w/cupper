# T160 工作笔记

- 执行者：子 agent（构建与桌面工程师）

## 开工前核对到的现状

- `cargo xtask host-bundle --kind self-use` 已有（T005）：宿主 `dev.cupper.app.local.host`、特性 `dev-auth`；CLI `dev.cupper.cli.local`、特性 `self-use`，编进宿主的 cdhash；两者都开 hardened runtime。
- 桌面主进程已有 `local` 这个 flavor（`paths.ts` 的 `cupper-local`、`isReleaseLike`、原生扩展按 flavor 核对），但 **Vite 拒绝 `local` 当 mode 名**（与 `.env.local` 冲突，`flavors.test.ts` 的注释里写了），所以还没有任何办法构建出 local 的界面代码。要给 Vite 一个别的 mode 名，映射到 flavor `local`。
- dev、e2e 两份 electron-builder 配置都没有把 CLI 放进 `Contents/Helpers/cupper`；5.10.1 的包结构有它，IT-selfuse-build 也要从产物里的 CLI 读宿主 cdhash。
- electron-builder 的 `extends` 会把数组拼接（`extraFiles`、`protocols`），自用配置继承发布配置时两份宿主路径、两个 URL scheme 都会进去。所以两份配置各自完整，自用配置的文件头列出与发布配置的差异。
- electron-builder 给主程序签 `entitlements`，给其余每个文件（4 个 helper、framework、dylib、原生扩展）签 `entitlementsInherit`；`signIgnore` 的宿主和 CLI 保留 `host-bundle` 的签名，Node 保留 `runtime-pack` 的签名。
- Electron 44 的 fuse 线有 9 根：第 9 根（下标 8，Electron 的 `WasmTrapHandlers`）@electron/fuses 1.8.0 和 electron-builder 都不认识，设计 5.7.8 也没列，出厂是开启。
- Electron 自带的 Info.plist 有摄像头、麦克风、蓝牙（两项）、音频采集的用途说明；没有 `NSAppleEventsUsageDescription`。
- 宿主的 worktree 根目录默认值对所有构建都是 `~/.cupper/worktrees`；5.4.1、5.7.9 要求自用构建用 `~/.cupper-local/worktrees/`，开发构建用 `~/.cupper-dev/worktrees/`。自用构建和开发构建的宿主特性相同（都只有 `dev-auth`），宿主分不清自己是哪一种，只能像 `log_dir` 一样按数据目录的名字推出来。
- 宿主的"待升级"自检已有（`supervisor::watch::SelfCheck`）：每个新连接 accept 时、拉起适配器前比对磁盘上和运行中的 cdhash；不一致就写审计 `host.pending_upgrade`，没有回合时有序退出，`host.exiting` 的 reason 是 `upgrade`。
- 自用构建的 fuses 关掉了 `enableNodeCliInspectArguments`，Playwright 的 `_electron` 用不了，两个 E2E 只能直接启动 App，看日志、`host.lock`、进程表和 CLI。

## 规模估计（停止线 800，硬停止线 1,000）

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| 打包配置 | `electron-builder.release.yml`、`electron-builder.local.yml`、两份 entitlements | 110 |
| 构建入口 | `build:local`（desktop 和根目录）、Vite 的 mode `self-use` → flavor `local`、测试入口名单提成模块供扫描复用 | 35 |
| 宿主的 worktree 根目录 | 按数据目录推出默认值，与 `log_dir` 同一规则 | 25 |
| xtask `selfuse` 作业 | 仅在 GitHub 托管的 runner 上：构建自用、"下一版"自用（宿主和 CLI 用 dev profile，cdhash 不同）、开发构建；跑三个用例；摘要记 App 的摘要值；App 的日志进构件。注册进 nightly；`nightly.yml` 加一行，另加可选输入只跑一个作业 | 120 |
| **实现小计** | | **290** |

**测试**

| 项 | 内容 | 草估 |
|---|---|---|
| 共用的辅助 | 启动 App、按日志等就绪、`host.lock`、进程表、codesign、退出并确认没有残留；只在托管 runner 上运行的防护 | 130 |
| IT-selfuse-build | Info.plist、逐个 Mach-O 的签名标志与 entitlements（含 helper）与 identifier、`@electron/fuses` 逐项、asar 里没有测试入口、`--build-info` 与 CLI 内嵌的 cdhash、原生扩展的 `buildInfo` | 120 |
| E2E-selfuse-coexist | 与开发构建同时运行：各自的宿主、`host.lock`、数据和日志目录；helper 都起来；原生扩展加载（control 连接就绪）；开发构建退出后自用的宿主照常 | 60 |
| E2E-selfuse-replace | 装到临时目录、运行、替换成"下一版"：新 CLI 拒绝旧宿主，旧宿主写 `host.pending_upgrade` 并以 `upgrade` 退出；退出旧 App 后新 App 跑新宿主 | 80 |
| 宿主 worktree 根目录的单元测试、flavor 扫描加 `self-use` | | 25 |
| **测试小计** | | **415** |

合计约 705 行，离停止线 800 不远。按以往的偏差乘 1.15 约 810，会擦线；每个模块完成后跑一次 `cargo xtask size`，推算越过 800 就停下汇报。文档（`docs/`）不计入。

## 过程记录

### 实现中核对到的事

- **electron-builder 给谁签什么**（在本机已有的 e2e 产物上只读核对）：osx-sign 给每个文件都传 `--entitlements` 和 `--options runtime`，但 codesign 只把 entitlements 嵌进可执行文件（MH_EXECUTE）：主程序、4 个 helper、`chrome_crashpad_handler`、`app_shim_loader`、`ShipIt`；framework、dylib 和原生扩展都没有 entitlements。所以 IT-selfuse-build 的白名单按"可执行文件 / 库"分：可执行文件是 `allow-jit` 加 `disable-library-validation`（宿主、CLI 为空，Node 只有 `allow-jit`），库一律为空。
- **递归 `readdir` 会走进 framework 的符号链接**（`Versions/Current` → `A`、顶层的 `Helpers`、`Libraries`），同一个 Mach-O 出现两三次。扫描先 `realpath` 再去重：e2e 产物里是 17 个 Mach-O，自用构建多一个 CLI，共 18 个，全是 arm64 单架构。
- **用例在本机只做过两件事**，都不启动任何 App：
  - 不设 `RUNNER_ENVIRONMENT` 跑 `test:selfuse`：三个文件都在 `built()` 的防护上失败，没有启动任何东西；
  - 把 IT-selfuse-build 指向本机旧的 e2e 产物：五条断言都按预期失败（身份、entitlements、`enableNodeCliInspectArguments`、asar 里的测试入口、`test-harness` 特性），说明每条检查都看得见它要找的东西。
- **`codesign --display <pid>`** 给出运行中的 `Executable=` 和 `CDHash=`，用来核对 `host.lock` 里的宿主是哪一份；`ps -x -o comm=` 在 macOS 上是完整路径，用来看 App 里起了哪些进程。
- **主进程收到 SIGTERM 等同 ⌘Q**：走 `before-quit`，没有运行中的回合时不问；宿主随后成为孤立状态，1 秒一次的观察发现后退出。

### 规模

| 时点 | `cargo xtask size` |
|---|---|
| 宿主 worktree 根目录 | +37 |
| 加上打包配置、构建入口 | +208 |
| 加上 selfuse 作业、三个用例、nightly | +740（离停止线 60） |

比估计（705）多 5%。测试 424 行（估计 415），实现 316 行（估计 290）。

### 第一次在 runner 上运行（nightly 41581457853，b767fd4）

构建四步都成功（build 273 s）；用例 10 过 3 败。构件 `selfuse-macos-15-logs` 下载到本机一个新的空目录，内容只当数据读。

1. **IT-selfuse-build 的"没有测试入口"**：asar 里命中 `__cupperE2E`、`CUPPER_E2E_DIR`、`CUPPER_E2E_BACKGROUND`。
   - 本机 `vite build --mode self-use` 到临时目录后扫描：代码文件一个都没有，只有 `main/index.js.map` 命中，命中的正是这三个名字。原因是 sourcesContent 带着 `main/index.ts` 的源码，其中有 e2e 专用、已被 Vite 去掉的那几段。
   - 修法：发布配置和自用配置的 `files` 加 `!**/*.map`。App 里没有地方读 source map（没有 `setSourceMapsEnabled`，也没有 `--enable-source-maps`）。
2. **E2E-selfuse-coexist 的 helper 列表**：实际起来的是主程序、Renderer helper、**两个**普通 helper 和宿主，没有 GPU helper。
   - 本机核对：VS Code 用的是 Electron 42.10，它的 `--type=gpu-process` 跑在 "Code Helper" 里，没有 "(GPU)"。所以 Electron 44 的 GPU 进程跑在普通 helper 里，GPU helper 和 Plugin helper 一样不会启动。
   - 原用例的期望写错了。改为断言主程序、普通 helper、Renderer helper、宿主，以及进程类型里有 gpu-process、renderer、utility。
3. **E2E-selfuse-replace 退出旧 App 超时**：
   - host.log：B 的 CLI 一连上（`conn.rejected peer_exited`，CLI 校验 cdhash 不符后自己断开），旧宿主就写下 `host.pending_upgrade`，0.45 s 后以 `upgrade` 有序退出。
   - 旧界面马上拉起了两次新宿主（control 连接和窗口连接各一次），都因为旧宿主还占着锁、以退出码 3 退出（相隔 10 ms）。之后预算 500 ms 用完，界面停在 `failed: host_unreachable`。这一次没有出现"旧界面接上新宿主"。
   - 第一次 SIGTERM 后 30 s 内，App 里一直有进程在运行；finally 里的第二次 SIGTERM 让它立刻结束（整个用例 39 s）。Electron 只把第一个 SIGTERM 当作优雅退出，之后恢复默认处理。所以被替换后的旧 App 卡在了优雅退出的过程中，卡在哪里还不知道。
   - 这次没有修，只加了诊断：等待超时时附上 App 里还在运行的进程（pid、ppid、运行时长、参数）和 Chromium 的 stderr；旧 App 的退出改为 soft 检查，后面的步骤照常跑。

### 第二次运行（nightly 41586879127，f3fff60）

IT-selfuse-build 和 E2E-selfuse-coexist 通过；E2E-selfuse-replace 只在 soft 检查上失败，旧 App 第一次 SIGTERM 后 30 s 仍未退出。诊断给出：

- **还在运行的进程**：主进程（40 s）、普通 helper 里的 gpu-process 和 network service、一个**由旧主进程拉起、已运行 30 s 的 cupper-host**（新版本的二进制）。Renderer 已经没了。
- **host.log 的时间线**：
  - 06:13:37.509 旧宿主以 upgrade 退出。
  - 旧界面随即两次拉起新宿主：一次退出码 3，一次 37.795 起来了。
  - 37.929–38.019 旧界面的 control 和 window 连接完成握手，main.log 在 38.028 记了"host connection ready"。
  - 用例在旧宿主退出后最多约 0.3 s 发出 SIGTERM（每 250 ms 查一次进程表），落在这次重连的握手当中。
  - window 连接 38.052 关闭（窗口关了）；**control 连接一直开到 06:14:08**，第二次 SIGTERM 杀掉进程时才关。也就是说 `will-quit` 里的 `control.close()` 没有关掉它：重连在退出开始之后才完成。
- **第一次运行（41581457853）也一样**：SIGTERM 落在重连期间，那次重连以 `host_unreachable` 失败，旧 App 照样没退出。
- **代码上能看到的一个缺口**（没有改）：`ControlConnection.end()` 不会取消正在进行的 `#connect()`；`launcher.open()` 返回后，`#connect()` 不看 `#ended` 就 `attach`，而 `HostClient.attach()` 对已经 `close()` 的客户端也照样回到 ready。具体卡在退出流程的哪一步，没有证实。

### 第三次运行（nightly 41589556442，6c76959）与随后的用例修正

- **E2E-selfuse-coexist 没有就绪**：replace 留下的旧 App 没退出，一直占着 `cupper-local/ui` 的单实例锁。coexist 启动的 App 在 main.log 里记下 `another instance is running; quitting` 后就退出了。
- **E2E-selfuse-replace 本身的失败**：
  - 用例一就绪就换包。这时旧宿主首次运行的安装器正在从包里复制 esbuild-wasm，发现包变了（`runtime install: Err(BundleChanged)`），宿主自己进入待升级并退出。
  - 新 CLI 来连时宿主已经不在，退出码是 3 而不是 4。这个失败被 finally 里的退出超时盖住了。
  - 旧 App 的 control 连接在 SIGTERM 之前约 0.4 s 就已经停在 `failed: spawn_failed`，但它照样没退出。窗口已经关了，主进程的 JS 还在运行（收到 `second-instance` 后又建了一个窗口）。所以退出卡住并不限于"重连途中"。编排者据此扩大了 T225 的范围（main 489fede）。
- **用例的修正**（编排者裁定，停止线放宽到 850）：
  - 换包前，先等运行时的三个组件都有 `.installed.json`。这个信号与运行顺序无关：已经装过的组件不会再写安装日志，但标记文件一直在。
  - 退出超时后用第二次 SIGTERM 结束 App，退出检查本身仍判失败。
  - finally 里的退出失败用 `expect.soft` 报告，不盖住用例原来的失败。
- 编排者已定：E2E-selfuse-replace 在 T225 修好之前必然失败；T160 等 T225 合并后再跑一次 selfuse，三个用例都通过才合并。
