# T150 系统通知与 ui/navigate

- 派发日期：2026-11-10
- 执行者：子 agent（宿主与桌面工程师，cupper-sys、宿主、桌面主进程）
- 分支：`feat/T150-notifications`
- 依赖：T099、T101（都已合并）。T155、T170、T148 依赖本任务

## 范围

**做**（计划 T150 一行；设计 5.2.9 的"宿主 helper 的通知身份"六条和"通知的点击"，2026-11-10 补写；5.2.3、5.2.13、5.7.2、5.7.8、5.10.1；PRD AP-3）：

0. **先补验 T101 没验证的一项（T150 的第一条验收，设计 5.2.9 末尾）**：
   - **问题**：正在运行的宿主改跑 NSApplication 之后，下面两种现象是否都消失：
     - 宿主在、UI 已退出时点通知，系统又拉起一个宿主；
     - 宿主、UI 都在时点通知，弹出 Finder 对话框 "…is not open anymore"。
   - **做法**：用 T101 的原型 `spikes/T101-notification-identity/` 测。按它汇报"遗留问题"第 1 条的测法：helper 按 `notify … stay` 跑起来，但改跑 NSApplication，点两次。
     - 第一次在主 App 在、不在前台时点：应当没有 -600，不出 Finder 对话框，主 App 到前台。
     - 第二次在主 App 已退出、helper 还在时点：应当不再拉起第二个 helper，主 App 被启动并拿到 URL。
     - 用 `log show` 查 CoreServicesUIAgent 有没有 -600，launchd 有没有 "Successfully spawned"。
   - **准备和在场**：
     - 原型要装在 `/Applications/SP10.app`，替换现在那一份，需要用户同意；点通知要用户在场，约 10 分钟。
     - 先把原型改好、构建好，然后 SendMessage 告诉 `main`，由编排者约用户。
     - 原型的改动提交在本分支的 `spikes/` 下，结果写进本目录的 `results.md`，日志脱敏。
   - **不通过就停下来报告**：两种现象有任何一种还在，就不往下做。由编排者和用户决定改由 LaunchServices 拉起宿主（影响 ADR-01、Q21），或者启用 T158。
   - 等用户期间可以先做第 1 步的估算。
1. **开工前按模块估算**：sys、notifier、宿主的其他改动、桌面、打包、用例各估一遍，写进 `notes.md`。估算超过停止线就先报告，由编排者拆分，不要动手。
2. **cupper-sys**（`unsafe` 只在这里）：
   - 主线程改跑 NSApplication（accessory，不进 Dock、不开窗口），取代 `runloop.rs` 的裸 CFRunLoop。
   - UserNotifications：
     - 申请授权、查询授权状态；
     - 弹通知，不注册任何 category 和 action；
     - 点击回调经 channel 交出；
     - `removeAllDeliveredNotifications`。
   - LaunchServices 与 NSWorkspace：
     - `LSRegisterURL`，以及查询自己是否已登记；
     - `NSWorkspace.open([URL], withApplicationAt:)`；
     - 按路径激活 App。
   - 引入 objc2 系列 crate 时锁定精确版本，许可证要在 ADR-11 的白名单里（`license` 检查会挡）。
3. **宿主 `notifier`**：
   - **接入点**：`IndexActor::step` 处理 `Action::Intent` 的地方，或者读索引流里的 `notification.intent`（交接事项 T150 那几行）。
   - **文案**：内置中英文案，跟随系统语言，措辞参照 Codex（ui-spec 第 4.8 节）。
     - 标题用会话标题，没有时用内置文案。
     - 正文只用按原因选的内置文案（需要批准、需要回答、回合完成、回合失败），不放 agent 的原文（设计 5.2.9）。
   - **开关**：设置 `notifications.enabled`，默认开。设置页的界面在 T147。
   - **`test_config.notifier: record`**：只记录、不弹，用例据此断言；测试构建另给一个模拟点击的入口。
   - **点击**：
     - 经 control 连接给桌面主进程发 `ui/navigate {session_id, request_id?}`；
     - 激活外层 App：宿主路径往上三级，核对身份后再激活；
     - UI 不在时，用构建种类的 URL scheme 交一个 URL 启动它（设计 5.2.9）。
   - **授权状态**：写进 `host.updated.notification_auth`（附加），被拒后不重复申请。
   - **系统单独拉起的宿主**：只转交点击后退出，排在读 stdin 载荷之前（5.2.13 第 1 步）。
   - **登记与清理**：启动时检查并补登记；有序退出时清掉已送达的通知。
4. **桌面主进程**：
   - `hostLauncher` 拉起之前经 `cupper_native` 确保宿主已登记（5.7.2）。
   - 收到 control 上的 `ui/navigate` 后转给渲染进程，`ShellAPI.onNavigate` 只导航，不把焦点移到请求面板上（5.7.8）。
   - `deeplink` 模块：
     - 按构建种类注册 URL scheme（5.7.9；交接事项里 T074 那一行）；
     - `open-url` 用白名单路由，参数只接受符合格式的 ID，只做导航（5.7.8 "其他入口"）；
     - App 还没启动完就收到的 URL 也不能丢。
5. **打包**：`cargo xtask host-bundle` 给 `CupperHost.app` 的 Info.plist 写上：
   - `CFBundleDisplayName`、`CFBundleName`：构建种类的显示名称；
   - `CFBundleIconFile`：把 UI 的 icns 复制进 `Resources/` 并指向它（设计 5.10.1）。
   - 名称和图标只在第一次登记时取，所以第一份申请授权的构建（自用构建）就要写对。
6. **用例**（都已登记）：
   - **IT-notifier-record**（crate 内）：
     - 三种原因各弹一条，文案正确；
     - 开关关掉就不弹；
     - 前台 `desktop_ui` 正在看这个会话时不弹；
     - 模拟点击后 control 连接收到 `ui/navigate`。
   - **UNIT-notifier-no-actions**：系统通知没有任何操作按钮。
   - **E2E-notification-navigate**：record 模式下模拟点击，UI 切到对应的会话，焦点不进请求面板。
   - **MAN-notification-product**：作者在自用构建上复核真实的授权框、通知、点击和名称图标。证据格式按 `docs/manual/`。最后由编排者约用户，不要自己找用户。

**不做**：
- 设置页"通知"的界面和授权被拒时的提示（T147，计划 v1.3.90）；
- 通知上的操作按钮和在通知里直接回复（5.7.8，有意与 Codex 不同）；
- 提示音；
- 主进程弹通知的后备方案（T158，K5 第一条验收不通过时再议）；
- M3 的推送。

## 验收标准

- [ ] 第 0 步的补验通过，结果写进 `results.md`（不通过就停在这一步）。
- [ ] IT-notifier-record、UNIT-notifier-no-actions 通过；E2E-notification-navigate 通过，并在 runner 上连跑 10 次（由 `main` 触发 e2e-repeat）。
- [ ] 宿主的约束都守住：
  - `unsafe` 只在 cupper-sys；
  - 时间只经 `Clock`；
  - 子进程只经 `cupper-sys::spawn`；
  - 索引流只有 `IndexActor` 一个写者；
  - 协议只做附加，schema-compat 0 个破坏性改动。
- [ ] ShellAPI 如果要改，只做附加，`frozen.ts` 的检查通过。
- [ ] MAN-notification-product 由用户复核，证据入库。截图不入库，入库的日志过敏感词清单。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 计划估为 L。先按第 1 步估算再动手。
- 停止线：800；硬停止线：950。超过时由编排者拆分：宿主侧和桌面侧分开，或者把 `deeplink` 单独拿出去。
- 原型的改动不计入规模，单列在 `notes.md` 里。
- 不要靠压缩代码或注释去守线。

## 参考资料

- 设计：
  - 5.2.3（主线程）、5.2.9（关注度与通知，含六条规则和第一条验收）、5.2.13（启动顺序第 1 步、有序退出）；
  - 5.3（`ui/navigate`、`host.updated`）；
  - 5.7.2（`hostLauncher`、`deeplink`）、5.7.8（通知不放按钮、其他入口只导航）、5.7.9（构建种类的 bundle id、显示名称、URL scheme）；
  - 5.10.1（包结构、宿主的名称和图标）。
- 卷宗：T101（`report.md`、`results.md`、原型代码）；T100（索引的通知意图）；T074（URL scheme 尚未注册）。
- ui-spec 第 4.8 节（Codex 的通知文案和不发的条件）。
- 交接事项：`docs/tasks/handoff-notes.md` 里接收任务为 T150 的各行。
- 本机遗留：`.private/cleanup/T101-leftovers.md`（`/Applications/SP10.app` 和两条 LaunchServices 登记仍在）。

## 注意

- **用户在场的步骤只有两次**：第 0 步点通知、最后的 MAN-notification-product。都由 `main` 约用户，执行者不直接找用户。
- **不在本机碰系统状态**，除非 `main` 转达了用户的同意：
  - 不往 `/Applications` 装东西；
  - 不改 LaunchServices 的登记（`lsregister`、`LSRegisterURL` 指向真实的 bundle）；
  - 不改通知授权。
  - 测试和 e2e 一律用 record 模式。
- 不在本机跑会抢焦点的用例（守则第 10 条）。
- 不运行真实的 Claude 或 Codex CLI。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者给你开好 worktree `.claude/worktrees/T150`，分支 `feat/T150-notifications` 从 main 开出。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
