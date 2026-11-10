# MAN-notification-product：自用构建上的系统通知（T254）

## 执行信息

| 字段 | 内容 |
|---|---|
| 检查表 | [docs/manual/man-notification-product.md](../../manual/man-notification-product.md)，提交 05c5050a |
| 日期 | 2026-11-10 |
| 执行人 | 用户动手、看现象；编排者转述；子 agent 在终端结束进程、读日志并记录 |
| 构建 | 自用构建（Cupper Local）。第 1–6 步用 1b173698；第 7 步查出缺陷，修好后用 1643e8e3（合并了 main）重装，重做第 7 步，再做第 8、11 步 |
| 版本 | cupper 1643e8e3（第 1–6 步为 1b173698）；macOS 26.4 |
| 环境 | 作者的 Mac，Apple 芯片，系统语言英文；agent 用 Claude，在作者自己的一个项目里 |
| 判定依据 | 用户的现场观察和截图（截图不入库）；系统日志（usernoted、NotificationCenter、CoreServicesUIAgent、launchservicesd、launchd）；自用构建的 `host.log`、`main.log`、`host.db`（索引流的 `host.updated`、`notification.intent`，会话流的回合事件，`meta.last_clean_exit_at`）。只读，只摘与通知有关的部分 |

## 逐项结果

| 项 | 结论 | 看到的现象 |
|---|---|---|
| 1 授权请求 | 通过 | 第一次打开时弹出授权请求，系统日志记 "Sending request for permission for dev.cupper.app.local.host"，即以宿主的身份。用户点的是横幅本身（"action"），系统按"不允许"处理，宿主如实记 `notification_auth: denied`；用户随后在系统设置里打开。授权横幅上的名称和图标用户没有逐项确认，名称和图标由第 2、3 步确认 |
| 2 系统设置 → 通知 | 通过 | 只有一条 "Cupper Local"，图标是拿铁；打开后副标题为 "Desktop"。系统这边已允许时，`host.updated` 仍是 denied：宿主只在启动时查询（见"发现"） |
| 3 通知的内容 | 通过 | 横幅图标是拿铁；标题是会话标题；正文 "Approve or decline to proceed."（系统语言英文）；只有关闭的 ×，没有任何操作按钮；没有提示音。日志：needs_action 意图带请求 ID，同时以横幅呈现 |
| 4 界面开着、不在前台时点 | 通过 | 切到对应的会话，请求面板在；Cupper Local 来到前台；没有 Finder 对话框。日志：点击后系统先把宿主设为前台（accessory），约 300 ms 后界面来到前台；没有 -600；宿主没有再被拉起 |
| 5 界面在前台、开着别的会话时点 | 通过 | 切到对应的会话；没有闪；没有 Finder 对话框；切过去后直接按 1，"Yes" 没有被选中，焦点不在请求面板上。日志：点的时候界面已在前台，没有 -600 |
| 6 回合完成、回合失败 | 通过 | 回合完成：日志里两条 turn_completed 意图都以横幅呈现，用户没有逐字核对正文。回合失败：回合进行中由子 agent 结束 Cupper Local 自己的 Claude 适配器进程，回合记为 failed（adapter_exited），弹出横幅：标题是会话标题，正文 "The turn ended with an error."，没有按钮；会话里有出错卡片，待处理的请求随之关掉 |
| 7 界面已退出、宿主还在时点 | 通过（第二次） | 第一次（1b173698）不通过：Cupper Local 被启动，也重启了宿主，但没有切到会话。原因是重启宿主后窗口重新加载，排着的导航被丢掉。修复（511dc4a8）后用 1643e8e3 重做：先短暂显示"无宿主"，弹出重启宿主的确认框，选"重启"后切到了对应的会话；没有 Finder 对话框。日志：宿主交出 URL；launchd 只启动了界面；窗口重新加载后连上，随即订阅了那个会话的流 |
| 8 宿主和界面都不在时点 | 通过 | Cupper Local 被启动；没有确认框，也没有"无宿主"；没有 Finder 对话框。日志：launchd 以 LaunchServices 任务拉起一个宿主，它约 280 ms 内交出 URL 后退出，`host.log` 里没有它的任何一行（没有加锁、绑套接字、开数据库）；随后界面拉起自己的宿主，最后只有一个；窗口连上后订阅了那个会话的流 |
| 9 来到前台稳不稳 | 通过 | 第 4、5 步每次都停在前台，没有被盖回去；日志里界面在点击后约 300 ms 到前台，之后一直停留。`notifier_settle` 300 ms 够用 |
| 10 注销时有序退出 | 未做 | 不做真机验证（用户决定，2026-11-10），由 `crates/cupper-sys/tests/runloop.rs` 的用例覆盖：退出的 Apple Event 交给宿主走和 SIGTERM 相同的有序退出，而不是 AppKit 直接 `exit(0)` |
| 11 正常退出清掉通知 | 通过 | ⌘Q 后通知中心里 Cupper Local 的通知都没了。日志：宿主有序退出，退出码 0；系统日志在同一时刻记 `removeAllDeliveredNotifications`；`meta.last_clean_exit_at` 已写 |

## 发现

1. **第 7 步查出的缺陷，已修复**：带深链接启动 App 时，如果宿主要重启（自用构建里新界面认证不了旧宿主，T215），窗口会在宿主就绪后重新加载，导航目标被第一个文档用掉后就丢了。这不只影响通知：任何"带深链接启动、又要重启宿主"的情况都会丢导航。T254 改为：在窗口的某个文档连上数据连接之前，每次加载都重发最近一个目标（提交 511dc4a8），重做通过。
2. **授权横幅点横幅本身等于拒绝**：macOS 26 的授权请求是通知样式的横幅，"允许"要点横幅上的按钮，点横幅本身系统按"不允许"处理（与 T101 相同）。用户以为点了允许，结果被拒，要到系统设置里打开。
3. **宿主只在启动时查询授权**：用户在系统设置里打开以后，`host.updated.notification_auth` 一直停在 denied，T147 的"去系统设置打开"提示也会一直显示，直到宿主重启。编排者将另立小任务（例如在 App 激活或打开设置页时重新查询）。
4. **界面从不上报 `client/presence`**：宿主不知道窗口正在看哪个会话，所以用户正看着一个会话时，它的审批和回合完成也照样弹通知（第 3 步第一次就是这样）。编排者将另立界面任务。
5. **出错卡片里的 stderr 太长**：适配器退出时，出错卡片直接列出 stderr 的尾部，十几行技术信息。留给 T148 复刻度时考虑默认折叠。
6. **短暂的"无宿主"**（第 7 步，已知现象）：自用构建用一次性钥匙认证宿主，新启动的界面认证不了上一个界面拉起的宿主，先显示连不上，再弹重启确认框（T215）。要等 Q35 换成签名认证才会消失。

## 证据

第 10 步按用户决定不做真机验证，由自动用例覆盖；其余各项都通过，所以追加证据：

```toml
[[evidence]]
id = "MAN-notification-product"
date = "2026-11-10"
record = "docs/tasks/T254-notifications-navigate/man-notification-product.md"
versions = { cupper = "1643e8e3", macos = "26.4" }
```
