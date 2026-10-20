# T137 工作笔记

## 开工前的核对

- `handoff-notes.md` 里第一列含 T137 的行：T080 的状态层接口（`useStreams().session(id)`、`useEntry`）。任务书的交接事项另有 T081（`usePrivileged`、截图页的索引流夹具）和 T134（`confirm_config` 的确认框内容由主进程从会话流取）。
- `tests/ids.toml` 的 T137 段已有 UI-session-banners、UNIT-ui-session-banners。
- 数据：会话所在项目的信任状态在索引流（`sessions[id].project_id` → `projects[...].trust_state`）；`confirmation_required`（`config_digest`、`reason`）在会话流 `configured` 里，`SessionView` 改它时 touch 的键是 `session`。
- 文案依据：5.2.6"未信任或 `stale` 时……界面显示'未信任：项目配置未加载'"（Claude 只加载用户级配置，Codex 不能使用）；5.8.4 的横幅是"命令行工具申请权限"。确认原因的说法对照 `privileged` 命名空间里确认框的文案。
- 会话页现在没有拿到 `sessionId`（`Page` 只用它做 `key`），要改成传进去。

## 规模估计（开工前）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 文案 | 两种未信任说明（`untrusted`、`stale`）、"信任此项目"、三种确认原因、"确认"；提权横幅改通用文案 | 12 | — |
| 警告框 | "未信任"（`trust_project`）、"配置待确认"（`confirm_config`）按 props 画；接索引流与会话流的外层；会话页传入 `sessionId` | 75 | — |
| 两处修正 | 提权横幅不再显示自述名字；"未信任"标签右对齐（悬停按钮改为浮在行尾） | 8 | — |
| 截图页夹具 | 把 `index` 推广成按流名给 op 的 `streams` | 8 | — |
| UI-session-banners | 两种警告框、两种主题 | — | 40 |
| UNIT-ui-session-banners | 出现与消失的条件、两个按钮的 `requestPrivileged`、提权横幅不显示自述名字 | — | 70 |
| 合计 | | 103 | 110 |

合计约 **215 行**，上限约 300。

## 实现记录

- **警告框**（`packages/ui-core/src/session/SessionBanners.tsx`）：
  - `UntrustedBanner`：项目不是 `trusted` 时出现；`untrusted` 与 `stale` 各一句说明（5.2.6：项目配置没有加载，Claude 只用用户级配置，Codex 会话不能运行；`stale` 是配置变了、需要重新信任）；"信任此项目"走 `trust_project`。
  - `ConfirmBanner`：`confirmation_required` 存在时出现，按 `reason` 取说明（`full_access_unconfirmed`、`tier_above_setter`，其余原因写"需要确认（原因码）"）；"确认"走 `confirm_config`。没有本地状态：确认之后警告框留着，直到会话流里的 `configured` 不再带 `confirmation_required`。
  - `SessionBanners` 从索引流取会话的项目（订阅整个索引视图），从 `useStreams().session(id)` 取会话视图（订阅 `session` 键）。会话页现在接收 `sessionId`，警告框放在会话列顶部，左对齐，组内间距 4。
  - 按钮用次级按钮底（`--color-btn-secondary`）；反馈走 `usePrivileged`。
- **两处修正**：
  - 提权横幅只写通用文案"命令行工具申请权限"（5.8.4），不显示申请方自述的 `client_name`；核实过的信息只在原生确认框里出现。
  - 项目行的"未信任"放到行尾，和会话行的标签右对齐；悬停按钮改为浮在行尾（`absolute`），悬停、获得焦点或菜单打开时盖住并隐去标签。
- **截图页夹具**：`index` 推广成 `streams`（按流名给 snapshot 的 op，例如 `index`、`session/s_7`），T081 的用例改用 `streams: { index }`。
- **基线**：新增 `session-banners-light.png`、`session-banners-dark.png`；`sidebar-light.png`、`sidebar-dark.png`、`sidebar-narrow.png`、`banners.png` 因两处修正而变（横幅文案、"未信任"的位置），重新生成后逐张看过；UI-shell 不变。
- **审阅截图**：`screenshots/`，截图页换回系统字体后截取，1280 × 820：两种主题的会话页警告框，以及修正后的侧边栏。按 F2，没有查看本机 Codex App 包。

### 行数（不含 notes、lock、截图基线）

| 模块 | 估计 | 实际 |
|---|---|---|
| 文案 | 12 | 9 |
| 警告框（含会话页接线） | 75 | 97 |
| 两处修正 | 8 | 8 |
| 截图页夹具 | 8 | 6 |
| UI-session-banners | 40 | 23 |
| UNIT-ui-session-banners | 70 | 71 |
| 合计 | 215 | **214** |
