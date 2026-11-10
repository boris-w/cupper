# T251 工作笔记

- 执行者：子 agent（宿主与桌面工程师；T150、T250 的执行者）

## 开工前按模块估算（2026-11-10）

按 `cargo xtask size` 的口径（新增行，含测试）。

| 模块 | 内容 | 估计 |
|---|---|---|
| sys：点击的代理 | 给通知中心设代理（`didReceive` 经回调交出点击；`willPresent` 在宿主正好在前台时照常显示横幅）；通知的 `userInfo` 带会话 ID、请求 ID，系统单独拉起的宿主没有数据库，只能靠它；代理活到进程结束 | 90 |
| sys：NSWorkspace | `open([URL], withApplicationAt:)`、`openApplication(at:)`（激活） | 35 |
| 宿主：点击的处理 | 点击经 channel 进 tokio；有 control 连接就发 `ui/navigate`，等一个登记的短时长（排在系统把宿主放到前台之后）再按路径激活外层 App；没有就交 URL；外层 App 按路径往上三级、核对 bundle id；URL 的 scheme 按构建种类；record 模式下交 URL 和激活只记录 | 130 |
| gateway | 只发给 control 连接（`desktop_ui`、角色 `control`），返回有没有发出去 | 20 |
| 系统单独拉起的宿主 | `main.rs` 里排在读载荷之前：认法的纯函数及单元测试；只跑主循环等点击，限时退出（经 `Clock`）；交 URL | 80 |
| 测试入口 | `test/notification_click {notification_id}`（test scope，只在 test-harness 构建）：协议、目录、harness 路由 | 50 |
| 桌面：导航与 `deeplink` | control 上的 `ui/navigate` 转给渲染进程，窗口没加载完先排队；`open-url` 在 ready 之前注册、只认一种路由、校验两个 ID；e2e 和开发构建补 `protocols`；单元测试 | 160 |
| IT-notifier-click | crate 内：测试网关上一条 control 连接收到 `ui/navigate`；没有 UI 时记录的 URL 格式对 | 80 |
| E2E-notification-navigate | record 模式下模拟点击，切到会话，焦点不进请求面板 | 70 |
| **合计** | | **约 715** |

超过停止线 650（硬线 780），按任务书先报告，等编排者裁定再动手。
