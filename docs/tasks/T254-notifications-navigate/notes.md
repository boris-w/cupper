# T254 工作笔记

- 执行者：子 agent（宿主与桌面工程师；T150、T250、T251 的执行者）

## 开工前按模块再估一遍（2026-11-10）

按 `cargo xtask size` 的口径（新增行，含测试）。

| 模块 | 内容 | 估计 |
|---|---|---|
| 导航的排队 | 主进程里的一个小模块：App 还没 ready 时先存下目标，ready 之后再交；交的时候显示主窗口，文档还在加载就等 `did-finish-load` | 35 |
| preload | 渲染进程还没订阅 `onNavigate` 时先存下目标，订阅时补交；界面要等 `init()` 回来才渲染，订阅可能晚于文档加载完。ShellAPI 的接口不变 | 15 |
| `deeplink` | 按构建种类的 scheme 表；只认 `<scheme>://session/<id>[?request=<id>]`，scheme 必须是本构建的，两个 ID 按协议格式校验，多余的路径、参数、用户名、端口、片段都不认；`ui/navigate` 的参数用同一个校验 | 40 |
| 接线 | `index.ts`：`open-url` 在模块加载时登记（早于 `will-finish-launching`）；control 上的 `ui/navigate` 转给导航；ready 之后放行 | 20 |
| 打包 | e2e、开发构建的 electron-builder 配置补 `protocols` | 8 |
| `deeplink` 的单元测试 | 合法的两种；别的构建的 scheme、ID 格式不对、多余的路径或参数等；导航的排队 | 60 |
| E2E-notification-navigate | record 模式下用 CLI 建一个会请求批准的会话，从索引流取 `notification.intent` 的 `intent_id`，经 harness 连接调 `test/notification_click`；断言界面切到这个会话、请求面板出现但是 `manual`、焦点不在面板里 | 75 |
| **合计** | | **约 255–275** |

在停止线 300 以内，开工。

界面这边已经满足"导航不聚焦请求面板"：会话页按会话 ID 重新挂载（`key={route.sessionId}`），新挂载时 RP-3 记的"焦点在输入框里"是 false，所以面板出现时是 `manual`。E2E 会验证这一点。
