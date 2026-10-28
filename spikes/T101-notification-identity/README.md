# T101 SP-10 通知身份

原型实测代码，只用于回答待定问题 Q2（helper 的通知身份），不是产品代码。任务书见 `docs/tasks/T101-notification-identity/brief.md`，记录和结论见同目录的 `results.md`。

## 布局

```
SP10.app                         dev.cupper.spike.sp10，名称 SP10 Spike，蓝色 M 图标，注册 sp10-spike:
  Contents/MacOS/sp10            主 App（app/main.swift）：一个窗口，列出收到的 URL；经 posix_spawn 拉起 helper
  Contents/Helpers/SP10Host.app  dev.cupper.spike.sp10.host，LSUIElement
    Contents/MacOS/sp10-host     helper（host/main.swift）：不建 NSApplication，主线程跑 CFRunLoop
```

`build.sh own`：helper 有自己的显示名称 SP10 Notifier 和橙色 H 图标，名称、图标都与主 App 不同，所以看得出系统用的是哪个 bundle 的。`build.sh bare`：helper 的 Info.plist 照 T005 现在给宿主写的那样，只有 `CFBundleName`，没有图标。两种都是 ad-hoc 签名、开 hardened runtime（自用构建的签法）。

主 App 拉起 helper 的方式与产品相同：`posix_spawn` 加 `POSIX_SPAWN_SETSID`、`POSIX_SPAWN_CLOEXEC_DEFAULT`，fd 0–2 指向 `/dev/null`，环境变量只有 `HOME`、`PATH`，不经 `open` 或 LaunchServices。

## 用法

`SP10_OUT=<目录> ./build.sh own` 之后，`open <目录>/SP10.app --args <主 App 的参数> <helper 的参数>`：

- 主 App：`spawn`（拉起 helper 后留着）、`spawn-quit`（拉起后立即退出，模拟 UI 不在）；
- helper：`notify <id> stay|exit`（申请授权、弹通知，然后等点击或 3 秒后退出）、`open-main <id> [scheme]`（不经点击，直接走点击后的路径）、`status`（授权状态和已送达的通知）、`clear`（先移除已送达的通知）。

helper 收到点击后：主 App 在运行就先 `NSRunningApplication.activate()`，1 秒后还不在前台再 `NSWorkspace.openApplication`；不在运行就用 `NSWorkspace.open([URL], withApplicationAt:)` 只把 `sp10-spike://session/<id>` 交给按 bundle id 找到的那个 App。处理完 5 秒后退出。系统不带参数启动 helper 时，它等 30 秒，期间收到点击照上面处理。

两个进程都把事件追加到 `~/Library/Logs/dev.cupper.spike.sp10.log`。

## 副作用

- 系统设置 → 通知里多出 helper 的条目和它的授权记录；
- LaunchServices 登记 `SP10.app` 和 `SP10Host.app`；
- 上面的日志文件。

清理方法写在主仓库的 `.private/cleanup/T101-leftovers.md`（不入库）。
