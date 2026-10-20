# T085 工作笔记

- 执行者：子 agent（工程师；T082 的执行者接着做）

## 开工前核对现状

- `NewSessionPage` 只把输入框放在视口高度 42% 处。
- 输入框（T086）按路由的 `projectId`、没有就取第一个项目；`ready` 规则是有项目才能发送。
- `useSend` 里 `session/create` 写死 `agent: "claude"`、`workspace: {kind: "local"}`。
- **agent 的可用状态**：`agents/capabilities` 的结果只有 `cached?`（缓存的能力描述），没有可用状态，也没有原因码。可用状态和原因在索引视图的 `agents`（`AgentStatus`）里：
  - `state` 为 `unavailable`、`crashed` 等；
  - `cli` 缺失，表示没有找到 CLI；
  - `auth.state` 为 `logged_out`，表示没有登录；
  - `adapter.runtime` 为 `failed`，表示运行时安装失败。
  
  所以 agent 选择改用索引视图的 `agents`，它随 `agents.updated` 实时更新。这与任务书写的"取 `agents/capabilities`"不同，汇报里说明。
- 侧边栏（T081）的项目列表有"未信任"标记和"添加项目"的特权调用；下拉菜单用 T128 的 `Menu`。

## 开工前的规模估计

| 模块 | 内容 | 草估 |
|---|---|---|
| 大标题与项目选择器 | "你想在 {项目} 中构建什么？"，项目名是下拉菜单；列出项目和未信任标记，末尾是"添加项目"；没有项目时写"我们要构建什么？" | 70 |
| 动作条 | agent 下拉（只列 Claude，不可用时按原因码显示原因），工作区"本地" | 50 |
| agent 的可用状态 | 从索引视图的 `AgentStatus` 得出原因码 | 20 |
| 接线 | `Composer` 收所选的 agent 和工作区，`useSend` 带进 `session/create` | 15 |
| 文案 | | 10 |
| **实现小计** | | **165**，乘 1.1 约 180 |
| 组件测试 | 选项目改路由；`session/create` 带上所选的 agent 和工作区；没有项目时不能发送；"添加项目"走特权路径 | 80 |
| UI-new-session | 有项目、没有项目、项目选择器展开、agent 下拉展开，亮暗两种主题 | 50 |
| **合计** | | 约 **310** |

在约 380 行的线内。组件测试挂在 UI-new-session 下（T085 段只有这一个编号），汇报里说明。
