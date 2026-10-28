# T101 汇报

- 执行者：子 agent（原型实测工程师）
- 分支：`spike/T101-notification-identity`
- 完成日期：2026-10-28

## 做了什么

1. **搭了一个和产品布局相同的原型**，代码在 `spikes/T101-notification-identity/`（Swift，+305 行）：
   - 主 App `dev.cupper.spike.sp10`（SP10 Spike，蓝色 M 图标），里面的 `Contents/Helpers/SP10Host.app` 是 helper，bundle id `dev.cupper.spike.sp10.host`，设了 `LSUIElement`；
   - 主 App 拉起 helper 的方式和产品相同：`posix_spawn` 加 SETSID、CLOEXEC_DEFAULT，环境变量用白名单，不经 LaunchServices；
   - helper 和宿主一样，主线程跑 CFRunLoop；
   - 两种都用 ad-hoc 签名并开 hardened runtime，即自用构建的签法；
   - helper 的 Info.plist 有两种写法：own（自己的显示名称 SP10 Notifier、橙色 H 图标）和 bare（照 T005 现在的写法，只有 `CFBundleName` SP10Host，没有图标）。
2. **实测时原型装在 `/Applications/SP10.app`**，与产品的安装位置相同，经编排者和用户同意。用户在场，逐步做了 s1、s2、s3、s2b、s2c、s4b、s5、s6；后半程按编排者要求一次只发一条通知。
3. **记录**：
   - `docs/tasks/T101-notification-identity/results.md`：MAN-notification-spike 的证据和全部结论；
   - `logs/prototype.log`、`logs/system.log`：脱敏后的日志；
   - `notes.md`：规模和过程；
   - `tests/ids.toml` 末尾追加了 MAN-notification-spike 的证据条目，trace-ids check 通过。

## K5 判定：通过（有条件）

三项判据都做到了：
- **申请授权**：helper 登记后能申请；授权请求、通知和系统设置里用的都是 helper 自己的名称和图标（"SP10 Notifier"、橙色 H），用户认得出。
- **弹通知**：能弹，系统设置里只有 helper 这一条。
- **点击**：回调能回到正在运行的 helper，而且在主线程上，CFRunLoop 就够；helper 能按 bundle id 激活主 App，或者启动主 App 并只交给它一个 URL。

所以按原设计做 T150，不启用 T158。但有四个问题必须由 T150 和设计解决：

1. **嵌套的 helper 不会被 LaunchServices 自动登记。** 主 App 装进 /Applications 后几秒内就登记了，helper 没有；对主 App 做 `lsregister -f` 也不会带上它。没登记时，usernoted 报 "Failed to find or validate client"，返回 `UNErrorDomain 1`，授权框也不弹。
   - 修法：调用 `LSRegisterURL` 登记 helper，而且要在宿主进程启动之前登记。宿主进程里当场登记后，同一进程的第一次通知调用仍被拒过两次。
2. **宿主退出后点通知，系统会自己经 LaunchServices 拉起一个宿主。** 它没有参数，父进程是 launchd，`XPC_SERVICE_NAME` 为 `application.<id>.*`。
   - 这个进程只跑 CFRunLoop 时拿不到点击（s2b，30 秒没收到，主 App 也没被激活）；改跑 NSApplication 后能拿到，并启动了主 App、交了 URL（s2c，用户截图确认）。
3. **宿主在、UI 已退出时点通知，系统也会再拉起一个宿主**（s2、s3）。
4. **宿主在、UI 也在时点通知，系统会弹出 Finder 对话框 "The application "SP10Host" is not open anymore."**，只有 OK 按钮，用户看到了一次。
   - 日志里这时 LaunchServices 返回 -600 procNotFound：系统想经 LaunchServices 把 helper 带到前台，可 helper 不是经它启动的。s1、s5 两次点击都出现了 -600。
   - 用户说对话框是点通知中心里"更早的一条"之后出现的，但日志里只有 6 次点击，每次都对得上自己的那条通知。对话框里的名称和 s5 时的 bare 写法相符，多半是 s5 那一下。

## 给 T150 的结论

**CupperHost.app 的 Info.plist**
- `CFBundleDisplayName`：写成构建种类的显示名称（Cupper、Cupper Local……），通知和系统设置用的是它。
- `CFBundleName`：建议同上，不再用 "CupperHost"，没有显示名称时系统会露出这个名字。
- `CFBundleIconFile`：把 UI 的 icns 复制进 `CupperHost.app/Contents/Resources` 并指向它。系统只用 helper 自己 bundle 里的图标，不会去取外层 App 的。
- `LSUIElement`：保留。
- 用途说明、`CFBundleURLTypes`：都不写。

**名称和图标只在第一次登记时取。** s5 前改成 bare 写法，LaunchServices 也重新登记了，但系统设置里仍是 "SP10 Notifier" 和橙色 H。所以第一个会申请授权的构建（自用构建）就要写对。

**其他要点**
- **授权被拒**：关掉授权请求就记为 denied，之后再申请也不会弹框。界面要提示用户去系统设置里打开。
- **交 URL**：用 `NSWorkspace.open([URL], withApplicationAt:)`，主 App 在 `applicationDidFinishLaunching` 之前就拿到了那一个 URL。连没有登记的 scheme 也送到了（s4b），所以不需要通用路由。
- **找目标 App 建议按路径，不按 bundle id**：由宿主路径往上三级就是外层 App，再核对它的身份。同一个 bundle id 可能登记着好几份，本机 `dev.cupper.app.e2e` 就有 6 条记录。
- **升级**：原地重新构建之后，授权还在。
- **清理**：`removeAllDeliveredNotifications` 有效（s6，用户确认）。

## 设计 5.2.9 要补的规则（交编排者）

1. UI 拉起宿主之前，确保宿主的 bundle 已在 LaunchServices 登记；宿主启动时也检查。
2. 宿主主线程改跑 NSApplication，不进 Dock、不开窗口，取代裸 CFRunLoop。这会动到 5.2.3 和 `cupper-sys::runloop`。
3. 系统单独拉起的宿主只做一件事：等点击回调，把目标交给 UI（UI 在就激活并发 `ui/navigate`，不在就按路径启动它、只交一个 URL），然后退出。
   - 认法：没有参数、没有载荷、父进程是 launchd、`XPC_SERVICE_NAME` 以 `application.` 开头。
   - 它不加 flock、不绑套接字、不开数据库。这一步要排在读 stdin 载荷之前，否则开发和 e2e 构建会读超时退出，点击也就丢了。
4. 有序退出时清掉已送达的通知；崩溃留下的通知靠第 3 条兜底。
5. 宿主上报授权状态；被拒后不重复申请。
6. 在 5.7.9 或 5.10.1 写明宿主要带图标和显示名称。

## 关键取舍

- **装在 /Applications，不放临时目录**：放在临时目录时 LaunchServices 会标成 in-temp-dir，结论要打折扣。
- **own 写法的名称和图标特意与主 App 不同**，系统用的是哪个 bundle 一看便知。
- **授权被误关后，请用户在系统设置里打开**，不换 bundle id：任务书只给了两个 id。
- **加做 s2c**：s2b 表明 CFRunLoop 收不到点击，改跑 NSApplication 才给 T150 找到一条出路。

## 验证方式

验收标准对照：
- [x] 6 项都有现象记录，并给出了 K5 判定和理由（见 results.md 的逐项表）。
- [x] T150 要用的结论：Info.plist 的键、名称和图标的来源、helper 退出后点击的处理；设计 5.2.9 要补的规则，已交编排者。
- [x] MAN-notification-spike 的证据：用户确认了以下现象，截图都没有入库：
  - 授权框弹出了，后被误关；
  - s1 点击后主 App 回到前台；
  - s2c 后窗口出现，里面有 "opened sp10-spike://session/s2c"；
  - 系统设置里是 "SP10 Notifier"、橙色 H，改成 bare 后没变；
  - 通知上的图标是橙色 H；
  - 出现过 Finder 对话框；
  - s6 后通知都没了。
- [x] 遗留清单已写进主仓库 `.private/cleanup/T101-leftovers.md`。仓库里没有个人信息：敏感词表 0 命中，日志里其他应用的 bundle id 已替换。编排者扫描时 `icon_512x512@2x.png` 被当成邮箱，是误报。
- [x] PR #14 的 CI：https://github.com/boris-w/cupper/pull/14 ，运行 https://github.com/boris-w/cupper/actions/runs/40632003208 。交回时 7 个已通过，`rust (macos-15)` 还在跑；编排者合并前核对 8 个全绿。

本机检查：
- `node tools/trace-ids/src/main.ts check` 通过；
- `cargo xtask size`：+305，在补充裁定的停止线 340 以内（硬停止线 400）；
- 与当前 main 能干净合并。

## 提交列表

```
be5fe26 docs(task): record the T101 notification identity results
8aadd83 chore(spike): register the SP-10 helper and run AppKit when the system starts it
81149ea chore(spike): add the SP-10 notification identity prototype
e8afe36 docs(task): estimate T101 by module before starting
```

## 遗留问题 / 范围外发现

1. **没有验证（编排者已定为 T150 的第一条验收）**：正在运行的宿主改跑 NSApplication 之后，问题 3（重复拉起）和问题 4（Finder 对话框）会不会消失。
   - 建议的测法：helper 按 `notify … stay` 跑起来，但改跑 NSApplication，点两次。
     - 第一次在主 App 在、不在前台时点：应当没有 -600，不出 Finder 对话框，主 App 到前台。
     - 第二次在主 App 已退出、helper 还在时点：应当不再拉起第二个 helper，主 App 被启动并拿到 URL。
   - 用 `log show` 查 CoreServicesUIAgent 有没有 -600、launchd 有没有 "Successfully spawned"。
   - 如果消除不了，可以改由 LaunchServices 拉起宿主（与 ADR-01 不同，也会影响 Q21 的 TCC 归属），或者改用 T158。
2. **只有日志、没有用户确认的**：授权框上的图标；通知上的名称；s2b 是不是"点了没反应"（用户没有明确回答）；s3、s4b 窗口里的文字。
3. **没测的**：macOS 14、15；Developer ID 签名；从 DMG 拖进"应用程序"文件夹时，会不会一并登记嵌套的 helper（本机其他 App 嵌套 helper 的登记时间与外层 App 相同）。
4. **K5 通过后的登记**：T158 的编号（UNIT-main-notifications 等）要由编排者标成"不适用（K5 已通过）"。
5. **本机遗留**，清理方法见遗留清单：
   - `/Applications/SP10.app`；
   - 两条 LaunchServices 登记；
   - 系统设置里 "SP10 Notifier" 那一条。没有删除按钮，建议先关掉"允许通知"，再删 App、注销登记、重新登录；会不会自动消失，没有验证。
6. **范围外**：实测时，另一个应用对原型窗口做辅助功能查询，系统日志里留下了 tccd 的记录。原型没有申请任何 TCC 权限，这些行没有入库。
