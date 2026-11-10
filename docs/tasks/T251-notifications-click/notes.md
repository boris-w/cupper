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

编排者裁定（第一次）：整体做，停止线 750、硬线 830；`userInfo` 带会话 ID 和请求 ID；外层 App 先核对 bundle id。

## 外层 App 的身份：先核对 bundle id，签名要求留给 T180

宿主路径往上三级就是装着它的那个 App 包，它的 bundle id 必须等于宿主签名标识去掉 `.host`（5.7.9）。现在不核对签名：自用、开发、e2e 构建都是 ad-hoc 签名，签名标识也只是 bundle id；发布构建的 Team ID 到 T180 才有，那时给外层 App 加上签名要求（与 5.8.1 的 UI 要求相同），列在汇报的遗留里。

## 拆分（2026-11-10，main 2d19bfe2，计划 v1.3.92）

宿主这一侧做完时已 +704，桌面导航、`deeplink` 和 E2E 还要约 230，合计约 935，超硬线 830。报告后编排者裁定拆分：

- **T251**：到宿主这一侧为止，验收是 IT-notifier-click。单独合并也有用：点通知能把外层 App 带到前台，UI 不在时能把它启动起来，只是还不会跳到会话。
- **T254（新增）**：桌面主进程转发 `ui/navigate`、`deeplink`（按构建注册 scheme，`open-url` 只认那一种路由）、`deeplink` 的单元测试、E2E-notification-navigate、MAN-notification-product。

## 各模块的实际行数（`cargo xtask size`：+704 −65）

| 模块 | 估计 | 实际 | 文件 |
|---|---|---|---|
| sys：点击的代理与 `userInfo` | 90 | 约 140 | `notify.rs` +136（含 `userInfo` 的往返断言） |
| sys：NSWorkspace | 35 | 约 65 | `launch_services.rs` +61、导出与依赖 |
| 宿主：点击的处理 | 130 | 约 190 | `notifier.rs` +188（含 IT-notifier-click 约 50）、`durations.rs` +7、`index` 接线 |
| gateway | 20 | 27 | `clients.rs` +16、`mod.rs` +11 |
| 系统单独拉起的宿主 | 80 | 约 180 | `notifier/click.rs` +163（外层 App、scheme、URL、认法、`forward`，含单元测试）、`main.rs` +17 |
| 测试入口 | 50 | 约 87 | 协议 +35、fixtures +1、`harness.rs` +23、supervisor +29（也含点击的 channel 和任务） |
| **合计** | 405 | **704** | |

超估主要在两处：sys 的代理类（`define_class!` 加上每个 `unsafe` 的 SAFETY 注释）比估的长；宿主 record 模式要记下交给 App 的动作，系统单独拉起的宿主（主循环、限时、交 URL）也比估的长。
