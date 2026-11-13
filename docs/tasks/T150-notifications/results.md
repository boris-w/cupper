# T150 第 0 步：宿主改跑 NSApplication 后的点击补验

- 日期：2026-11-10，10:37–10:43（本机时间）
- 环境：macOS 26.4（25E246），arm64；Swift 6.3.3
- 原型：`spikes/T101-notification-identity/`，本分支的改动只有一处：helper 在所有模式下都跑 NSApplication（accessory），不再只在系统单独拉起它时才跑；启动 1 秒后记下 LaunchServices 看到的运行中的 helper（`running helpers=`）。
- 安装：经编排者转达用户同意，在 `/Applications/SP10.app` 原地重建（`SP10_OUT=/Applications ./build.sh own`），bundle id 不变；helper 已登记，没有新增登记，没有改通知授权。
- 日志：`logs/prototype.log`（原型两个进程的事件，UTC）、`logs/system.log`（`log show` 摘录，本机时间）。都已脱敏：前台的其他应用换成 `<another app>`、`<other>`。原始日志文件已删除。
- 本文是第一条验收（设计 5.2.9 末尾）的证据。"用户观察"一栏只写编排者转述的用户原话；截图没有入库。

## 结论：通过

两种现象都消失了：

1. **"宿主在、UI 已退出时点通知，系统又拉起一个宿主"**：第 2 次点击时，系统把正在运行的 helper 带到前台，没有再经 launchd 拉起 helper；helper 启动了主 App，只交给它一个 URL。
2. **"宿主、UI 都在时点通知，弹出 Finder 对话框 …is not open anymore"**：三次在主 App 运行时的点击，CoreServicesUIAgent 都是 "Found application … to open application …" 之后直接把 helper 设为前台，没有 -600 procNotFound，用户也没见过对话框。

整个时段的系统日志里没有一条 `procNotFound`，launchd 也没有启动过 `application.dev.cupper.spike.sp10.host.*` 任务；出现过的 helper 进程都是主 App 用 `posix_spawn` 拉起的（launchd 记为 `unmanaged`）。

原因与 T101 的推断一致：helper 跑 NSApplication 之后，LaunchServices 认得这个进程（`running helpers=[<自己的 pid>]`），所以系统"把 helper 带到前台"时找得到它，不再报 -600，也不再另起一个。

所以按设计做 T150，不需要改由 LaunchServices 拉起宿主，也不需要启用 T158。

## 逐次记录

| 次 | 条件 | 点击（本机时间） | 回调 | LaunchServices / launchd | 主 App | 用户观察 |
|---|---|---|---|---|---|---|
| 预热 | 直接运行 `sp10-host status` | — | — | 已登记；授权 2（authorized），提醒开着 | — | — |
| 1 | `spawn notify t1 stay`：主 App 在，点击前用户切到别的应用 | 10:38:18.037 | 正在运行的 helper（18837）收到，主线程 | "Found application"，无 -600；SETFRONT 依次为主 App、helper（`foreground=0`）、主 App；无 launchd 拉起 | `activate()` 后被系统转给 helper，1 秒后 `openApplication` 让主 App 回到最前 | "窗口回到最前了，没有对话框" |
| 2 | `spawn-quit notify t2 stay`：主 App 拉起 helper 后退出 | 10:41:13.084 | 正在运行的 helper（19978）收到 | SETFRONT helper（`foreground=0`），无 -600；launchd 只启动了主 App（22107），没有启动 helper | 被启动，收到 `sp10-spike://session/t2` | "出现了 并且有opened sp10-spike://session/t2"；整个过程没见过对话框 |
| 3 | `spawn notify t3 stay`：本应主 App 在最前，实际点击时最前是另一个应用 | 10:42:27.272 | helper（23155）收到 | 同第 1 次，无 -600，无 launchd 拉起 | 同第 1 次 | "只有一个 [截图] 这个，没有看到finder图标和is not open anymore之类的"（截图里是在最前的 SP10 Spike 窗口） |
| 3b | `spawn notify t3b stay`：重做第 3 次 | 10:43:00.678 | helper（24145）收到 | "Found application"，无 -600；SETFRONT helper，6 ms 后主 App；无 launchd 拉起 | `activate()` 生效，主 App 在最前 | "没有弹出那种对话框 ，SP10 Spike在最前面"；用户说先点了 SP10 Spike 窗口，再从通知中心点 |

- 第 3 次、3b 本想复现 T101 s5 的条件（点击那一刻主 App 就在最前）。按 launchservicesd 的 SETFRONT 记录，两次点击的那一刻最前的都是另一个应用（3b 前 2 秒前台从主 App 换到了别的应用），和用户的说法不一致，以日志为准就是没有严格做到这个条件。不影响结论：T101 的 s1 主 App 不在最前，同样出了 -600；-600 的原因是 LaunchServices 找不到可激活的 helper 进程，与主 App 在不在最前无关，这次四次点击都找到了。
- 主 App 和 helper 的 pid、bundle 路径见两份日志。

## 给 T150 实现的新发现

1. **点击后系统会把宿主设为前台**：helper 跑 NSApplication 之后，系统"把 helper 带到前台"真的会让它成为前台应用（accessory，`foreground=0`）。第 1、3 次 helper 先 `activate()` 了主 App，十几毫秒后系统把前台给了 helper，主 App 靠原型 1 秒后的 `openApplication` 才回到最前；3b 的顺序反过来，`activate()` 直接生效。所以宿主激活外层 App 不能只在回调里调一次 `activate()`，要等系统这一下之后再做，或者改用 `NSWorkspace.openApplication(at:)`（第 1、3 次就是它生效的）。
2. **LaunchServices 认得运行中的宿主**：`NSRunningApplication.runningApplications(withBundleIdentifier:)` 能查到 helper 自己，可以用来确认 NSApplication 已经接上。
3. **授权、登记没有因原地重建而丢失**：预热时已登记、授权仍为 authorized（T101 后用户打开的那次）。

## 本机遗留

- `/Applications/SP10.app` 仍在（这次是 own 写法），LaunchServices 的两条登记和系统设置里的 "SP10 Notifier" 照旧，清理方法见主仓库 `.private/cleanup/T101-leftovers.md`，没有新增遗留。
- 原型日志文件 `~/Library/Logs/dev.cupper.spike.sp10.log` 已删除。
