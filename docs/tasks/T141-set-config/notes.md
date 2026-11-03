# T141 notes

## 开工前的估算（2026-11-03，报给编排者，等回复后再写代码）

已有的部分：
- 档位规则（`policy::tier_change`、`ceiling`，UNIT-tier-rules）；
- SEC-9 的 `policy::set_config`（网关已调用）和 `policy/describe` 对 `session/set_config` 的描述；
- `TierGuard` 的 `Set {by, tier}` 与 `SetBy`（写 `mode_set_by`、`full_access_confirmed_at`）；
- 驱动的 `DriverCommand::SetConfig`（没有回复，结果以 `ConfigChanged` 到达，被拒绝时什么也不来）；
- 主进程的特权操作 `set_config`（原生确认框加 `policy/describe`）；
- 输入框底栏的扩展插槽（`composer/extensions/*.tsx`）和 `Menu` 组件。

### 宿主（约 340）

| 模块 | 行数 |
|---|---|
| `AgentPort::set_config` 请求（带回复），runtime 实现：`Wire::Configured` 带上回复端；测试替身的 Agent 实现 | 60 |
| 会话 actor 的 `session/set_config`：按能力核对选项和取值；权限模式走档位规则；已连接时经驱动请求，等回复；未连接时只记下，下次 attach 时应用；成功后写 `session.configured`、更新 `config_json`（resume 后重新应用的是用户选的值）、交给 `TierGuard` 记下设定者；账本完成与回复；agent 拒绝时返回 `agent_error` | 130 |
| `Rows` 写 `config_json`；路由加 `SessionSetConfig`；dispatch 测试表去掉一项 | 10 |
| IT-set-config-tier：5.8.2 的表逐格（收紧或同档、放宽到 elevated、放宽到 full_access × 各列）加 `mode_set_by`、`full_access_confirmed_at`；模型的修改与回复，resume 后重新应用；agent 拒绝；未连接时的修改 | 140 |

### 界面（约 370）

| 模块 | 行数 |
|---|---|
| `composer/config.ts`：从会话视图读能力和当前配置；发 `session/set_config`，失败时 toast | 40 |
| `extensions/permission.tsx`：图标加短名，下拉列出各模式及档位；完全访问走 `requestPrivileged`；elevated 给提示 | 95 |
| `extensions/model.tsx`："{模型} {强度} ⌄"，下拉分两组 | 60 |
| "完全访问权限已开启"的提示（输入框上方） | 20 |
| 图标、i18n 文案 | 25 |
| 组件测试（vitest）：菜单内容、普通档直接发命令、完全访问走特权操作、提示出现、模型按钮的文字与选择 | 130 |

### 用例

| 模块 | 行数 |
|---|---|
| E2E-full-access-native（nightly）：真实 App、假 agent；底栏选完全访问 → 原生确认框（替身）确认 → 宿主写 `full_access` → 提示出现；取消时不变 | 50 |

合计约 760。按 T120、T121、T126 的经验再加 20–30%，约 900–990，在停止线 1,000 以内，暂不拆分。做到一半如果推算会超过 1,000，按"宿主 / 界面"拆：宿主（`set_config` 与 IT-set-config-tier，约 400）先合，界面与 E2E 另开任务。

### 需要编排者定的点

1. 新建会话页（没有 `session_id`）上不显示这两个选择器，交给 T144（它负责"第一条消息前显示缓存的能力"）。
2. 截图：mock 宿主不发 `session.capabilities`，现有截图里底栏不会出现新按钮，基线不变；本任务不加新的截图用例，界面用组件测试覆盖。
3. T143 留下的 `change_model` 入口：错误框里加"换模型"按钮、打开模型菜单，约 20 行；规模有余量就做，否则列进汇报。

## 实现记录
