# T150 工作笔记

- 执行者：子 agent（宿主与桌面工程师）

## 第 0 步

结果见 `results.md`。原型的改动不计入规模，单列：`spikes/T101-notification-identity/` +18 −7（helper 所有模式都跑 NSApplication，记下 LaunchServices 看到的运行中的 helper；README 补一段）。

**新发现，交编排者补进设计 5.2.9**：点击后，系统会把正在运行的宿主设为前台应用（accessory，`foreground=0`）。宿主在点击回调里对外层 App 调 `activate()`，十几毫秒后就被系统这一下盖过（第 1、3 次）；原型 1 秒后的 `NSWorkspace.openApplication` 才把主 App 拉回最前。所以宿主激活外层 App 要排在系统这一下之后，或者改用 `NSWorkspace.openApplication(at:)`。T150 按此实现。

## 第 1 步：按模块估算（2026-11-10，开工前）

按 `cargo xtask size` 的口径（新增行，含测试；生成物、`docs/` 不计）。读过的代码：`cupper-sys`（runloop、codesign、lib）、宿主的 `main.rs`、`startup`、`index`（`status` 纯核心已经按"前台 `desktop_ui` 在看就不发"过滤意图）、`supervisor`、`gateway`（`broadcast`、`clients`）、`settings`、`harness`、`xtask host-bundle`，桌面的 `launcher`、`control`、`index.ts`、`preload`、`shared/ipc`，`ui-core` 的 router（`onNavigate` 已经只导航），e2e 的 fixture（已默认 `notifier: record`）和 `gate.spec.ts` 的 harness 连接。

| 模块 | 内容 | 估计 |
|---|---|---|
| sys：依赖与导出 | objc2、objc2-foundation、objc2-app-kit、objc2-user-notifications、block2，精确版本（MIT / Zlib OR Apache-2.0 OR MIT，都在白名单里）；`lib.rs` 导出 | 20 |
| sys：`app.rs` | 主线程跑 NSApplication（accessory），任意线程可停，取代 `runloop.rs`；主线程上的测试要一个 `harness = false` 的测试文件 | 130 |
| sys：`notify.rs` | UserNotifications：代理（`willPresent` 横幅加列表，`didReceive` 经回调交出点击）、申请授权、查询授权状态、弹通知（不设 category、没有 action）、`removeAllDeliveredNotifications`、内容的构造；进程不在 App 包里时不碰通知中心（否则抛异常）；UNIT-notifier-no-actions | 230 |
| sys：`workspace.rs` | `LSRegisterURL`、查询自己是否已登记（`URLsForApplicationsWithBundleIdentifier`）、`open([URL], withApplicationAt:)`、`openApplication(at:)`（激活）、系统首选语言；只测不碰系统状态的部分 | 140 |
| sys：`codesign.rs` | 按路径取外层 App 的签名标识（核对身份）及测试 | 40 |
| **sys 小计** | | **560** |
| 协议 | `host.updated.notification_auth`（开放枚举）、设置键 `notifications.enabled`、`test/notifications` 与 `test/notification_click`（目录、样例） | 70 |
| 宿主：`main.rs` | 主线程改跑 NSApplication；系统单独拉起的宿主（排在读载荷之前）：认法的纯函数及单元测试、等点击、交 URL、退出 | 90 |
| 宿主：`notifier` | actor 与句柄：收意图、开关、中英文案（4 种原因 × 标题后备、正文）、system / record 两种后端、授权只申请一次并写进 `host.updated`、启动时检查并补登记、有序退出时清通知、点击：control 连接发 `ui/navigate`，激活外层 App（排在系统前台切换之后），UI 不在时按构建种类的 scheme 交 URL；外层 App 的路径与身份 | 330 |
| 宿主：接线 | `IndexActor`（意图连同标题、`pending_kind` 交给 notifier；`host.updated` 新字段）、gateway 只发给 control 连接、supervisor 的启动与有序退出、harness 的两个测试方法、登记的时长 | 90 |
| 宿主：IT-notifier-record | 三种原因各一条且文案正确、开关关掉不弹、前台在看不弹、模拟点击后 control 连接收到 `ui/navigate` | 170 |
| **宿主小计** | | **680** |
| 桌面：登记 | `cupper_native` 的 `ensureHostRegistered`、TS 声明、`hostLauncher` 拉起前调用、单元测试 | 50 |
| 桌面：导航 | control 上的 `ui/navigate` → 显示窗口 → `NAVIGATE` 发给渲染进程，文档没加载完先排队；单元测试 | 90 |
| 桌面：`deeplink` | `open-url` 在 ready 之前注册、白名单路由与 ID 格式、启动完成前收到的 URL 不丢、e2e 和开发构建补 `protocols`；单元测试 | 120 |
| **桌面小计** | | **260** |
| 打包 | `host-bundle`：每种构建的显示名称写进 `CFBundleDisplayName`、`CFBundleName`，复制图标并写 `CFBundleIconFile`；测试断言 | 40 |
| 用例：E2E-notification-navigate | record 模式下模拟点击，切到会话，焦点不进请求面板 | 80 |
| **合计** | | **约 1,690** |

超过停止线 800 和硬停止线 950 一倍左右，按 brief 先报告，不动手。

**拆分建议**（各自在 800 以内，按依赖顺序）：

1. **A 系统接口与身份**（约 670）：sys 全部（560）、宿主主线程改跑 NSApplication（约 20）、`host-bundle` 的名称和图标（40）、`hostLauncher` 拉起前登记（50）。UNIT-notifier-no-actions 在这里。
2. **B 宿主 notifier**（约 730）：协议附加（70）、系统单独拉起的宿主（70）、notifier（330）、接线（90）、IT-notifier-record（170）。
3. **C 桌面导航与深链接**（约 290）：`ui/navigate` 转给渲染进程、`deeplink`、E2E-notification-navigate；MAN-notification-product 放在最后这一个。

brief 里的两种拆法（宿主侧和桌面侧分开、`deeplink` 单独拿出去）都不够：宿主侧（sys、宿主、打包）约 1,280，仍超过硬停止线。

**开工前要定的事**：

1. **图标**：仓库里没有 App 图标，UI 现在用的是 Electron 的默认图标（electron-builder 没配 `mac.icon`）。宿主的名称和图标只在第一次登记时取，而自用构建是第一个会申请授权的构建。要先定图标从哪来：用户提供，或者先画一个（像 T101 的 `icon.swift` 那样生成），放在 `apps/desktop/build/icon.icns`，四份 electron-builder 配置和 `host-bundle` 都取它。
2. **开关**：设计 5.2.9 写的是一个开关 `notifications.enabled`，但协议里已经有三个按原因的键 `notifications.needs_action`、`.turn_completed`、`.error`（设计 5.2.11 的表）。协议只能附加，删不掉这三个。建议附加 `notifications.enabled` 作总开关，三个按原因的键照旧生效（默认都开）。
3. **系统单独拉起的宿主**手里没有 control 连接，发不了 `ui/navigate`；它一律用 `open([URL], withApplicationAt:)` 把 URL 交给外层 App（UI 在运行时，URL 经 `open-url` 送到正在运行的 UI，同样只导航）。设计 5.2.9 第 3 条的"UI 在就激活并发 `ui/navigate`"需要相应改写。
4. **URL 的格式**设计里没写，建议 `<scheme>://session/<session_id>`，有请求时加 `?request=<request_id>`；路由只认这一种，ID 按协议的格式校验。

## 拆分之后（2026-11-10，main c6cebb25、45c319e0、83ee81d0）

T150 只做主线程改跑 NSApplication、LaunchServices 登记、宿主的名称；UserNotifications 与 notifier 归 T250，点击与导航归 T251。重估约 500；编排者裁定停止线 520、硬线 600（任务书里仍写 450/540）。

### 退出的 Apple Event（编排者要求记下，合并时补进设计 5.2.13 的"有序退出"）

宿主跑 NSApplication、被 LaunchServices 当成 App 之后，注销、关机和活动监视器的"退出"会给它发"退出"Apple Event（`kCoreEventClass`/`kAEQuitApplication`）。AppKit 默认的处理是 `terminate:`，没有代理时当场 `exit(0)`：不发 `host/shutdown`、回合不标 interrupted、适配器不按 ADR-09 关闭，下次启动当成崩溃恢复。

- 实测（本机临时目录里的探针，不入库）：在 `NSApp.run()` 之前装自己的退出处理不管用，AppKit 在 finishLaunching 时会换回它自己的，进程照样当场退出；在代理的 `applicationWillFinishLaunching:` 里装就能接住，回调在主线程上。
- 做法：`cupper-sys::runloop` 的代理在 `applicationWillFinishLaunching:` 里接管退出事件，交给宿主给的回调；宿主的回调发 `Exit::Terminated`，与 SIGTERM 走同一条有序退出，退出码照常。
- 测试：`crates/cupper-sys/tests/runloop.rs`（`harness = false`，跑在主线程上）在循环运行后从主线程给自己发退出事件，断言回调收到、进程没有退出，再从另一个线程停止循环；另一段断言"运行之前就停止"会在开始时停下。
- 一个进程只完成一次 launching，所以只有第一个循环收得到退出事件；宿主只建一个。`RunLoop` 被丢弃时移除自己的停止源和退出处理：CoreFoundation 把回调相同、`info` 为空的两个源当成同一个，不移除的话后建的循环的停止源加不进去（测试里第二个循环就是这样卡住的）。

### 不碰 LaunchServices 的构建

- 宿主：只有不带 `test-harness` 的构建（发布、自用、开发）启动时检查并补登记；带 `test-harness` 的构建（e2e、宿主的集成测试）一律不碰：它们的通知只记录（5.9.9），测试还会在自己做的 bundle 里跑宿主。
- 桌面：e2e 构建的 `hostLauncher` 不登记，其余三种在每次拉起宿主之前登记。
- 真实登记的测试 `crates/cupper-sys/tests/launch_services.rs` 只在 GitHub runner 上跑（看 `GITHUB_ACTIONS`）：每个 PR 的 `rust` 作业里，`cargo test --workspace --all-features` 带上它；本机（包括 `cargo xtask ci`）直接返回，不登记任何东西。bundle 放在 target 目录下（`CARGO_TARGET_TMPDIR`），不放系统临时目录，免得 LaunchServices 把它标成 in-temp-dir。

### 图标：本任务只写名称（编排者裁定 3）

- 仓库关掉了依赖的构建脚本，`node_modules/electron` 里没有 `dist/Electron.app`，也就没有 `electron.icns`；主 App 的默认图标来自 electron-builder 打包时下载的 Electron zip，而 `host-bundle` 在它之前跑。取这个图标要么写下载解包的代码，要么入库一份替身，都活不到图标定稿（T250 合并前）。
- 先不写图标没有风险：通知中心只在 helper 第一次连上通知服务时读取名称和图标（T101 `results.md`），LaunchServices 重新登记不会改变系统设置；T150 只做登记，不调用任何通知接口。
- 所以 `CFBundleIconFile`、图标文件和 UNIT-host-bundle-identity 里的图标断言都随定稿的图标在 T250 一起加；electron-builder 的 `mac.icon` 不改。

### 规模

`cargo xtask size`：+543 −66，其中原型 `spikes/` +18 −7 不计，计入 **+525**（停止线 520、硬线 600，超停止线 5 行，已报编排者）。

| 目录 | 新增 | 内容 |
|---|---|---|
| crates/cupper-sys | 381 | `runloop.rs` 改写（NSApplication、停止、退出事件、测试用的发退出事件，约 180）、`launch_services.rs`（95，含单元测试）、`tests/runloop.rs`（35）、`tests/launch_services.rs`（49，只在 runner 上跑）、依赖与导出 |
| crates/cupper-host | 42 | 主循环接线、`Host::quitter`、启动时检查登记 |
| crates/xtask | 41 | 显示名称、UNIT-host-bundle-identity |
| apps/desktop | 35 | `hostLauncher` 拉起前登记及单元测试 |
| crates/cupper-desktop-native | 26 | `ensureHostRegistered`、TS 声明、测试 |

比重估的 500 多出的部分在 `runloop.rs`：`RunLoop` 被丢弃时要移除停止源和退出处理（CoreFoundation 把相同回调的两个源当成同一个），以及测试用的"循环运行后从主线程发退出事件"。
