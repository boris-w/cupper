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

- 宿主部分完成后 `cargo xtask size` 为 +470（估算 340，偏高 38%）：actor +193（估 130）、IT-set-config-tier 169（估 140）、夹具 +49、runtime +65。照这个偏差推算，界面约 480，加上 E2E 约 60，合计约 1,010，超过 1,000。按事先定好的规则停下来报告；编排者裁定拆分（2026-11-03）：T141 只含宿主，界面、审阅截图和 E2E-full-access-native 移到 T231。

## 取舍（宿主）

- **`AgentPort::set_config` 带回复**：`DriverCommand::SetConfig` 照旧没有回复，留给 TierGuard 改回档位时用。runtime 收到 `session/set_config_option` 的结果后，先给会话发 `ConfigChanged`，再把同一份选项交给等着的回复；agent 拒绝时回复它的错误。适配器不在时回复"the adapter went away"。
- **回复晚于流**：会话收到 agent 的回复后，先按返回的选项写 `session.configured`，并让 TierGuard 核对模式，然后在同一次提交里回复命令。随后到达的 `ConfigChanged` 内容相同，什么也不写。所以客户端拿到回复时，流里已经是新配置，`mode_set_by` 也已经写进行里。
- **档位**：取值的档位来自能力里这个值的 `risk`，从会话当前的档位出发，按 `policy::tier_change` 判定。放宽之前先告诉 TierGuard 设定者是谁（`Set {by, tier}`），等 agent 报告新模式时再记下 `mode_set_by`；进入完全访问时同时记下 `full_access_confirmed_at`。完全访问写 `privileged.executed` 审计（`policy/describe` 的描述）。
- **核对**：没有这个选项返回 `unsupported`（`data.capability` 为选项名）；没有这个取值返回 INVALID_PARAMS；agent 拒绝返回 `agent_error`，带 `agent_code` 和脱敏后的 `agent_message`。
- **未连接时**：只记进会话的配置（`parts.config`）并写 `session.configured`，回复里是记下后的配置；下次 attach 照常把这些值逐项应用。已连接时成功修改的值也记进 `parts.config`，所以之后同一个 actor 里再 attach（例如适配器重启后 resume），应用的是用户选的值，而不是创建时的值；`config_json` 随 `session.configured` 更新，重启后加载的也是它。
- **已知缺口**：attach 已经发出、还没返回时，会话算未连接，这期间的修改要等下一次 attach 才会应用。
