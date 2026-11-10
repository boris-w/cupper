# T251 汇报

- 执行者：子 agent（宿主与桌面工程师；T150、T250 的执行者）
- 分支：`feat/T251-notifications-click`
- 完成日期：2026-11-10

## 做了什么

**估算与拆分。**
- 开工前估约 715 行，报告后编排者定为整体做，停止线 750、硬线 830。
- 宿主这一侧做完时已经 +704，桌面和 E2E 还要约 230，合计会超硬线。报告后编排者裁定拆分：本任务做到宿主这一侧为止，验收是 IT-notifier-click；桌面导航、`deeplink`、E2E-notification-navigate、MAN-notification-product 拆给新任务 T254。

**cupper-sys**（`unsafe` 只在这里）：
- 通知的 `userInfo` 带会话 ID 和请求 ID。系统单独拉起的宿主不开数据库，只能靠它。
- 给通知中心设代理：
  - 点击经回调在主线程交出（通知 ID、会话 ID、请求 ID）；
  - 宿主正好在前台时，横幅照常显示；
  - 代理由调用方持有，通知中心只弱引用它。
- NSWorkspace：
  - `open([URL], withApplicationAt:)`：只把 URL 交给指定的那一个 App，它没在运行就启动它；
  - `openApplication(at:)`：把 App 带到前台。

**宿主**：
- **点击的处理**：点击经 channel 进入 tokio 里的一个任务。
  - 有桌面 UI 的 control 连接时（`desktop_ui`、角色 `control`）：发 `ui/navigate {session_id, request_id?}`，再等一个登记的短时长（`notifier_settle`，300 ms），排在系统把宿主放到前台之后，然后把外层 App 带到前台。
  - 没有 control 连接时：把 `<scheme>://session/<id>[?request=<id>]` 交给外层 App，它没在运行就启动它。
- **外层 App**：从宿主 bundle 往上三级，它的 bundle id 必须等于宿主签名标识去掉 `.host`；scheme 按构建种类取（`cupper`、`cupper-local`、`cupper-e2e`、`cupper-dev`）。
- **gateway**：新增 `notify_controls`，只发给 control 连接，返回有没有发出去。
- **系统单独拉起的宿主**（`main.rs` 最前面，排在读载荷之前）：
  - 认法：没有参数、父进程是 launchd、`XPC_SERVICE_NAME` 以 `application.` 开头；
  - 先设代理，再跑主循环，限时等点击（`notifier_forward`，10 s，经 `Clock`）；
  - 收到点击就交 URL，然后退出；
  - 不加锁、不绑套接字、不开数据库；
  - 只在不带 `test-harness` 的构建里存在。
- **正常的宿主**：只在经 UserNotifications 发通知的构建里，在 AppKit 完成启动之前设代理。
- **record 模式**：
  - `ui/navigate` 照常发；交 URL 和激活只记录，不碰 LaunchServices；
  - 新增测试入口 `test/notification_click {notification_id}`，按已记录的通知构造点击，走同一条处理路径。

**用例**：
- IT-notifier-click：
  - crate 内，测试网关加一条真实的 control 连接；
  - 没有 UI 时，记下的 URL 格式对；
  - 有 UI 时，control 连接收到 `ui/navigate`，参数对，并记下了激活；
  - 认法的单元测试：参数、父进程、任务名，各缺一项都不算。
- scheme 与 URL 的单元测试；`userInfo` 往返的断言补进了 UNIT-notifier-no-actions。

## 关键取舍

- **`userInfo` 带 ID，不按意图 ID 反查**（编排者同意）：系统单独拉起的宿主没有数据库。
- **外层 App 先核对 bundle id**（编排者同意）：那个路径本来就是装着这个宿主、已签名的 App 包；自用、开发、e2e 构建都是 ad-hoc 签名，签名标识也只是 bundle id。签名要求见遗留第 1 条。
- **激活排在系统之后**：等 300 ms 再用 `openApplication`。这是 T150 第 0 步的发现：点击后系统会先把宿主放到前台，盖掉宿主当场做的激活。
- **系统单独拉起的宿主一律交 URL**：它没有 control 连接；UI 在运行时，URL 经 `open-url` 送到，同样只导航（设计 5.2.9 第 3 条）。
- **测试入口接在 record 后端上**：按已记录的通知 ID 构造点击，再送进真实点击用的同一个 channel，所以测试走的路径和真实点击一样。

### `test/notification_click` 在权限矩阵里的位置（供合并时核对设计 08）

- scope 是 `test`，在设计 5.8.3 的 `test/*` 那一档：只有 `test_harness` 主体可以调用；UI control、UI window、cli（无论带不带 write、approve）、remote_device 一律拒绝。
- **只在 `test-harness` 构建里存在**（e2e 构建和宿主的集成测试构建）。发布、自用、开发构建里不登记这个方法，所以根本不存在。
- 代码里的对应：
  - 协议的 `builds: [TestHarness]`；
  - 目录测试里 `test/*` 一行和 `x-builds` 断言；
  - 宿主权限矩阵 `unit_policy_matrix` 里 `test/*` 那一行（"NNNNNBN"）；
  - 分派表里 `Builds(HARNESS)`。
- 5.8.3 的 `test/*` 是通配的一行，按现有写法已经覆盖，不用新增一行；如果设计要逐个列出 `test/*` 的方法，就补上它。
- 错误：没有这条已记录的通知时，返回 `not_found`。

## 验证方式

验收标准对照（拆分后）：
- [x] IT-notifier-click 通过：本机 `cargo test -p cupper-host --all-features` 全套通过；CI 的 `rust` 作业通过。
- [x] 宿主的约束都守住：
  - `unsafe` 只在 cupper-sys；
  - 两个新时长都登记在 `durations.rs`，计时都经 `Clock`；
  - 没有新的子进程：交 URL 和激活走 NSWorkspace；
  - 协议只做附加（新增一个 test 方法），本机 protocol 作业和 CI 的 schema-compat 都通过；
  - 四种特性组合下 clippy 都无告警。
- [x] 不在本机碰系统状态：本机的用例一律走 record，没往 `/Applications` 装东西，没改登记，没碰通知服务和授权。
- [x] PR 上 GitHub CI 的 8 个检查都通过；nightly 的 `selfuse`、`release-config` 在分支上通过：
  - PR #103：https://github.com/boris-w/cupper/pull/103
  - ci 运行：https://github.com/boris-w/cupper/actions/runs/42764723874 ，8 个作业全过
  - cupper-sys.yml 的 security-peer（macos-26）通过：https://github.com/boris-w/cupper/actions/runs/42764723664
  - nightly（selfuse 那次）：https://github.com/boris-w/cupper/actions/runs/42764723192 ，通过
  - nightly（release-config 那次）：https://github.com/boris-w/cupper/actions/runs/42764731499 ，通过
- 拆到 T254 的验收（`deeplink` 的单元测试、E2E-notification-navigate、ShellAPI 的检查、MAN-notification-product）不在本任务里。

本机其他检查：cupper-sys 全部测试、fmt、prettier、eslint、`trace-ids check` 都通过。

规模：`cargo xtask size` +710 −67，在停止线 750 以内。按模块的实际行数写在 notes.md：sys 约 205、宿主约 470、协议约 36。

## 提交列表

```
fef0d1c0 docs(task): T251 notes: the split into T254, and the size by module
8149f30e feat(host): a click on a notification goes to its session
9b932fd7 feat(protocol): test/notification_click
9af93f6e Merge branch 'main' into feat/T251-notifications-click
e0a7bc75 docs(task): estimate T251 by module before starting
```

把 main 合进来之后，我重排过本地还没推送的提交，让每个提交单独也能通过构建和测试：协议提交里带上了宿主的分派表和权限矩阵。编排者已提醒：守则第 1 条不让子 agent 执行 rebase，以后要整理提交历史，先报告，由编排者决定。

## 遗留问题 / 范围外发现

1. **交给 T180：外层 App 的签名要求。** 现在只核对外层 App 的 bundle id（从宿主路径往上三级的 Info.plist）。T180 有了发布构建的 Team ID 之后，点击前还要核对外层 App 的代码签名满足 UI 的要求，与设计 5.8.1 里宿主对 UI 主进程的要求相同。不满足就不激活、不交 URL。
2. **交给 T254**：
   - 桌面主进程转发 control 上的 `ui/navigate`；
   - `deeplink`：e2e 和开发构建补注册 scheme；`open-url` 只认 `<scheme>://session/<id>[?request=<id>]`，两个 ID 按协议格式校验（前缀加 1–64 个 `[A-Za-z0-9_-]`）；App 还没启动完收到的 URL 也要处理；
   - E2E 用 `test/notification_click`（harness 连接，`notification_id` 取索引流 `notification.intent` 里的 `intent_id`）。
   - 现在的状态：点通知已经能把外层 App 带到前台，UI 不在时能把它启动起来，只是还不会跳到会话。
3. **没有在本机验证的**：真实的点击、激活的时机（300 ms 够不够）、系统单独拉起的宿主（守则不允许在本机做），都留给 T254 的 MAN-notification-product。注销时宿主走有序退出这一条，也在那一次一起复核（交接事项里 T150 那一行）。
4. **设计要补的**：设计 5.2.9 的"通知的点击"和第 3 条可以写上两个时长：`notifier_settle` 300 ms，`notifier_forward` 10 s。
5. **本机遗留**：没有新增；T101 的 `/Applications/SP10.app` 等照旧，等用户决定清理。
