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
