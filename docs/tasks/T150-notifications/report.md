# T150 汇报

- 执行者：子 agent（宿主与桌面工程师）
- 分支：`feat/T150-notifications`
- 完成日期：2026-11-10

## 做了什么

**第 0 步：补验第一条验收，通过。** 用户在场，原型装在 `/Applications/SP10.app`，经编排者转达用户同意。证据在 `docs/tasks/T150-notifications/results.md` 和 `logs/`，日志已脱敏，原始日志已删除。
- 原型只改了一处：helper 在所有模式下都跑 NSApplication（accessory）。
- 一共点了 4 次（1、2、3、3b），两种现象都没有再出现：
  - 每次都没有 -600，没有 Finder 的 "…is not open anymore" 对话框；
  - 主 App 退出后点击，系统没有再拉起第二个 helper，主 App 被启动并拿到了 URL。
- 整个时段的系统日志里没有一条 `procNotFound`，launchd 也没有启动过 helper 任务。
- 3 和 3b 按日志，点击那一刻在最前的是别的应用。编排者认定这个条件本来就做不到严格（点通知中心那一刻，通知中心就是前台），不需要重做。
- 新发现，已写进 notes.md，编排者已补进设计 5.2.9：点击后系统会把宿主设为前台，盖过宿主在回调里对外层 App 的 `activate()`。

**估算与拆分。** 开工前按模块估约 1,690 行。编排者按"每一块合并时都有人用"拆成 T150、T250、T251，本任务只做下面这些。

1. **cupper-sys**
   - `runloop.rs` 在原文件上改写：主线程改跑 NSApplication（accessory），任意线程可以停止，"运行之前就停止"的语义不变。
   - 接管"退出"Apple Event。注销、关机、活动监视器的"退出"会给 App 发这个事件，AppKit 默认直接 `exit(0)`，跳过有序退出。现在在 `applicationWillFinishLaunching:` 里接住它，交给宿主。
   - `RunLoop` 被丢弃时移除自己的停止源和退出处理。CoreFoundation 把回调相同、`info` 为空的两个源当成同一个，不移除的话，后建的循环停不下来。
   - 新增 `launch_services.rs`：查询某个 bundle id 登记在哪些路径、按路径读 bundle id、`ensure_registered`（这个路径没登记才调 `LSRegisterURL`）。
   - objc2 系列依赖锁定精确版本：objc2 =0.6.5，objc2-app-kit、objc2-foundation =0.3.2；许可证在 ADR-11 白名单内，`license` 检查已过。
2. **宿主**
   - `main.rs` 换用新的主循环。
   - 新增 `Host::quitter`：退出事件发 `Exit::Terminated`，和 SIGTERM 走同一条有序退出。
   - 启动时检查登记：放在 `spawn_blocking` 里，不在启动路径上，只记日志。
3. **桌面**
   - `cupper_native` 新增 `ensureHostRegistered`。
   - `hostLauncher` 每次拉起宿主之前调用它；失败只记日志，照常拉起。
4. **打包**：`host-bundle` 把每种构建的显示名称写进宿主 Info.plist 的 `CFBundleDisplayName` 和 `CFBundleName`。
5. **用例**
   - UNIT-host-bundle-identity：四种构建的宿主名称都对。
   - `cupper-sys/tests/runloop.rs`：跑在主线程上（`harness = false`）。
   - `cupper-sys/tests/launch_services.rs`：只在 GitHub runner 上跑。
   - 原生扩展和 `hostLauncher` 的单元测试。

## 关键取舍

- **退出事件放在本任务里处理**（编排者选 1，停止线改为 520、硬线 600）。改跑 NSApplication 是本任务引入的，退出路径的回归也在本任务里堵上。
  - 在临时目录用探针实测过：在 `run()` 之前装自己的处理没用，AppKit 在 finishLaunching 时会换回它自己的，进程照样当场退出；在 `applicationWillFinishLaunching:` 里装才接得住。
  - 宿主的退出码和最后一行日志都照常。
- **test-harness 和 e2e 构建不碰 LaunchServices**（编排者同意）。
  - 宿主只在不带 `test-harness` 的构建（发布、自用、开发）里检查登记；桌面的 e2e 构建不登记。
  - 理由：这些构建的通知只记录，宿主的测试还会在自己做的临时 bundle 里跑宿主，登记就会改动本机的 LaunchServices 数据库。
  - 真实登记的测试看 `GITHUB_ACTIONS`：在每个 PR 的 `rust` 作业里由 `cargo test --workspace` 带上；本机（包括 `cargo xtask ci`）直接返回。bundle 放在 target 目录下，免得被标成 in-temp-dir。
- **本任务只写名称，不写图标**（编排者裁定 3）。
  - 仓库关掉了依赖的构建脚本，`node_modules/electron` 里没有 `Electron.app`，也就没有 `electron.icns`。主 App 的默认图标来自 electron-builder 打包时下载的 Electron zip，而 `host-bundle` 在它之前跑。
  - 取这个图标要么写下载解包的代码，要么入库一份替身，都活不到图标定稿。
  - 先不写图标没有风险：通知中心只在 helper 第一次连上通知服务时读取名称和图标（T101），本任务不调用任何通知接口。
  - `CFBundleIconFile`、图标文件和图标的断言随 T250 一起加。占位的 icns 曾放过，没提交就删了；electron-builder 的 `mac.icon` 没改。
- **`runloop.rs` 在原文件上改写，没有另起新文件**。设计 5.2.3 写的就是"`cupper-sys::runloop` 随之改"；原有的停止源代码不动，diff 只反映真实改动。
- **桌面要的 bundle id**：从宿主 bundle 的 Info.plist 读，由桌面进程读取。宿主自己的 id 用签名标识（csops），不读包内文件。

## 验证方式

验收标准对照：
- [x] 第 0 步的补验通过（`results.md`）。
- [x] UNIT-host-bundle-identity 通过；宿主的用例在 NSApplication 主循环上全部通过。
  - 本机：宿主全套 `cargo test -p cupper-host --all-features` 通过，含 SIGTERM、`host/exit`、空闲退出、自检升级。
  - CI：`rust` 作业里同样通过。
- [x] 宿主的约束都守住：
  - `unsafe` 只在 cupper-sys；
  - 宿主不新增读时间的地方。测试用的定时器在 cupper-sys 的 `test-util` 里，用的是 CoreFoundation，不在 clippy 禁用的方法之列；
  - 没有新的子进程；
  - 发布、自用、test-harness 三种特性组合下 clippy 都无告警。
- [x] PR 上 GitHub CI 的 8 个检查都通过；nightly 的 `selfuse`、`release-config` 在分支上通过。
  - PR #96：https://github.com/boris-w/cupper/pull/96
  - ci 运行：https://github.com/boris-w/cupper/actions/runs/42750862654 ，8 个作业全过
  - cupper-sys.yml 的 security-peer（macos-26）通过：https://github.com/boris-w/cupper/actions/runs/42750862364
  - nightly（selfuse 那次）：https://github.com/boris-w/cupper/actions/runs/42750860832 ，全过
  - nightly（release-config 那次）：https://github.com/boris-w/cupper/actions/runs/42750870478 ，全过
  - `rust` 作业的日志里，`launch_services_registers_a_bundle_once_on_a_runner ... ok` 真的跑了，没有跳过；`tests/runloop.rs` 的两段都打印了通过。

本机其他检查：
- cupper-sys 全部测试通过：退出事件交给宿主而不是 `exit(0)`、另一线程停止、运行前停止；
- xtask `host_bundle` 的测试、desktop 单元测试（110 个）、原生扩展的 Node 测试都通过；
- fmt、prettier、eslint、`trace-ids check` 都通过。

规模：`cargo xtask size` +543 −66，原型 +18 不计，计入 +525。编排者已接受（停止线 520、硬线 600）。

## 提交列表

```
996240ec docs(task): T150 notes after the split: the quit event, registration, size
fc81fd6e feat(xtask): name the host bundle as its build kind
6ef20034 feat(desktop): register the host bundle before starting it
b586c763 feat(sys): register the host bundle with LaunchServices
1dbbf4f1 feat(sys): run AppKit on the host's main thread
1b5a09b8 Merge branch 'main' into feat/T150-notifications
219f535f Merge branch 'main' into feat/T150-notifications
a60c981c docs(task): estimate T150 by module before starting
4d1e2602 docs(task): record T150 step 0, the click check on NSApplication
42d9bd9d chore(spike): run AppKit in every SP-10 helper mode for T150 step 0
```

## 遗留问题 / 范围外发现

1. **要补进设计的两条**，编排者说合并时补：
   - 5.2.13 的"有序退出"：退出事件和 SIGTERM 走同一条有序退出；
   - 5.2.9 的"通知的点击"：系统先把宿主设为前台，已补。
2. **任务书里的停止线仍写 450/540**；编排者的裁定是 520/600，`cargo xtask size` 按任务书报"超线"。
3. **交给 T250 的事**：
   - `CFBundleIconFile` 和图标文件，UNIT-host-bundle-identity 补上图标断言，四份 electron-builder 配置的 `mac.icon` 指向同一个 icns；
   - UserNotifications 的 sys 接口可以挂在现在的主循环上。`RunLoop` 一个进程只能建一个能收退出事件的，宿主已经只建一个。
4. **本机遗留**：
   - `/Applications/SP10.app`（这次是 own 写法）及 T101 的两条 LaunchServices 登记、系统设置里的 "SP10 Notifier" 仍在，按 `.private/cleanup/T101-leftovers.md` 等用户决定清理，没有新增遗留；
   - 原型日志文件已删除。
5. **没有验证的**：
   - 注销、关机时 loginwindow 真的会给宿主发退出事件，这一点没在本机实测（不能注销用户的机器）。用例证明的是：只要事件来了，就交给宿主走有序退出，而不是 AppKit 当场退出。
   - 自用构建的真实注销，留给 MAN-notification-product 前后观察。
