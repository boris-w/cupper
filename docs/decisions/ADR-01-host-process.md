# ADR-01 会话宿主的进程形态与生命周期

- 状态：提议
- 日期：2026-10-16
- 来源：`docs/meetings/2026-10-16-tech-selection/`（`minutes.md` 中的 C6；`architect.md` ADR-01 一节；三份第二轮回应）

## 背景

PRD v1.0 要求会话、历史、审批、Git、worktree 都由一个与界面分离的会话宿主持有（NFR-1）。还有几条相关要求：

- NFR-8：界面崩溃不影响会话；后台常驻在 M3 之前完成。
- NFR-9：崩溃恢复。
- NFR-6：冷启动 2 秒内可用。
- SEC-8：宿主不监听网络端口。

宿主同时是本地鉴权的信任锚（ADR-05），M3 起还要保存设备身份密钥。

## 可选方案

1. 作为 UI 进程里的一个模块
2. 随 UI 退出的子进程，例如 Electron 的 utilityProcess
3. 独立的进程，由 App 按需拉起，但不随 UI 退出
4. launchd LaunchAgent（SMAppService）
5. 内嵌在 App 里的 XPC 服务

方案 1、2 违反 NFR-8，而且只能靠自觉维持边界。方案 5 只能由本 App 连接，CLI 和远程桥接不进来，空闲时还会被系统杀掉。

## 决策

**M1 采用方案 3；M2 增加方案 4，作为可选的后台常驻模式。两种模式使用同一个二进制、同一个套接字和同一套协议，区别只在于由谁来启动。**

用户已接受：开启后台常驻后，系统会提示"已添加后台项目"，cupper 也会出现在登录项设置里（2026-10-16，U-T3）。

**打包形态**：宿主做成 app-like 的 helper bundle，放在 `Cupper.app/Contents/Helpers/CupperHost.app`，设置 LSUIElement，有自己的 identifier。这样做有三个好处：
- 宿主有独立的 TCC 身份和通知身份；
- M3 可以在这里嵌入描述文件，使用数据保护钥匙串；
- M2 的 LaunchAgent plist 可以放在 `Contents/Library/LaunchAgents`，按相对路径指向它。

Node 和适配器不放进包里，而是按锁定的版本装在 `~/Library/Application Support/<App>/runtime/` 下（由 ADR-08 细化）。

**生命周期规则**

| 场合 | 行为 |
|---|---|
| 打开 App | UI 连接套接字；连不上就拉起宿主（`posix_spawn` 加 `POSIX_SPAWN_SETSID`），总预算约 500 ms |
| 单实例 | 宿主对数据目录里的锁文件加 `flock`，拿到锁之后才绑定套接字 |
| UI 崩溃或重载 | 宿主不受影响，UI 重新连接后按游标续传 |
| 宿主崩溃 | UI 重新拉起宿主。宿主启动时按 NFR-9 恢复：按进程组和启动时间清理残留的适配器；把运行中的回合标记为"已中断"；把待处理的交互请求标记为"已失效" |
| 退出 App | 有运行中的回合或待处理的请求时先提示；确认后，在非后台模式下有序退出 |
| 空闲 | 非后台模式下，如果没有客户端、没有运行中的回合、没有待处理的请求，持续 N 分钟后退出 |
| 升级 | 用 electron-updater 并关闭 `autoInstallOnAppQuit`，由我们自己控制安装时机。顺序是：UI 通知宿主"待升级" → 宿主空闲后退出 → 安装 → 新 UI 拉起新宿主。宿主运行期间不读取包内的任何资源；每次拉起 Node 或适配器之前都比对它们的 cdhash，不一致就转为"待升级"。M2 的后台模式下，关闭 UI 时不安装更新 |
| 系统通知 | 全部由宿主 helper 弹出，UI 只上报"窗口是否在前台" |
| 防休眠（NFR-13） | 宿主持有 IOPMAssertion，用完立即释放 |
| 数据目录 | `~/Library/Application Support/<App>/`，权限 0700，里面放 SQLite、blob、锁文件和套接字；日志放在 `~/Library/Logs/<App>/` |

## 后果

- **好处**：满足 NFR-8；用进程边界来防止 AI 写代码时跨层调用；M1 到 M3 的进程形态不需要改。
- **代价**：要多管理一个签名的 helper 和它的生命周期；升级流程更复杂。
- **需要 SP-4 验证**：
  - 由 App 拉起和由 launchd 拉起时，TCC 授权分别归属谁；
  - `kill -9` 宿主之后，残留进程能否清理干净；
  - 冷启动时宿主侧 p95 不超过 400 ms；
  - 没有 Developer ID 时 SMAppService 的表现；
  - 升级演练：宿主和适配器都在运行时替换整个 .app，检查旧宿主能否继续运行、签名校验会不会失败、能否平滑切换到新版本；
  - helper 在系统通知设置里显示的名称和图标。
- **后备方案**：
  - 升级演练失败：把宿主复制到包外按版本区分的目录，包内放一个启动器。
  - TCC 授权归属有问题：M1 就直接改用 LaunchAgent。
