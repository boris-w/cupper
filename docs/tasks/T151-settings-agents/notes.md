# T151 notes

## 开工前的估算（2026-11-09）

停止线 500，硬停止线 600。

### 现状

- **界面**：
  - 路由只有 `session` 和 `new` 两页（`app/router.tsx`、`app/pages.tsx`），没有设置页，也没有入口。
  - `components/` 里有 `Switch`、`Menu`、`Alert`，可以复用。
- **宿主**：
  - `AgentsUpdated.git` 一直是空的（`index/mod.rs` 里写死为 `None`）。
  - T094 的 `driver::probe::git::detect` 已经能取到 git 和 git-lfs 的版本，只是没有接上。
  - 索引流里没有设置，`IndexView` 也没有 `settings` 字段。
- **数据来源**：
  - agent 状态：`IndexView.agents`；连接：`IndexView.clients`；设置：新增的 `IndexView.settings`。
  - 已归档会话：`session/list_archived` 和 `session/restore`（T240、T241）。
  - 重启：`agents/restart`。

### 按模块

| 模块 | 内容 | 行 |
|---|---|---|
| 协议 | 见下文 | 20 |
| 宿主：`settings.updated` | 启动对账时写一次；`settings/update` 提交后，经索引的信箱再写一次 | 30 |
| 宿主：git 版本 | 见下文 | 25 |
| 宿主用例 | 索引对账和更新后各有一条 `settings.updated`；`agents.updated.git` 带上版本（假的 git） | 45 |
| 路由与入口 | `Route` 加 `settings{section}`；侧边栏底部的"设置"；Shell 在设置页时让出侧边栏，设置页占满主区 | 35 |
| 设置页框架 | 见下文 | 80 |
| agent 状态 | 见下文 | 110 |
| 连接与设备 | 见下文 | 45 |
| 已归档会话 | 见下文 | 75 |
| i18n 文案 | `settings` 命名空间 | 45 |
| 界面用例 | 见下文 | 130 |
| 合计 | | 约 640 |

**协议**：
- 新增事件 `settings.updated {settings}`，复用 `Settings` 结构。
- `IndexView.settings` 为 `Option`，带 `skip_serializing_if`，后写的覆盖先写的。
- 跑 `pnpm gen`。

**宿主：git 版本**：
- supervisor 启动时，以及 `agents/refresh` 时，调用 `probe::git::detect`。
- 结果经索引的信箱写进 `AgentsUpdated.git`。

**设置页框架**：
- 左侧导航沿用侧边栏的行样式，顶部是"返回应用"。
- 内容区最大宽 768，水平居中，分节间距 40。
- 提供 `Section`、`Row` 组件：左边是标签加说明，右边是控件。开关按 32 × 19。
- 分页做成一张表，T147 往表里加一项就能加一页。

**agent 状态**：
- 每个 agent 一张卡，显示：
  - CLI 路径、版本、已验证范围（compat）、安装方式；
  - 适配器版本、运行时状态和失败原因；
  - 登录状态；
  - 配置目录的校验结果；
  - 内存占用。
- 重启按钮调用 `agents/restart`，显示进行中的状态。
- 没装 Codex CLI 时，显示 `unavailable` 的文案。
- 错误条目恢复指引里的 `upgrade_cli`、`install_cli`、`restart_agent`，按钮跳到这一页的对应 agent。

**连接与设备**：
- 本机的客户端列表：类型、角色、名称、版本、连接时间、认证方式，并附弱认证的说明。
- M3 的设备还没有，显示空状态。

**已归档会话**：
- `session/list_archived` 按键集分页，"加载更多"。
- 恢复按钮调用 `session/restore`：进行中显示转圈，worktree 会话大约要等 1 秒；结果为 `read_only` 时注明。

**界面用例**：
- UI-settings-agents 的 vitest，约 90 行，覆盖：
  - 框架与导航；
  - agent 卡的几种状态（可用、`unavailable`、运行时失败）；
  - 重启发出的命令；
  - 归档的分页、恢复中、`read_only` 的注明；
  - 连接列表与设备的空状态。
- 截图规格：亮色、暗色两套，约 40 行。基线只由 `ui-baseline` 生成。

### 要拆

约 640，超过硬停止线 600。

**方案 A（推荐，按功能拆）**：

| 部分 | 内容 | 规模 |
|---|---|---|
| T151 | 宿主全部，加设置页框架和 agent 状态，以及它们的用例、截图 | 约 435 |
| 新任务 | 连接与设备，加已归档会话，以及它们的用例、截图。依赖 T151 的框架 | 约 205 |

T147 依赖 T151 的框架，不受影响。

**方案 B（按宿主、界面拆）**：

| 部分 | 内容 | 规模 |
|---|---|---|
| 宿主 | 宿主部分和它的用例 | 约 120 |
| 界面 | 界面全部 | 约 520，仍然超过停止线 500 |

### 要确认的问题

1. 取方案 A 还是 B。
2. **"返回应用"回到哪里**：提议回到进设置页之前的那一页（会话页或新会话页），由路由记住来处。
3. **错误条目里 `upgrade_cli`、`install_cli`、`restart_agent` 的按钮**：提议直接跳到设置页 agent 状态里的对应 agent。改的是 `rows/plan-error` 的错误条目，属于 T143 的组件，改动很小。

## 宿主部分做完后的规模（2026-11-09）

已在本机提交两个提交，714110b8（协议）和 ce552e88（宿主），合计 +228 −47，不含生成物。

**比估的 120 多出约 110**：
- 协议的样例：`events.json` 26 行，`event.rs` 的往返样例 7 行，索引视图的单元用例和属性用例 20 行。前两处是现有用例要求每种事件都有样例。
- 宿主用例 50 行。
- `agents.updated` 的合并改成一个函数，git 版本和 agent 状态共用。
- 设置改由 watch 通道持有，索引跟着它写 `settings.updated`；这样多个更新的先后不会乱。

**界面部分的推算**：

| 模块 | 行 |
|---|---|
| 路由、入口、菜单的"设置"、Shell 让出侧边栏 | 约 45 |
| 设置页框架 | 约 70 |
| agent 状态 | 约 100 |
| 错误条目跳转 | 约 15 |
| i18n | 约 40 |
| UI-settings-agents 和错误条目用例 | 约 100 |
| 截图规格 | 约 35 |
| 合计 | 约 390 |

加上宿主部分，合计约 620，超过硬停止线 560。
