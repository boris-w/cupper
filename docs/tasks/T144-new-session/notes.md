# T144 笔记

## 开工前的估算（2026-11-16）

### 现状

- **新建会话页**（T085，`newsession/NewSessionPage.tsx`）：
  - 有大标题和项目选择；
  - 动作条只有 Claude 和"本地"；
  - 选中的 agent 经 `Composer` 的 `create` 传给 `useSend`。
- **底栏的模型和权限按钮**（`model.tsx`、`permission.tsx`）：
  - 都经 `WithSession` 读会话流，新建会话页上不显示；
  - 斜杠命令在新建会话页读 `agents/capabilities` 的缓存（`Cached`，只在输入 `/` 后才查询）。
- **主进程**：已有特权操作 `create_session`，完全访问的新建会话经原生确认后再发 `session/create`。界面只要在选中完全访问时改走它。
- **顶栏**：现在只有右侧按钮，没有会话标题。设计 5.7.4 要求"会话标题；worktree 会话在标题后显示分支名；agent 标识"。计划 T144 一行的"分支名显示在顶栏标题"要先有标题。
- **worktree 失败、LFS、`not_found`**：界面还没有任何显示。
- **Codex 在未信任项目里的报错**：`project_untrusted` 没有文案。这条交接给了 T151，但 T151 没做。
- **e2e**：
  - fixture 的 `test_config.agents` 可以加 `codex: {program: 假 agent, args: ["--persona", "codex"]}`；
  - Codex 在未信任的项目里不开会话，所以用例要先经横幅和原生确认信任项目。
  - PR 上的 e2e 只跑 `@smoke`，nightly 跑全部。所以 E2E-new-session-agent-codex 带 `@smoke`，before-first-message 不带。

### 做法（按模块）

1. **宿主与协议**：
   - 设置里加 `sessions.default_permission.claude`、`sessions.default_permission.codex`，取值为 `Option<String>`，`null` 表示沿用 CLI 配置；
   - 宿主默认值为 `null`；
   - 不算安全相关的键，`policy::method` 的穷举里登记为普通键；
   - 跑 `pnpm gen`；协议和宿主各补一条测试。
2. **能力的来源**（`composer/config.tsx`）：
   - 新增 `WithOptions`：会话里取会话流，新建会话页取 `agents/capabilities` 的缓存。
   - 缓存查询抽成 `useCached(agent)`，斜杠命令也用它。
   - 新建会话页上的选择存在 UI store（`newConfig[agent]`），会话建好后清掉。
   - 权限的预选：`sessions.default_permission.<agent>` 在缓存的能力里时用它，否则按缓存里的当前值（即 CLI 配置）。
   - 模型、权限按钮和 `TierHint` 改用 `WithOptions`。这样新建会话页上就有这些按钮，档位为 `elevated` 时也有提示。
3. **动作条与发送**：
   - **agent 下拉**：列出 Claude 和 Codex，各带可用状态。所选 agent 不可用时，按 AG-4 禁止发送，原因写在动作条上。
   - **工作区**：本地，或新建 worktree。非 Git 项目不能选 worktree，并提示"不是 Git 仓库或是仓库的子目录"（T234）。
   - **起始分支**：选 worktree 后，从 `git/branches` 里选，默认当前分支，本地分支在前。本地模式不显示分支选择。
   - **发送**：`session/create` 带上 `workspace{worktree, base_ref}` 和 `config`（用户的选择，以及有效的默认权限）。权限是完全访问时改走特权操作 `create_session`（原生确认）。
4. **Q44 设置行**：
   - 设置页"通用"加"默认权限"，Claude 和 Codex 各一行；
   - 选项是"沿用 CLI 配置"，以及缓存里该 agent 的权限模式；
   - 存下的值已不在能力里时，提示"已按沿用 CLI 配置处理"。
5. **Q45**：
   - 进入会话页时，把它的项目记进 UI 偏好 `newSession.lastProject`；
   - 新建会话页没有指定项目时，取这个项目，在 `useSessionState` 一处解析；
   - 它已不存在时不预选（见下面的问题 2）。
6. **失败提示**：
   - 会话页加工作区横幅：
     - `filter_unsupported` 用 5.2.10 的文案；
     - `lfs_object_missing` 提示在终端里运行 `git lfs fetch`；
     - `not_found` 提示可以自己运行 `git worktree prune`；
     - 其他情况显示通用文案和错误码。
   - 队列加暂停原因 `workspace_failed` 的文案。
   - 补上 `stream:error_project_untrusted`。
7. **顶栏**：会话页显示会话标题、agent 标识，worktree 会话再显示分支名。
8. **用例**：
   - **单元测试**：
     - 新建会话页：
       - agent 下拉的两项和不可用状态；
       - 工作区与分支；
       - 不可用时禁止发送；
       - `create` 的参数；
       - 完全访问走特权操作；
       - 缓存的模型和权限；
       - 默认权限的预选和失效。
     - 设置页的默认权限一行；
     - Q45 的预选和失效；
     - 工作区横幅；
     - 顶栏。
   - **e2e**：
     - **E2E-new-session-agent-codex**（`@smoke`）：信任项目后选 Codex，在本地发送，会话的 agent 是 codex，顶栏有 Codex 标识。
     - **E2E-new-session-before-first-message**（nightly）：先用一个会话填好能力缓存，再看新建会话页上的权限、模型、斜杠命令和默认权限的预选；改成 `elevated` 档后出现提示，发出后，会话的配置就是所选的值。

### 估算（行，含测试；按 `cargo xtask size` 的口径）

T182 时每个模块都低估了约一半，这次按 prettier 折行后的实际写法估算。

| 模块 | 代码 | 测试 |
|---|---|---|
| 1 宿主与协议的设置键 | 15 | 15 |
| 2 能力的来源（`WithOptions`、缓存查询、新建会话页的选择、预选、`TierHint`；改模型、权限、斜杠命令三个扩展） | 125 | （在 3 里） |
| 3 动作条与发送（agent、工作区、分支、禁止发送、`config`、完全访问走特权操作） | 145 | 150 |
| 4 Q44 设置行 | 55 | 40 |
| 5 Q45 启动预选 | 30 | 35 |
| 6 失败提示 | 40 | 30 |
| 7 顶栏（标题、agent 标识、分支名） | 35 | 20 |
| i18n 文案 | 35 | |
| 两个 e2e 用例 | | 120 |
| **合计** | **480** | **410** |

总计约 890 行，超过硬线 720。

### 可选的拆法

- **方案 A**：本任务只做新建会话页本身，即模块 2、3、6 和两个 e2e 用例，约 600 行。另立任务做 Q44（模块 1、4，以及页面上的预选）、Q45（模块 5）和顶栏（模块 7），约 290 行。
- **方案 B**：本任务做模块 1 到 6，约 830 行；顶栏留给 T153，T153 本来就要做顶栏的菜单。
- **方案 C**：不拆，提高规模线。

### 要编排者定的问题

1. agent 不可用（缺 CLI、未登录、运行时失败）时，是否禁止发送？我倾向禁止，原因写在动作条上（AG-4）。
2. Q45 中上次的项目已不存在时，"不预选"是指显示"我们要构建什么？"、不选任何项目（这时标题里也要有项目选择），还是退回现在的做法、选列表里第一个项目？
3. 顶栏的会话标题、agent 标识和分支名，是在本任务做，还是交给 T153 或 T148？
4. 新建会话页上选的模型、权限、推理强度，会话建好后清掉（我的做法），还是在下一次新建时保留？
