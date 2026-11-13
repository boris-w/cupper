# T254 汇报

- 执行者：子 agent（宿主与桌面工程师；T150、T250、T251 的执行者）
- 分支：`feat/T254-notifications-navigate`
- 完成日期：2026-11-10

## 做了什么

开工前按模块重估约 255–275 行，在停止线 300 以内（`notes.md`）。

1. **主进程转发 `ui/navigate`**：control 连接上收到 `ui/navigate {session_id, request_id?}` 后，用和深链接相同的格式校验，再交给导航。
2. **`deeplink` 模块**（`apps/desktop/src/main/deeplink.ts`）：
   - 每种构建一个 scheme：`cupper`、`cupper-local`、`cupper-e2e`、`cupper-dev`；e2e 和开发构建的 electron-builder 配置补上了 `protocols`。
   - 只认 `<scheme>://session/<session_id>[?request=<request_id>]`：scheme 必须是本构建的，两个 ID 按协议格式校验（前缀加 1–64 个 `[A-Za-z0-9_-]`）；多余的路径、参数、片段、用户名、端口一律忽略。
   - `open-url` 在模块加载时就登记，早于 App 完成启动；带着链接启动 App 时，这个链接也会处理。
   - 只导航，不把焦点移到请求面板上。界面这边原本就满足：会话页按会话 ID 重新挂载，面板出现时是 `manual`。
3. **导航怎么送到窗口**（`WindowNavigation`，见下文"两轮修正"）：
   - 主窗口记着最近一个目标，新的进来就替换旧的；
   - 在窗口的某个文档连上数据连接之前，每加载一次文档（包括重新加载）都重发一次；连上之后才算送达；
   - preload 给还没订阅 `onNavigate` 的界面先存着；ShellAPI 的接口没改。
4. **用例**：
   - `deeplink` 的单元测试：合法的两种；别的构建的 scheme、ID 格式不对、多余路径和参数等 16 种都忽略；`WindowNavigation` 连接前重新加载会重发、连接后不再发、新目标替换旧的。
   - E2E-notification-navigate（`tools/e2e/src/notification.spec.ts`）：record 模式下经 harness 连接调 `test/notification_click`，`notification_id` 取索引流 `notification.intent` 的 `intent_id`；断言侧边栏切到这个会话、请求面板是 `manual`、焦点不在面板里。这个用例不需要窗口焦点。
5. **MAN-notification-product**：
   - 检查表 `docs/manual/man-notification-product.md`；
   - 用户在自用构建上做完，记录在 `docs/tasks/T254-notifications-navigate/man-notification-product.md`，证据已追加进 `tests/ids.toml`。

### 两轮修正

- **第一轮（1b173698），e2e-repeat ×10 有 5 次失败**：
  - **抢前台**：导航时我调了 `showMainWindow()`；窗口已经存在时它会 `show()`，在 macOS 上会激活 App。激活外层 App 是宿主的事，record 模式下只记录。改为只在没有窗口时新建，不再 show；真实点击时宿主激活 App，`activate` 事件会把窗口带出来。
  - **导航丢失**（10 次里 3 次）：我用 `webContents.isLoading()` 判断文档是否还在加载。它在 `show()` 之后会短暂地为 true，代码于是去等一个早已发生过的 `did-finish-load`，目标就丢了。改为文档第一次加载完之前先排队。
- **第二轮（511dc4a8），MAN 第 7 步查出"重启宿主后导航丢失"**：
  - 经过：UI 已退出时点通知，宿主交出 URL，App 被启动、目标也送到了第一个文档。但自用构建的新界面认证不了旧宿主（T215），用户确认重启；新宿主就绪后，主进程按 T215 的设计重新加载窗口，路由回到默认页，目标已被第一个文档用掉，没有人再发。
  - **这不只影响通知**：任何"带深链接启动、又要重启宿主"的情况都会丢导航。
  - 修法：上面第 3 条的 `WindowNavigation`。顺带删掉了"App ready 之前排队"的另一套机制，它被这个机制覆盖了。
- **合并 main**（1643e8e3）：PR 曾与 main 冲突（T252 的图标、T253 的命令行安装接线）。只有 `index.ts` 的两行 import 冲突，两边都保留；两份配置自动合并，T252 的 app-icons 和本任务的 `protocols` 都在。合并结果上本机全部检查通过。

## 关键取舍

- **桌面只导航，不把窗口带到前台**：激活外层 App 是宿主的事（设计 5.2.9），record 模式下只记录。桌面一 show 就抢前台，e2e 也会因此失败。
- **送达以"窗口的数据连接建立"为准**：数据连接建立说明这个文档能拿到会话数据、导航不会再被重新加载冲掉。
  - 新目标替换旧的，只记最近一个。
  - 用户在连接建立之前自己点了别处，按现有路由行为处理，没有另加逻辑；如果之后又发生重新加载，记着的目标会再发一次。
- **不用 sessionStorage 存路由**：范围更大，不放在 T254。
- **E2E 不在本机跑**：e2e 版 App 现在带 `cupper-e2e` 的 URL scheme，本机一启动 LaunchServices 就会记下它；只在 runner 上跑。编排者已在交接事项里提醒：以后别的 agent 在本机跑 e2e，系统同样会记下它。
- **MAN 的记录放在卷宗目录，不写进检查表**：按 `docs/manual/README.md`。
- **第 10 步（注销时有序退出）移出人工检查范围**：用户决定，2026-11-10，由 `crates/cupper-sys/tests/runloop.rs` 的用例覆盖（退出的 Apple Event 交给宿主走有序退出，而不是 AppKit 直接 `exit(0)`）。所以"有一项未做就不追加证据"那条规则不适用，证据已追加。

## 验证方式

验收标准对照：
- [x] **E2E-notification-navigate 通过，runner 上连跑 10 次**：
  - 1b173698 上 5/10 失败（42773822820），修正后 10/10 通过（42777513452）；
  - 511dc4a8 上 10/10 通过（42788235502）；
  - 合并后的 1643e8e3 上 10/10 通过（42790276010）。
- [x] **`deeplink` 的单元测试通过**：desktop 单元测试 120 个全部通过。
- [x] **ShellAPI 只做附加**：接口没改，shell-api 的测试和 `frozen.ts` 的检查都通过。
- [x] **主进程的安全约束**：
  - 深链接只导航，参数只接受符合格式的 ID，scheme 只认本构建的；
  - `ui/navigate` 的参数用同一个校验；
  - 不聚焦请求面板：E2E 断言面板是 `manual`、焦点不在面板里；MAN 第 5 步里直接按 1 也不会选中。
- [x] **MAN-notification-product 由用户复核，证据入库**：
  - 结论：第 1–9、11 步通过；第 7 步第一次不通过，修复后重做通过；第 10 步移出人工检查范围，由用例覆盖。
  - 截图没有入库；记录不写路径、项目名、会话标题和对话内容，过了敏感词清单。
  - 证据 `cupper = 1643e8e3`、`macos = 26.4`。`trace-ids status`：MAN-notification 的两个子编号都完成。
- [x] **PR 上 GitHub CI 的 8 个检查都通过；nightly 的 `selfuse`、`release-config` 在分支上通过**：
  - PR #105：https://github.com/boris-w/cupper/pull/105
  - 最终提交 f7f0e5e9 上的 ci 运行：https://github.com/boris-w/cupper/actions/runs/42797385193 ，8 个作业全过；
  - 代码定稿是 1643e8e3，之后几个提交只改文档和 `tests/ids.toml`。它上面的 nightly selfuse https://github.com/boris-w/cupper/actions/runs/42790288012 、release-config https://github.com/boris-w/cupper/actions/runs/42790299476 都通过；
  - E2E ×10：https://github.com/boris-w/cupper/actions/runs/42790276010 。
- **E2E-SEC-fake-host**：1b173698 上的 nightly e2e（42777615137）失败过一次，主进程未捕获 "Object has been destroyed"，栈在 Electron 内部的 `visibilityChanged`，和导航无关。分支上连跑 20 次（42786224093）60/60 通过，按偶发处理。

本机检查：desktop 单元测试、tsc、eslint、prettier（含 `tools/e2e`）、`trace-ids check`，以及 UNIT-host-bundle-identity（合并 main 后要读四份配置）都通过。

规模：`cargo xtask size` +326 −6。其中 `tests/ids.toml` 的证据 +6，代码与测试 +320：`apps/desktop` +263，`tools/e2e` +57。停止线 300、硬线 360，编排者已接受；超停止线的部分来自两轮修正和合并 main 后多行的 import。

## 提交列表

```
f7f0e5e9 docs(manual): step 10 of MAN-notification-product is out of the manual scope
150eabca docs(task): MAN-notification-product evidence; step 10 left to runloop.rs
81890e95 docs(task): MAN-notification-product record
1643e8e3 Merge remote-tracking branch 'origin/main' into feat/T254-notifications-navigate
511dc4a8 fix(desktop): resend the navigation target after a reload until connected
1b173698 fix(desktop): navigation brings no window forward, and waits for the document
05c5050a docs(manual): MAN-notification-product checklist
4e15da4b docs(task): T254 notes: what came up, and the size
5488c9aa test(e2e): E2E-notification-navigate
90d4b2a3 feat(desktop): navigate on the host's ui/navigate and on deep links
06c3d094 docs(task): estimate T254 by module before starting
```

## 遗留问题 / 范围外发现

以下几项单列，都不在 T254 里改：

1. **授权横幅点横幅本身等于拒绝**：macOS 26 的授权请求是通知样式的横幅，"允许"要点横幅上的按钮，点横幅本身系统按"不允许"处理（与 T101 相同）。MAN 第 1 步用户以为点了允许，结果记为 denied，只能去系统设置里打开。
2. **宿主只在启动时查询授权**：用户在系统设置里打开之后，`host.updated.notification_auth` 一直停在 denied，T147 的"去系统设置打开"提示也会一直显示，直到宿主重启。编排者将另立小任务，比如在 App 激活或打开设置页时重新查询。
3. **界面从不上报 `client/presence`**：宿主不知道窗口正在看哪个会话。用户正看着一个会话时，它的审批和回合完成也照样弹通知（MAN 第 3 步第一次就是这样）。编排者将另立界面任务。
4. **出错卡片里的 stderr 太长**：适配器退出时，出错卡片直接列出 stderr 的尾部，十几行技术信息。留给 T148 复刻度时考虑默认折叠。
5. **短暂的"无宿主"**（已知现象）：UI 退出后点通知、App 被重新启动时，自用构建的新界面认证不了上一个界面拉起的宿主（一次性钥匙，T215），先显示连不上，再弹重启确认框。要等 Q35 换成签名认证才会消失。
6. **重启宿主后导航丢失**（已在本任务修复，511dc4a8）：不只影响通知，任何带深链接启动、又要重启宿主的情况都会碰到。合并后建议在设计 5.7.2 的 `deeplink` 一行补一句"窗口的数据连接建立之前，每次加载都重发最近一个目标"。
7. **`notes.md` 没有补后来的事**：两轮修正、合并 main、最终规模都写在本汇报里，没有再提交文档改动，免得 PR 的 CI 重跑。
8. **本机状态**：
   - `/Applications/Cupper Local.app` 是 1643e8e3 的自用版，由用户自己用 ditto 装上（我往 /Applications 复制被权限系统拦下，没有绕过）；
   - 系统设置里 "Cupper Local" 的通知授权已由用户打开；
   - T101 的 `/Applications/SP10.app` 等遗留照旧，等用户决定清理。
