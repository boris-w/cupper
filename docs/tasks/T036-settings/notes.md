# T036 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）
- 日期：2026-10-26

## 读到的现状（开工前）

- `settings` 表在 `0001` 和设计 5.4.2 里都已经有了（`key`、`value` JSON、`updated_at`），不用改表结构。
- 契约已有 `SettingsGet`、`SettingsUpdate`、`Settings`、`SettingsPatch`。网关的 `settings_update` 规则在 T033 已经执行：安全相关的键要 `privileged`。`policy/describe` 的 `security_setting` 也已经有了。
- `driver/runtime/launch.rs` 的 `production()` 里：`locator.locate(&resolve::CLAUDE, None)` 的 `None` 是还没接的 `agents.claude.cli_path`，`adapter_env(…, &[])` 的 `&[]` 是还没接的 `agents.env_passthrough`。这两个只在发现 agent 时（启动、`agents/refresh`）读，改了设置要重新发现一次，结果会经 `agents.updated` 进索引流。
- 桌面的 `actions.ts` 遇到 `settings` 回 `failed("unsupported")`；照 `setConfig` 补，需要新的确认框和 i18n 文案（英文、中文）。
- `AgentsUpdated.git` 由 `index/mod.rs` 填 `GitTools { version: None, lfs: None }`；版本检测在 `driver/probe/git.rs`。

## 和契约冲突的一点

**"变更后经索引流通知界面"在契约里没有对应的事件**：
- 索引流的事件只有 `agents.updated`、`host.updated`、`clients.updated` 等，索引视图里也没有设置；设计里没写设置变更后怎样通知。
- 要满足这一条，得给契约加一种事件（例如 `settings.updated {settings}`，索引视图跟着加一项），或者把设置并进已有的事件。两种都是改契约。T016 正在冻结契约，按任务书应停下汇报。
- 不改契约能做到的只有：`cli_path`、`env_passthrough` 改了以后重新发现 agent，经 `agents.updated` 体现；其余的键（通知开关、`worktrees.root` 等）没有推送，其他窗口要自己重新 `settings/get`。

## 开工前的规模估计

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 设置模块 | 默认值、启动时从表读、给各模块的读取接口；`settings/update` 的校验（`env_passthrough` 的变量名、路径是绝对路径、`idle_exit_minutes` 的范围）、按键写行、与账本记录同一事务 | 150 | — |
| 登记与接入 | `SettingsPort` 进 `Routes`、监护者构造；UNIT-unsupported-set 删行 | 30 | 3 |
| agent 发现读设置 | `cli_path` 进 `locate`，`env_passthrough` 进 `adapter_env`；改了就重新发现 | 30 | — |
| 变更通知 | 契约加事件、索引视图、索引 actor 写入（需要先定契约） | 60 | 20 |
| 桌面特权操作 `settings` | `policy/describe` → 原生确认框 → `settings/update`；确认框与 i18n 文案 | 50 | 40 |
| `AgentsUpdated.git` | 启动时的 git 与 git-lfs 版本接进 `GitTools` | 30 | 15 |
| UNIT-settings | 默认值、读取、校验与持久化、缺 `privileged` 被拒、变更后的通知 | — | 110 |
| **合计** | | **约 350** | **约 190** |

- 直接相加约 **540**，按最近的偏差乘 1.2 约 **650**，远超停止线 400。
- 不算通知和 `AgentsUpdated.git`，也约 **430**（乘 1.2 约 520）。

## 拆分建议

| 拆分 | 内容 | 估计（直接相加） |
|---|---|---|
| **T036a 宿主的设置** | 设置模块与读取接口、`settings/get`、`settings/update`、登记与 UNIT-unsupported-set、`cli_path` 与 `env_passthrough` 接进 agent 发现；UNIT-settings（不含通知） | 约 320 |
| **T036b 桌面的设置与版本** | 桌面特权操作 `settings`（确认框、文案、用例）；`AgentsUpdated.git` | 约 135 |
| **通知** | 等契约决定：T016 加事件后单列，约 80；或者裁定 M1a 不推送 | — |
