# T081 工作笔记

## 开工前的核对

- `handoff-notes.md` 里第一列含 T081 的行：T084（插槽名、默认头部由本任务替换、token 写法、用户状态只经 `style`）、T118（截图用例的加法、字体子集检查）、T128（基础组件）、T134（`requestPrivileged` 的 kind 与六种返回值）。另有"所有写中文文案的任务"一行：子集外的符号（如 `⚠`）会让 `fonts.test.ts` 失败。
- `tests/ids.toml` 的 T081 段有 UI-sidebar、UI-banners；交互的 Vitest 用例需要新编号（例如 UNIT-ui-sidebar），开工时补进 T081 段。
- 数据（都在索引流的 `IndexView` 里）：`sessions`（`SessionSummary`：`status`、`pending_kind`、`unread`、`error_code`、`agent`、`title`、`last_activity_at`）、`projects`（`ProjectUpserted`：`name`、`trust_state`、`position`）、`elevations`（`ElevationRequested`：`client_name`、`scopes`）、`host`（`HostUpdated`：`pending_upgrade` 等）、`clients`。"配置待确认"不在索引流里，在会话流的 `configured.confirmation_required`（`config_digest`、`reason`）。
- T080（只读看了它分支上的 notes）：流 store 包 sink，按动画帧 `versions.flush()`；组件经 `useEntry(view, key)` 订阅某个键的版本、自己从视图读数据；索引流常驻订阅，会话流"当前 + 3"。键是 `sessions`、`projects`、`elevations`（集合增删）、`session:<id>`、`project:<id>`、`elevation:<id>`、`host`、`clients`。
- 截图：mock 宿主对 `stream/subscribe` 回固定样例，不带索引数据。截图页要加一个夹具项：对 `index` 的订阅回 `mode: snapshot`，随后发一页 `stream/snapshot`（项目、会话、提权申请等 op）。这一步要等 T080 把侧边栏接到 store 上才有意义。

## 规模估计（开工前）

按格式化后的写法估，不含 notes、lock、截图基线。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 图标与 token | 文件夹、新会话、加号、错误图标、转圈；`animate-spin`（1.25 s 一圈，减少动态效果时停） | 40 | — |
| 文案 | 侧边栏、横幅、toast、特权操作反馈的键 | 45 | — |
| 侧边栏头部 | 产品名；状态点（绿、灰、红，连接中为蓝色转圈），tooltip 写待升级等细节 | 35 | — |
| 侧边栏列表 | "新会话"一行；项目区标题与"添加项目"（`add_project`）；项目分组（未信任标记、悬停的"+"与菜单、展开与收起）；会话行（30 高、转圈、蓝点、"等待批准""需要用户输入"、错误图标、agent 名的 tooltip、选中） | 180 | — |
| 侧边栏底部 | 提权横幅（"查看"走 `elevation`，多条时显示数量） | 40 | — |
| 会话页警告框 | "未信任"（信任走 `trust_project`）、"配置待确认"（确认走 `confirm_config`） | 45 | — |
| Toaster | Provider、顶部居中、最多 3 条、自动消失、关闭按钮、按 id 替换 | 70 | — |
| 重连状态 | 按连接状态显示和收起 toast | 25 | — |
| 特权操作的反馈 | `usePrivileged`：六种返回值 → toast（`cooldown` 显示秒数，`busy`、`user_activation_required`、`failed` 各有说明，`confirmed`、`cancelled` 不打扰） | 45 | — |
| 接到 T080 的 store | 索引流 → 侧边栏与提权横幅；会话流 → 会话页警告框 | 60 | — |
| 截图页的索引流夹具 | mock 宿主按夹具回快照 | 20 | — |
| UI-sidebar | 会话行各状态与两种标签、两种主题、窄窗口浮层 | — | 45 |
| UI-banners | 提权横幅、两种警告框、重连、toast 的位置与 3 条上限 | — | 55 |
| 交互用例 | 三类 `requestPrivileged`、反馈映射、toast 上限、展开与收起、点会话导航 | — | 140 |
| 合计 | | 605 | 240 |

合计约 **845 行**。以往的实际偏差在 −6% 到 +15% 之间，推算约 850–970，贴近或超过约 900 的停止线。按任务书，开工前停下汇报。

## 拆分建议

会话页的两个警告框和侧边栏关系最弱：数据在会话流里（`configured.confirmation_required`），位置在 T082 负责的会话页上方。建议把它拆出去：

| 部分 | 内容 | 估计 |
|---|---|---|
| T081（关键路径） | 侧边栏全部（头部、列表、会话行、项目分组、添加项目、提权横幅）、Toaster、重连状态、特权操作的反馈、接到 T080 的索引流、截图页的索引流夹具；UI-sidebar，UI-banners 的提权横幅、重连与 toast 部分；交互用例 | 约 745（推算 750–860） |
| 新编号（会话页警告框，依赖 T080、T081，可与 T082 并行或并进 T082） | "未信任"（`trust_project`）、"配置待确认"（`confirm_config`），接会话流；UI-banners 的警告框部分及交互用例 | 约 100 |

- 两份都只依赖 T080 和本任务的 Toaster 与 `usePrivileged`；T090 的 E2E 需要两者都在，但警告框那份很小，可以在 T081 合并后马上派。
- 不拆的话，上限需要提到约 1,000。

## 不依赖 T080 的部分（裁定后先做）

组件全部按 props 写；Toaster、`usePrivileged` 只依赖 ShellAPI；交互用例用 jsdom 和 browser-mock 的 `setPrivilegedResult`。等 T080 合并、main 合进本分支后，再接 store、加截图页夹具和 UI-sidebar、UI-banners。

## 裁定（计划 v1.3.29）

采用拆分：会话页的"未信任""配置待确认"警告框归新的 T137。本任务：侧边栏、提权横幅、添加项目、重连状态、Toaster、`usePrivileged` 的反馈；验收 UI-sidebar、UI-banners（不含会话页警告框）、UNIT-ui-sidebar。上限约 900，硬停止线 1,000。T080 合并后由编排者把 main 合进本分支（f7d906b）。

## 实现记录

### 反馈（`feedback/`）

- **Toaster**：`ToasterProvider` 在 `App` 里包住 `HostProvider`；顶部居中，距顶 `--layout-toast-top`，最宽 `--layout-composer-adjacent-max-w`，最多 3 条（新的挤掉最旧的），间距 8；同 id 的 toast 替换旧的；普通 toast 4 秒后消失，`sticky` 的留到关闭或被替换；每条有关闭按钮。设计里的"向右滑动关闭"没有做。
- **`usePrivileged`**：经 `ShellAPI.requestPrivileged` 执行，按返回值给反馈：`confirmed`、`cancelled` 不打扰（前者的结果经流显示，后者是用户自己取消的）；`busy`、`cooldown`（向上取整的秒数）、`user_activation_required`、`failed`（主进程给的语义码 `error.code`）各一条 toast，id 是 `privileged:<kind>`。调用本身失败时按 `failed`（`unexpected`）处理。
- **重连状态**：`ConnectionToast` 在连接 `reconnecting` 或 `failed` 时挂一条 sticky 的 toast，回到 `ready` 后收起；头部状态点同时变成蓝色转圈或红点。

### 侧边栏（`sidebar/`）

- `parts.tsx` 是按 props 画的部分，`index.tsx` 是外框插槽的默认值，从 `useStreams().index` 读：头部订阅 `host`，列表订阅整个视图（`view`，索引流的事件不多），底部订阅 `elevations`。
- **头部**：产品名加状态点（绿、灰、红，连接中为蓝色转圈），tooltip 写状态、宿主版本和待安装的升级。
- **列表**：
  - "新会话"一行（在新建会话页时为选中色）；
  - "项目"小标题和"添加项目"（`add_project`）；
  - 项目按 `position` 排序；`trust_state` 不是 `trusted`（含 `stale`）时标"未信任"；悬停或获得焦点时出现"+"（在该项目新建会话）和一个空菜单（菜单项归 T153）；
  - 每个项目先显示 5 条会话（按 `last_activity_at` 倒序），其余"展开显示""收起"；
  - 会话行 30 高，标题与项目名对齐；行首：运行中转圈、出错图标（tooltip 和无障碍名称是按 `error_code` 映射的原因，未知码为"会话出错"）、未读蓝点；`needs_action` 时行尾按 `pending_kind` 写"等待批准"或"需要用户输入"（未知的 kind 按"需要用户输入"）；悬停 tooltip 写 agent 名；当前会话 `aria-current="page"`。
- **提权横幅**：底部卡片，最早的一条申请加"另有 n 条申请"，"查看"走 `elevation`。
- 会话标题、项目名、客户端名都只作为文本节点渲染。转圈用新 token `--animate-spin`（1.25 s 一圈），减少动态效果时停下。

### 截图

- 截图页的夹具加了 `index`：mock 宿主对每个订阅都回 `mode: snapshot`，随后发一页 `stream/snapshot`；`index` 流的 op 来自夹具，其他流为空。
- UI-sidebar：亮色、暗色（选中一条会话），窄窗口浮层（展开后）；UI-banners：提权横幅、两条反馈 toast（cooldown 30 秒、busy）和重连 toast 叠在顶部、头部转圈。重连状态的做法：让 mock 宿主不再回 `initialize`，再断开当前连接。
- UI-shell 的四张基线变了：侧边栏多了"新会话"一行和"项目"小标题（夹具没有数据），其余不变。重新生成后看过。
- **发现（范围外）**：Playwright 默认的逐像素阈值（0.2）比较宽，悬停底色（正文 5.3%）这类细微差异不会让比对失败。截图前把指针移开，基线也按无悬停状态重新生成。是否把阈值调严，交给 T118 的后续或 T152 决定。

### 审阅截图

`screenshots/`：截图页加载同样的夹具，把 `--font-sans` 换回系统字体（SF Pro、苹方）后截取，1280 × 820：侧边栏亮色、暗色、窄窗口浮层、横幅与 toast。按 F2，没有查看本机 Codex App 包。

### 行数（不含 notes、lock、截图基线）

| 模块 | 估计 | 实际 |
|---|---|---|
| 图标与 token | 40 | 66 |
| 文案 | 45 | 50 |
| Toaster、重连、特权反馈（含 `App`、外框接线） | 140 | 189 |
| 侧边栏（头部、列表、会话行、提权横幅、默认插槽） | 255 | 284 |
| 截图页夹具与依赖 | 20 | 17 |
| UI-sidebar、UI-banners | 100 | 101 |
| UNIT-ui-sidebar | 140 | 167 |
| 合计 | 745 | **874** |
