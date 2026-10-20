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

## 做的过程中的取舍

- **新建会话页移出 `app/pages.tsx`**：页面放进 `newsession/NewSessionPage.tsx`，`pages.tsx` 的 `Page()` 只 import 它。`index.ts` 也从新模块导出。
- **`Menu` 加 `align`**：原来写死 `end`，agent 下拉在动作条左端会伸出到条外。现在 agent 下拉用 `start`，项目选择器在标题中间，用 `center`；默认仍是 `end`，旧调用方不变。
- **`useSend` 的第三个参数**：`create`（所选的 agent 和工作区），默认值是 `DEFAULT_NEW_SESSION`（`claude` + `local`）。会话页的输入框不传，行为不变。新建会话页把所选的值经 `Composer` 的 `create` 传进去。
- **agent 下拉只列 Claude**：选中值由页面自己的 state 持有，以后 M1b 加 Codex 只是多一个菜单项。不可用时菜单项后面跟原因（`cli_missing`、`not_logged_in`、`runtime_failed`，其余算 `unavailable`），文案走 `composer:agent_*`。M1a 不因 agent 不可用而禁止发送：发送后宿主会按语义码报错，走 `useSend` 已有的发送失败路径。`create` 用 `useMemo` 保持引用稳定，免得 `useSend` 的回调每次渲染都重建。
- **大标题的锚点**：输入框顶部仍在视口高度 42% 处。容器上边距减去 6rem，给标题（`h-16` 加 `mb-8`）让出位置。
- **组件测试的模拟宿主**：`stream/subscribe` 只对 `index` 流回放索引的 ops。否则新建的会话流也会收到索引的 ops，视图报错后反复重订，vitest 内存耗尽。
- **既有基线的变化**：默认页就是新建会话页，所以 banners、shell-collapsed、shell-dark、shell-light、shell-narrow-overlay、sidebar-narrow 六张基线随之更新。逐张看过，变化只在主区的标题和动作条。

## 实际规模

`git diff --numstat`，不含基线 PNG 和 docs：

| 文件 | 增 | 删 |
|---|---|---|
| `newsession/NewSessionPage.tsx` | 134 | 0 |
| `composer/send.ts` | 15 | 8 |
| i18n `composer.json` | 11 | 1 |
| `components/Menu.tsx` | 7 | 2 |
| `composer/Composer.tsx` | 5 | 3 |
| `index.ts`、`app/pages.tsx` | 3 | 12 |
| **实现小计** | **175** | 26 |
| `newsession/new-session.test.tsx`（组件测试） | 133 | 0 |
| `new-session.spec.ts`（UI-new-session） | 58 | 0 |
| **合计** | **366** | 26 |

实现与估计相当（175 对 180）。组件测试比估计多约 50 行，主要是模拟宿主和各种 Provider 的搭建。合计在约 380 行的线内。

## 评审截图

`screenshots/` 下是系统字体、1280×820 的截图：projects-light、projects-dark、no-project-light、project-picker-light、agent-picker-dark。
