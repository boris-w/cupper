# T101 SP-10 通知身份：实测记录与结论

- 日期：2026-10-28，11:37–12:15
- 环境：macOS 26.4（25E246），arm64；Xcode 26.6，Swift 6.3.3
- 原型：`spikes/T101-notification-identity/`（用法见其 README）；脱敏后的日志在本目录的 `logs/`：`prototype.log` 是原型两个进程的事件，`system.log` 摘自 `log show`
- 本文是 MAN-notification-spike 的证据记录。"用户确认"一栏只写用户经编排者转述、确认过的内容，其余以日志为准，并注明"用户未确认"。

## 原型与做法

| | 主 App | helper |
|---|---|---|
| 位置 | `/Applications/SP10.app`，和产品的安装位置相同 | `SP10.app/Contents/Helpers/SP10Host.app` |
| bundle id | `dev.cupper.spike.sp10` | `dev.cupper.spike.sp10.host` |
| 名称、图标 | `CFBundleName` 为 SP10 Spike，蓝色 M | own 写法：`CFBundleName` 为 SP10Host，`CFBundleDisplayName` 为 SP10 Notifier，橙色 H（s1–s4b）；bare 写法：只有 `CFBundleName` SP10Host，没有图标，即 T005 现在给宿主写的 Info.plist（s5、s6） |
| 其他键 | `CFBundleURLTypes` 登记 `sp10-spike:` | `LSUIElement`；不写任何用途说明 |
| 签名 | ad-hoc，开 hardened runtime（自用构建的签法） | 同左，从内到外签 |

- 主 App 用 `posix_spawn` 拉起 helper：带 `POSIX_SPAWN_SETSID`、`POSIX_SPAWN_CLOEXEC_DEFAULT`，fd 0–2 指向 `/dev/null`，环境变量只留 `HOME`、`PATH`。这和 `cupper-sys::spawn` 拉起宿主的方式相同，不经过 `open` 或 LaunchServices。
- helper 和宿主一样不建 NSApplication，主线程跑 CFRunLoop。例外：s2c 之后，系统不带参数启动它时改跑 NSApplication。
- 点击之后：
  - 主 App 在运行：helper 调 `NSRunningApplication.activate()`；
  - 主 App 不在：helper 用 `NSWorkspace.open([URL], withApplicationAt:)`，只把 `sp10-spike://session/<id>` 交给按 bundle id 找到的那个 App。
- 每一步只发一条通知，标题依次是"SP-10 s1""SP-10 s2"……

## K5 判定：通过（有条件）

K5 的判据是：helper 能申请授权、弹通知、点击后激活主 App。三项都做到了。

- 通知、授权框和系统设置里显示的都是 helper 自己的名称和图标，用户认得出。
- 点击回调能回到正在运行的 helper，而且是在主线程上，CFRunLoop 就够。
- helper 按 bundle id 激活主 App，或者启动主 App 并只交给它一个 URL，都成功了。

所以按原设计做 T150，不启用 T158。但下面四个问题要由 T150 和设计 5.2.9 解决：前两个原型里已经有解法，后两个要补规则；其中第 4 条的解法还没验证。

1. **嵌套的 helper 不会自动在 LaunchServices 登记。** 没登记时，通知服务直接拒绝它："Failed to find or validate client"，返回 `UNErrorDomain 1`，授权框也不弹。宿主或 UI 要用 `LSRegisterURL` 登记 helper，而且要在宿主进程启动之前登记。
2. **宿主退出后，用户点了留在通知中心里的通知，系统会自己启动一个 cupper-host。** 这个宿主不带参数，父进程是 launchd，与 UI 无关。它只有跑 NSApplication 才拿得到这次点击；只跑 CFRunLoop 时什么也拿不到，用户看到的是"点了没反应"。
3. **宿主在运行、UI 已经退出时点通知，系统也会再启动一个 cupper-host。**
4. **宿主在运行、UI 也在时点通知，系统会弹出"The application "<名称>" is not open anymore."。** 用户实际看到了一次：Finder 图标的系统对话框，只有 OK 按钮。原因是系统要经 LaunchServices 把 helper 带到前台，可 helper 不是经 LaunchServices 启动的，返回 -600 procNotFound。第 3、4 条可能都来自"宿主不是一个 LaunchServices 认得的 App"。宿主改跑 NSApplication 能不能消除它们，**没有验证**，见文末"缺口"。

## 逐项现象

### 1. helper 申请通知授权：授权框的名称和图标

| 时间 | 现象（日志） | 用户确认 |
|---|---|---|
| 11:54 | helper 没在 LaunchServices 登记（`urlForApplication(withBundleIdentifier:)` 为空）。主 App 装进 /Applications 几秒后就被自动登记了，嵌套的 helper 没有；对主 App 做 `lsregister -f` 也不会带上它。`requestAuthorization` 立即返回 `UNErrorDomain 1 "Notifications are not allowed for this application"`，usernoted 记 "Failed to find or validate client of identifier dev.cupper.spike.sp10.host"。没有弹框 | 用户没看到授权框（编排者转述） |
| 11:56 | helper 改为启动时调用 `LSRegisterURL` 登记自己。登记后的标志是 `ui-element launch-disabled`（加上 own 写法的 `has-display-name`），和本机其他 App 嵌套的 helper 相同。同一进程紧接着查询授权状态，仍报 "Failed to find or validate client"，下一个进程就正常了 | — |
| 11:57:38 | 授权请求弹出：usernoted 记 "Sending request for permission for dev.cupper.spike.sp10.host"，以 `com.apple.notificationcenter.askpermissions` 的提醒呈现；NotificationCenter 按 helper 的 bundle 取到的名称是 "SP10 Notifier" | 授权请求弹出了 |
| 11:57:49 | 授权请求收到一次操作后收起，`requestAuthorization` 返回未授权，状态为 denied（`authorizationStatus=1`）。之后再申请不会再弹框，直接返回 denied | 用户不小心把它关掉了，没点"允许"。名称和图标用户没回答 |
| 12:00:55 | 用户在 系统设置 → 通知 里打开"允许通知"之后，`requestAuthorization` 直接返回 granted，没有弹框 | 用户已在系统设置里打开 |

结论：
- 授权请求以 helper 的身份发出，名称取自 helper 的 `CFBundleDisplayName`。授权框上的图标没有记录（日志里没有，用户也没回答）。
- 关掉授权请求就等于拒绝，之后系统不再弹框。

### 2. helper 弹出的通知：名称和图标

- 日志：每条通知的记录都是 `app:"dev.cupper.spike.sp10.host"`，以横幅呈现。名称取 NotificationCenter 第一次连上时查到的 "SP10 Notifier"。
- 用户确认：通知上的图标是橙色的，即 helper 的 H。用户没说是哪一条，也没说名称。

### 3. 系统设置 → 通知里的条目

- 用户确认（截图，s5 之后截的，没有入库）：
  - 条目标题 "SP10 Notifier"，"Allow notifications" 下面一行小字也是 "SP10 Notifier"，图标是橙色 H；
  - Desktop、Notification Center、Lock Screen 都勾着，Alert Style 为 Temporary，声音开，Show previews 为 Default，Notification grouping 为 Automatic。
- 这一条属于 helper 的 bundle（`dev.cupper.spike.sp10.host`）。主 App 从没调用过通知接口，预期不会有它的条目；用户没有提到别的条目。
- **名称和图标只在第一次取。** s5 之前 helper 已改成 bare 写法，LaunchServices 也按新的 Info.plist 重新登记了（名称 SP10Host，没有显示名称和图标），但系统设置里仍是 "SP10 Notifier" 和橙色 H。日志里 NotificationCenter 只在 11:57:38 helper 第一次连上时查过一次 App 信息。

### 4. helper 在运行时点击：回调、激活主 App

| 步骤 | 条件 | 点击 | 回调 | 主 App | 系统的附带动作 | 用户确认 |
|---|---|---|---|---|---|---|
| s1 | helper 在（主 App 的子进程），主 App 在、不在前台 | 12:01:54 | 正在运行的 helper 收到，`main-thread=true` | `activate()` 返回 true，1 秒内主 App 到了前台 | LaunchServices 把 helper 带到前台时返回 -600 procNotFound | SP10 Spike 窗口回到了最前 |
| s2 | helper 在（主 App 已退出），用户在通知出现 2 秒后点击，helper 还没退出 | 12:02:07 | 正在运行的 helper 收到 | 主 App 被启动，拿到 `sp10-spike://session/s2` | 系统另外启动了第二个 helper（pid 52155，没有参数，父进程是 launchd），它什么也没收到 | 用户未确认窗口里的文字 |
| s3 | helper 在，主 App 已退出 | 12:02:23 | 正在运行的 helper 收到 | 主 App 被启动，拿到 `sp10-spike://session/s3` | 同 s2，又启动了一个 helper（pid 52803） | 用户未确认 |
| s5 | bare 写法；helper 在，主 App 在且已在前台 | 12:11:41 | 正在运行的 helper 收到 | `activate()` 返回 true，主 App 留在前台 | -600 procNotFound，系统弹出 "The application "SP10Host" is not open anymore." | 用户看到了这个对话框（截图）；用户说是点通知中心里一条更早的通知后出现的 |

- 日志里一共只有 6 次点击（s1、s2、s3、s2b、s2c、s5），每次都对得上自己的那条通知。-600 只出现在 s1 和 s5 两次点击时，这两次 helper 都在运行，而且是正在运行的主 App 的子进程。
- 对话框里的名称 "SP10Host" 与 s5 时的 bare 写法相符，所以多半就是 s5 那一下，它也可能是从通知中心里点的。日志里找不到用户说的"更早的一条"所对应的另一次点击。
- 原因：点击后 usernoted 记 "Bringing app dev.cupper.spike.sp10.host front for response"，要经 LaunchServices 把 helper 的 bundle 带到前台。
  - helper 是主 App 的子进程时，LaunchServices 找得到这个进程，但它不是"可激活的 App"，于是返回 -600，系统弹出上面的对话框；
  - helper 是孤儿进程时，LaunchServices 找不到它，就另外启动一个。
  - `launch-disabled` 标志挡不住这两种情况。

### 5. helper 已退出时点击

| 步骤 | 系统启动的 helper 跑什么 | 现象（日志） | 用户确认 |
|---|---|---|---|
| s2b | CFRunLoop（宿主现在的做法） | helper 12:06:20 退出，用户 12:06:21 点击。usernoted 记 "Checking … for launch of dev.cupper.spike.sp10.host"，"Application … found using LaunchServices"。launchd 以 `application.dev.cupper.spike.sp10.host.<n>.<n>` 的名义启动了 helper（pid 61250，没有参数，父进程是 launchd，`XPC_SERVICE_NAME` 就是这个任务名）。它连上了 usernoted，但 30 秒里一直没收到点击，超时退出。主 App 没有被启动，也没有被激活 | 用户没有明确回答是不是"点了没反应" |
| s2c | NSApplication | helper 12:10:09 退出，用户 12:10:13 点击。系统同样启动了 helper（pid 69091），这次它收到了点击回调，并启动主 App、只交给它 `sp10-spike://session/s2c` | 用户截图：SP10 Spike 窗口出现，里面有 "SP10 Spike: the main app" 和 "opened sp10-spike://session/s2c" 两行 |

结论：系统会单独启动 helper 的 bundle。产品里这就是一个由系统拉起、与 UI 无关的 cupper-host，设计要补规则（见下文）。这个进程要跑 NSApplication 才拿得到点击。

### 6. 主 App 没在运行时，helper 启动它，只交一个 URL

- s3、s2c（点击之后）和 s4b（不经点击）都调用了 `NSWorkspace.open([URL], withApplicationAt:)`：系统启动了那个 App，`application(_:open:)` 收到的正是那一个 URL，而且在 `applicationDidFinishLaunching` 之前。
- s4b 交的是主 App 没有登记的 scheme（`sp10-unregistered://session/s4b`），照样送到了。所以指定了目标 App 时，scheme 不用在 Info.plist 里登记，也就不经过任何 App 都能注册的通用路由（安全 #7）。用户未确认 s4b 窗口里的文字。
- 按 bundle id 查到的是 `/Applications/SP10.app`，和由 helper 自身路径往上推三级得到的外层 App 一致。但同一个 bundle id 可能登记着好几份：本机 `dev.cupper.app.e2e` 就有 6 条记录，而 ad-hoc 签名谁都能声称任意 bundle id。

### 其他观察

- **s6**：
  - helper 调 `removeAllDeliveredNotifications()` 之后，已送达的通知清空了（`delivered=[]`）。用户确认通知中心里原型的通知都没了。
  - 拒绝那一轮发出的 s1 被系统静默放进了通知中心；打开授权后重发的 s1 用的是同一个标识，把它顶替了。
- **原地重新构建（相当于升级）之后，授权还在**（12:05:57 `authorization=2`）。但有一次 LaunchServices 丢了 helper 的登记：12:05:34 时 `registered=no`，helper 当场登记自己后的第一次查询仍报 "Failed to find or validate client"，返回 notDetermined；下一个进程就正常了。

## 给 T150 的结论

### CupperHost.app 的 Info.plist

| 键 | 写法 | 依据 |
|---|---|---|
| `CFBundleIdentifier` | 按构建种类，同 5.7.9 | 授权记录、系统设置条目都挂在 helper 的 bundle id 上 |
| `CFBundleDisplayName` | 构建种类的显示名称（Cupper、Cupper Local、Cupper E2E、cupper Dev） | NotificationCenter 和系统设置用的是它（第 1、3 项） |
| `CFBundleName` | 建议与 `CFBundleDisplayName` 相同，不再用 "CupperHost" | 没有显示名称时系统用它；s5 的"… is not open anymore"对话框里出现的就是 "SP10Host" |
| `CFBundleIconFile` | 把 UI 的 icns 复制进 `CupperHost.app/Contents/Resources/` 并指向它，每种构建用自己的图标 | 通知和系统设置里的图标来自 helper 自己的 bundle，不会去取外层 App 的（第 2、3 项） |
| `LSUIElement` | 保留 `true` | 宿主不进 Dock；系统按 `ui-element` 登记它 |
| `LSMinimumSystemVersion` | 14.0，不变 | — |
| 用途说明、`CFBundleURLTypes` | 都不写 | 交 URL 时指定了目标 App，宿主和 UI 都不需要登记 scheme（第 6 项） |

- **名称和图标只在第一次登记时取**，以后改 Info.plist 不会更新系统设置（第 3 项）。所以第一个会申请通知授权的构建就要把名称和图标写对：自用构建是 M1b 里最先用到的。
- 宿主的 cdhash 会随 Info.plist 和图标变化，T005 的 `host-bundle` 要把图标一起放进去再签名。

### 登记

- 宿主进程启动之前，就要让 helper 的 bundle 在 LaunchServices 里登记好：查询 bundle id，结果里没有宿主自己的路径时，调用 `LSRegisterURL(<CupperHost.app>, false)`。
- 建议由 UI 主进程经 `cupper_native` 在拉起宿主前做。宿主进程里当场登记后，同一进程的第一次通知调用可能仍被拒（上文 11:56、12:05:34 两次）。
- `LSRegisterURL` 由系统进程去读 Info.plist，宿主自己不读包内文件，不违反"运行中不读包内资源"。
- 没有验证：用户从 DMG 拖进"应用程序"文件夹时，系统会不会顺带登记嵌套的 helper。本机其他 App 嵌套的 helper 的登记时间与外层 App 相同，可能是安装或更新时一并登记的。原型是直接写进 /Applications 的，没有被登记。

### 主线程与事件循环

- 宿主自己在运行时，CFRunLoop 就能收到点击回调，而且在主线程上（第 4 项）。
- 系统单独启动宿主时，只有 NSApplication 收得到点击（第 5 项）。
- 建议宿主的主线程改跑 NSApplication（`LSUIElement`，不开窗口），取代 `cupper-sys::runloop` 的裸 CFRunLoop。这会动到设计 5.2.3。这样做能不能同时消除第 4 项的 -600 对话框和重复启动，见"缺口"。

### 授权被拒

- 关掉授权请求就记为 denied，以后不会再弹框（第 1 项）。
- 宿主应在 `host` 状态里报告授权状态，界面在 denied 时提示用户去 系统设置 → 通知 → <显示名称> 打开。具体的跳转写法没有验证。

### 激活与交 URL

- 激活：`NSRunningApplication.activate()` 在用户点击之后有效（s1）。
- 交 URL：`NSWorkspace.open([URL], withApplicationAt:)` 只把这个 URL 交给那一个 App（第 6 项）。
- **目标 App 建议按路径找**，不建议按 bundle id：由宿主自身路径向上三级就是外层的 Cupper.app，再核对它的 bundle id（以及发布构建的签名要求）。LaunchServices 按 bundle id 查，可能查到另一份同 id 的拷贝，包括伪造的。

## 设计 5.2.9 要补的规则（交编排者）

1. **登记**：UI 拉起宿主之前，确保宿主的 bundle 已在 LaunchServices 登记；宿主启动时也要检查。
2. **事件循环**：宿主的主线程跑 NSApplication（不进 Dock，不开窗口），不跑裸 CFRunLoop。要改 5.2.3 和 `cupper-sys::runloop`。
3. **系统单独启动的宿主**：
   - 认法：没有参数、没有启动载荷、父进程是 launchd、`XPC_SERVICE_NAME` 以 `application.<宿主 bundle id>.` 开头。
   - 这种宿主只做一件事：等至多几秒的点击回调，把目标交给 UI（UI 在就激活，再经 control 连接发 `ui/navigate`；UI 不在就按路径启动它，只交那一个 URL），然后退出。
   - 不加 `flock`，不绑套接字，不打开数据库，不读 stdin 上的载荷。开发和 e2e 构建会读启动载荷，这一步要排在读载荷之前，否则会读超时退出，点击也就丢了。
4. **有序退出时清掉已送达的通知**（`removeAllDeliveredNotifications`，s6 验证过）。这样宿主退出后，通知中心里不留点了会把宿主拉起来的通知。崩溃时清不掉，靠第 3 条兜底。
5. **授权被拒**：宿主上报授权状态，界面提示用户去系统设置；宿主不重复申请。
6. **Info.plist 与图标**：按上一节的表写，并在 5.7.9 或 5.10.1 写明宿主要带图标和显示名称。

## 缺口（没有验证）

- **宿主在运行时改跑 NSApplication，能不能消除第 4 项的 -600 对话框和重复启动**：
  - 原型只在系统单独启动 helper 时跑 NSApplication，由主 App 拉起的 helper 仍跑 CFRunLoop；
  - 要验证需两次点击：一次在主 App 在时，一次在主 App 退出后，约 3 分钟，要用户在场；
  - 消除不了时，要么改由 LaunchServices 拉起宿主（与 ADR-01 的 `posix_spawn` 不同，还会影响 Q21 的 TCC 归属），要么改用 T158。
  - 这一项建议在 T150 开工前补做，或作为 T150 的第一条验收。
- 授权框上的图标，以及通知上的名称，都没有经用户确认，只有日志。
- 只测了 ad-hoc 签名；Developer ID 签名、公证后的表现，留给 T180 前后的 MAN-notification-product。
- 只测了 macOS 26.4，没有测 macOS 14、15。
- DMG 拖入安装会不会登记嵌套的 helper，没有测。

## 用户确认的现象（MAN-notification-spike）

用户在场完成了 s1、s2、s3、s2b、s2c、s4b、s5、s6，现象经编排者转述，确认了以下几条：

- 授权请求弹出了，被不小心关掉；之后用户在系统设置里打开了通知。
- s1：点击通知后，SP10 Spike 窗口回到最前。
- s2c：helper 已退出时点击，SP10 Spike 窗口出现，里面有 "opened sp10-spike://session/s2c"（截图）。
- 系统设置 → 通知：一条 "SP10 Notifier"，橙色 H；改成 bare 写法后仍是这样（截图）。
- 通知上的图标是橙色 H。
- 出现过 Finder 的系统对话框："The application "SP10Host" is not open anymore."（截图）。
- s6 之后，通知中心里原型的通知都没了。

截图都没有入库，入库物料以本文的文字记录为准。
