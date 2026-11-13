# T251 系统通知 III：点击与导航

- 派发日期：2026-11-10
- 执行者：子 agent（宿主与桌面工程师；T150、T250 的执行者）
- 分支：`feat/T251-notifications-click`
- 依赖：T250（已合并）。T155、T170、T148 依赖本任务

## 拆分（开工后，2026-11-10，计划 v1.3.92）

宿主这一侧做完（+704）后推算约 935，越过硬线 830。下面第 5 条（桌面主进程的导航和 `deeplink`）、第 6 条里的 E2E-notification-navigate、`deeplink` 的单元测试和 MAN-notification-product 拆到新增的 T254。本任务到宿主这一侧为止：第 1 至 4 条和 IT-notifier-click。合并后点通知已能激活或启动 App，T254 接上跳到会话。

## 范围

**做**（计划 T251 一行；设计 5.2.9（通知的点击、六条规则的第 3 条）、5.2.13 第 1 步、5.7.2 的 `deeplink`、5.7.8、5.7.9；PRD AP-3）：

1. **点击回调**（cupper-sys，`unsafe` 只在这里）：
   - 给通知中心设代理，点击经 channel 交出（交接事项里 T250 那一行）。
   - 通知的 `userInfo` 带会话 ID 和请求 ID，或者由通知 ID（意图 ID）反查，你定。
2. **宿主处理点击**：
   - **UI 在**：经 control 连接给桌面主进程发 `ui/navigate {session_id, request_id?}`。
   - **激活外层 App**：宿主路径往上三级，核对身份后激活。要排在系统把宿主放到前台之后，或者用 `NSWorkspace.openApplication`（T150 第 0 步的发现，设计 5.2.9）。
   - **UI 不在**：用构建种类的 URL scheme 经 `NSWorkspace.open([URL], withApplicationAt:)` 交一个 URL：
     - 格式是 `<scheme>://session/<session_id>`，带请求时加 `?request=<request_id>`；
     - 不走通用路由（安全 #7）。
3. **系统单独拉起的宿主**（5.2.9 第 3 条、5.2.13 第 1 步）：
   - 认法：没有参数、父进程是 launchd、`XPC_SERVICE_NAME` 以 `application.` 开头。
   - 只等点击回调，一律交 URL，然后退出；不加 `flock`、不绑套接字、不开数据库。
   - 排在读 stdin 载荷之前。
4. **测试构建模拟点击的入口**：
   - 接在 `test_config.notifier: record` 上，例如一个 test 级的方法，按通知 ID 模拟点击。
   - 走和真实点击相同的处理路径；交 URL 和激活在 record 模式下只记录，不碰 LaunchServices。
5. **桌面主进程**：
   - **导航**：收到 control 上的 `ui/navigate`，转给渲染进程；`ShellAPI.onNavigate` 只导航，不把焦点移到请求面板上（5.7.8）。
   - **`deeplink` 模块**（5.7.2）：
     - 按构建种类注册 URL scheme（5.7.9，交接事项里 T074 那一行）；
     - `open-url` 只认上面那一种路由，两个 ID 都校验格式，否则忽略；
     - App 还没启动完就收到的 URL 也要处理。
6. **用例**（都已登记）：
   - **IT-notifier-click**（crate 内）：
     - 模拟点击后，control 连接收到 `ui/navigate`；
     - 没有 UI 时记录下要交的 URL，格式对；
     - 系统单独拉起的宿主认法的单元测试。
   - **E2E-notification-navigate**：record 模式下模拟点击，UI 切到对应的会话，焦点不进请求面板。
   - **`deeplink` 的单元测试**：合法和不合法的 URL。
   - **MAN-notification-product**：最后由编排者约用户，在自用构建上复核：
     - 真实的授权框、通知、名称和图标；
     - 点击跳转；
     - 没有 Finder 对话框；
     - 注销时宿主走有序退出（交接事项里 T150 那一行）。
     - 证据格式按 `docs/manual/`。

**不做**：
- 通知上的操作按钮、在通知里直接回复（5.7.8）；
- 设置页的通知开关和授权提示（T147）；
- 其他深链接路由。

## 验收标准

- [ ] IT-notifier-click、`deeplink` 的单元测试通过；E2E-notification-navigate 通过，并在 runner 上连跑 10 次（由 `main` 触发 e2e-repeat）。
- [ ] 宿主的约束都守住：
  - `unsafe` 只在 cupper-sys；
  - 时间只经 `Clock`；
  - 子进程只经 `cupper-sys::spawn`；
  - 协议只做附加。
- [ ] ShellAPI 只做附加，`frozen.ts` 的检查通过。
- [ ] 不在本机碰系统状态：
  - 不往 `/Applications` 装东西；
  - 不改 LaunchServices 的登记；
  - 不碰通知服务和授权；
  - 本机的用例和 e2e 一律用 record。
- [ ] MAN-notification-product 由用户复核，证据入库。截图不入库，日志过敏感词清单。
- [ ] PR 上 GitHub CI 的 8 个检查都通过；nightly 的 `selfuse`、`release-config` 在分支上通过（由 `main` 触发）；汇报里写上链接（守则第 9 条）。

## 规模

- 开工前先按模块估一遍：sys 的代理、宿主的点击处理、系统单独拉起的宿主、测试入口、桌面导航、`deeplink`、用例。估算超过停止线就先报告，不要动手。
- 停止线：650；硬停止线：780。
- 每完成一个模块就跑一次 `cargo xtask size`。
- 不要靠压缩代码或注释去守线。

## 参考资料

- 设计：5.2.9（通知的点击、六条规则、申请时机）、5.2.13（启动顺序第 1 步）、5.3（`ui/navigate`）、5.7.2（`hostLauncher`、`deeplink`）、5.7.8（其他入口只导航）、5.7.9（构建种类的 URL scheme）。
- 卷宗：T101（`results.md` 的交 URL、按路径找 App）、T150（第 0 步、主循环、登记）、T250（notifier、record 后端）、T074（URL scheme 尚未注册）。
- 交接事项：`docs/tasks/handoff-notes.md` 里接收任务为 T251、T250、T150 的各行。

## 注意

- 用户在场的步骤只有最后的 MAN-notification-product，由 `main` 约用户。
- 不在本机跑会抢焦点的用例（守则第 10 条）。不运行真实的 Claude 或 Codex CLI。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

继续用 `.claude/worktrees/T150`，编排者已切到新分支 `feat/T251-notifications-click`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
