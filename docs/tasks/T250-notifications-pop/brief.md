# T250 系统通知 II：弹出通知

- 派发日期：2026-11-10
- 执行者：子 agent（宿主工程师，cupper-sys、宿主；T150 的执行者）
- 分支：`feat/T250-notifications-pop`
- 依赖：T150（已合并）。T251、T252 依赖本任务

## 范围

**做**（计划 T250 一行；设计 5.2.9（关注度与通知、六条规则的第 4、5 条）、5.3 的 `host.updated`、5.7.8、5.10.1；PRD AP-3）：

1. **App 图标**（前提已满足，2026-11-10 定稿，`docs/meetings/2026-11-10-app-icon/`）：
   - 由拿铁母版 `icon/cupper-latte.png` 生成 icns 入库，放在 `apps/desktop/build/` 下。生成步骤写进 `notes.md`，或者做成可重复运行的脚本，不要只留产物。
   - 四份 electron-builder 配置的 `mac.icon` 都指向它。
   - `cargo xtask host-bundle` 把同一个 icns 复制进 `CupperHost.app/Contents/Resources`，写 `CFBundleIconFile`。
   - UNIT-host-bundle-identity 补上图标的断言。
   - **macOS 15 上看一眼**：设计师只在 macOS 26.4 上看过，系统按新的图标形状重新裁边，大小和自带 App 一致，没有被套进灰色底板。在 GitHub 的 `macos-15` runner 上做同样的检查，可以用 `spike-run` workflow，由 `main` 触发。结果写进 `notes.md`。
2. **cupper-sys 的 UserNotifications**（`unsafe` 只在这里）：
   - 申请授权、查询授权状态；
   - 弹通知，不注册任何 category 和 action；
   - 清掉已送达的通知。
   - 挂在 T150 的主循环上（交接事项 T250 那一行）。
3. **宿主 `notifier`**：
   - **接入点**：`IndexActor` 处理通知意图的地方，或者读索引流里的 `notification.intent`（交接事项里 T150 的几行）。
   - **文案**：内置中英文，跟随系统语言，措辞参照 Codex（ui-spec 第 4.8 节）。
     - 标题用会话标题，没有时用内置文案。
     - 正文只用按原因选的内置文案：需要批准、需要回答、回合完成、回合失败。不放 agent 的原文（设计 5.2.9）。
   - **开关**：已有的三个设置键 `notifications.needs_action`、`.turn_completed`、`.error`，回合失败归 `.error`。
   - **`test_config.notifier: record`**：只记录、不弹，用例据此断言。test-harness 和 e2e 构建一律用 record，不碰系统的通知服务。
   - **授权**：
     - 状态写进 `host.updated.notification_auth`（附加，`not_determined`、`authorized`、`denied`）；被拒后不重复申请。
     - 什么时候申请，由你定并说明理由：启动时，或者第一次要弹通知时。
   - **有序退出时清掉已送达的通知**（设计 5.2.13 有序退出的那一步）。
4. **用例**（都已登记）：
   - **IT-notifier-record**（crate 内）：
     - 三种原因各弹一条，文案对；
     - 对应的开关关掉就不弹；
     - 前台的 `desktop_ui` 正在看这个会话时不弹；
     - 授权状态进 `host.updated`。
   - **UNIT-notifier-no-actions**：系统通知没有任何操作按钮。

**不做**：
- 点击、`ui/navigate`、激活外层 App、交 URL、系统单独拉起的宿主、`deeplink`、模拟点击的入口（T251）；
- 设置页的通知开关和授权被拒时的提示（T147）；
- 图标颜色的设置（T252）；
- 提示音、在通知里直接回复、操作按钮（5.7.8，有意与 Codex 不同）。

## 验收标准

- [ ] IT-notifier-record、UNIT-notifier-no-actions 通过，宿主的其余用例照常通过。
- [ ] 主 App 和宿主都用拿铁母版生成的图标；macOS 15 上的检查有结果。
- [ ] 宿主的约束都守住：
  - `unsafe` 只在 cupper-sys；
  - 时间只经 `Clock`；
  - 子进程只经 `cupper-sys::spawn`；
  - 索引流只有 `IndexActor` 一个写者；
  - 协议只做附加，schema-compat 0 个破坏性改动。
- [ ] 不在本机碰系统的通知服务和授权：本机的用例和 e2e 一律用 record。
- [ ] PR 上 GitHub CI 的 8 个检查都通过；nightly 的 `selfuse`、`release-config` 在分支上通过（由 `main` 触发）；汇报里写上链接（守则第 9 条）。

## 规模

- 开工前先按模块估一遍：图标、sys、notifier、协议、用例。估算超过停止线就先报告，不要动手。
- 停止线：650；硬停止线：780。
- 图标文件和生成脚本的产物不计入。
- 不要靠压缩代码或注释去守线。

## 参考资料

- 设计：5.2.9（通知意图、文案、开关、六条规则）、5.2.13（有序退出）、5.3（`host.updated`）、5.7.8（通知不放按钮）、5.10.1（宿主的名称和图标）。
- 卷宗：T150（主循环、登记、名称）、T101（`results.md`：通知中心在 helper 第一次连上通知服务时读取名称和图标）、T100（索引的通知意图）。
- 会议：`docs/meetings/2026-11-10-app-icon/`（图标定稿、母版和生成脚本）。
- ui-spec 第 4.8 节（Codex 的通知文案和不发的条件）。
- 交接事项：`docs/tasks/handoff-notes.md` 里接收任务为 T250、T150 的各行。

## 注意

- 名称和图标在 helper 第一次连上通知服务时被系统记下，之后不再更新。所以图标必须在本任务里一起进包，不能晚于 UserNotifications 的接入。
- 不在本机碰系统状态：不往 `/Applications` 装东西，不改 LaunchServices 的登记，不申请、不改通知授权。
- 不在本机跑会抢焦点的用例（守则第 10 条）。不运行真实的 Claude 或 Codex CLI。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

继续用 T150 的 worktree `.claude/worktrees/T150`（构建缓存还在），编排者已切到新分支 `feat/T250-notifications-pop`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
