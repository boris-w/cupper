# T254 系统通知 IV：导航与深链接（桌面）

- 派发日期：2026-11-10
- 执行者：子 agent（宿主与桌面工程师；T150、T250、T251 的执行者）
- 分支：`feat/T254-notifications-navigate`
- 依赖：T251（已合并）。T148、T155、T170 依赖本任务

## 范围

**做**（计划 T254 一行，v1.3.92 由 T251 拆出；设计 5.7.2 的 `deeplink`、5.7.8、5.7.9、5.2.9；PRD AP-3）：

1. **转发 `ui/navigate`**：
   - 桌面主进程收到 control 连接上的 `ui/navigate {session_id, request_id?}`，转给渲染进程。
   - `ShellAPI.onNavigate` 只导航，不把焦点移到请求面板上（5.7.8）。
2. **`deeplink` 模块**（5.7.2）：
   - 按构建种类注册 URL scheme（5.7.9）。T251 记下 e2e 和开发构建还要补注册（交接事项里 T251、T074 那几行）。
   - `open-url` 只认 `<scheme>://session/<session_id>[?request=<request_id>]`：
     - 两个 ID 按协议格式校验（前缀加 1–64 个 `[A-Za-z0-9_-]`）；
     - scheme 必须是本构建的；
     - 其他一律忽略。
   - App 还没启动完就收到的 URL 也要处理（`will-finish-launching` 之前就登记 `open-url`，窗口就绪后再导航）。
   - 只导航，不聚焦请求面板。
3. **用例**（都已登记）：
   - **E2E-notification-navigate**：
     - record 模式下，经 harness 连接用 `test/notification_click` 点一条已记录的通知，`notification_id` 取索引流里 `notification.intent` 的 `intent_id`；
     - UI 切到对应的会话，焦点不进请求面板。
   - **`deeplink` 的单元测试**：合法和不合法的 URL，包括别的构建的 scheme、ID 格式不对、多余的路径或参数。
   - **MAN-notification-product**：最后由编排者约用户，在自用构建上复核下面几项。证据格式按 `docs/manual/`。
     - 真实的授权框，以及通知上的名称和图标（拿铁）；
     - 点击后跳到会话：UI 在前台、不在前台、已退出三种情况；
     - 没有 Finder 对话框，也没有再拉起宿主；
     - 激活的时机：`notifier_settle` 300 ms 够不够；
     - 注销时宿主走有序退出（交接事项里 T150 那一行）。

**不做**：
- 其他深链接路由；
- 通知上的操作按钮、在通知里直接回复（5.7.8）。

## 验收标准

- [ ] E2E-notification-navigate 通过，并在 runner 上连跑 10 次（由 `main` 触发 e2e-repeat，记得同时给 `spec` 和 `test`）。
- [ ] `deeplink` 的单元测试通过。
- [ ] ShellAPI 如果要改，只做附加，`frozen.ts` 的检查通过。
- [ ] 主进程的安全约束都守住：
  - 深链接只做导航，参数只接受符合格式的 ID；
  - 不把焦点移到请求面板（5.7.8 的"其他入口"）。
- [ ] MAN-notification-product 由用户复核，证据入库。截图不入库，日志过敏感词清单。
- [ ] PR 上 GitHub CI 的 8 个检查都通过；nightly 的 `selfuse`、`release-config` 在分支上通过（由 `main` 触发）；汇报里写上链接（守则第 9 条）。

## 规模

- 拆分时估约 230。开工前先按模块再估一遍。
- 停止线：300；硬停止线：360。每完成一个模块就跑一次 `cargo xtask size`。
- 不要靠压缩代码或注释去守线。

## 参考资料

- 设计：5.2.9（通知的点击、交 URL）、5.3（`ui/navigate`）、5.7.2（`deeplink`、`hostConnection`、`controlClient`）、5.7.8（其他入口只导航）、5.7.9（构建种类的 URL scheme）、5.10.1（主 App 的 Info.plist 里登记 scheme）。
- 卷宗：T251（宿主侧、`test/notification_click`）、T250（record 后端）、T150（第 0 步）、T101（`results.md`：交 URL 时 App 在 `applicationDidFinishLaunching` 之前就拿到 URL）。
- 交接事项：`docs/tasks/handoff-notes.md` 里接收任务为 T254、T251、T150、T074 的各行。

## 注意

- 用户在场的步骤只有最后的 MAN-notification-product，由 `main` 约用户。
- 不在本机碰系统状态：不往 `/Applications` 装东西，不改 LaunchServices 的登记，不碰通知服务和授权；本机的用例和 e2e 一律用 record。
- 本机不跑会抢焦点的用例（守则第 10 条）。不运行真实的 Claude 或 Codex CLI。
- 守则第 1 条：不执行 merge、rebase。要整理提交历史，先告诉 `main`。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

继续用 `.claude/worktrees/T150`，编排者已切到新分支 `feat/T254-notifications-navigate`，从 main 开出（含 T251 和本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
