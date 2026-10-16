# 5.12 M1a 与 M1b 的范围划分

- 所属：`docs/02-design.md` 第 5 节
- 依据：PRD 第 5 节（分期与退出标准）、第 6、7 节（P0 需求）；ADR-03（SP-6 在 M1b 打磨界面之前）
- 本节只划分范围，不拆任务（任务拆分是阶段 ③）

## 5.12.1 划分原则

- **M1a 先把"宿主是唯一事实来源"做实**：协议、事件存储、会话编排、交互代理、本地认证、崩溃恢复，用 Claude 一个 agent 跑通，配一个最简的界面和完整的 cupper-cli。退出标准是 NFR-1 的验收项全部自动化通过。
- **M1b 横向扩展**：第二个 agent（Codex）、Git 与 worktree、审查面板、其余 P0 的输入和消息流功能、界面打磨、签名发布。SP-6 放在 M1b 开头、打磨界面之前。
- 协议和事件模型在 M1a 就按完整的 v1 设计，M1b 只加方法和事件（不改已有的契约），这本身也是对 NFR-1"新增而不改写"的一次演练。

## 5.12.2 M1a：宿主骨架

| 部分 | 内容 |
|---|---|
| 工程 | 仓库结构、Rust 和 pnpm 工作区、工具链锁定；CI 的 `rust`、`protocol`、`ts`、`golden`、`license` 任务 |
| `cupper-protocol` | v1 的全部类型（含 M1b 才用的方法，先定契约）；schema 导出；TS 生成链及后处理 |
| `cupper-sys` | 拉起与进程组、libproc、`LOCAL_PEERTOKEN`、SecCode 校验、csops |
| 宿主 | 网关（固定令牌握手、`codesign`/`dev_key`/`harness` 三种认证、control 和 window 角色）；策略与账本；事件存储（写线程、迁移与备份、检查点、回合快照、blob、原始帧）；会话编排（状态机、排队、中断兜底、自发回合、崩溃后 resume）；交互代理（权限、提问、计划确认）；关注度（状态、未读、在场、通知意图，暂不弹系统通知）；驱动层（**只有 Claude**：`AgentRuntime`、线路分接、提取层、选项对照表、CLI 发现、能力缓存与探测）；运行时安装器（Node、esbuild-wasm、claude-agent-acp）；进程监护（清理、自检、空闲退出、有序退出）；日志与脱敏 |
| cupper-cli | NFR-1 需要的全部命令、`elevate`、`repl`、`--json` |
| 假 agent | 脚本模式（含 5.9.2 新增的命令）、回放模式；导入 SP-1 录制；金样例流水线 |
| 桌面端（最简） | 主进程：拉起宿主、两类连接、原生扩展校验宿主、出方向白名单、真实输入检查、原生确认框、基本加固（sandbox、CSP、`app://`、导航限制）。界面：侧边栏（项目、会话、状态）、新建会话（Claude、本地模式）、消息流（Markdown、工具单行摘要、思考、错误）、审批和提问卡片、输入框（发送、排队、中断）、重连状态。只有开发构建和 e2e 构建 |
| 测试 | 宿主集成测试；端到端的 NFR-1 验收套件 |
| **退出标准** | NFR-1 的验收项全部由自动化测试通过（[5.9.4](09-testing.md#594-端到端测试与-nfr-1-验收)） |

## 5.12.3 M1b：桌面 MVP

| 部分 | 内容 |
|---|---|
| SP-6 | M1b 开头完成，结论决定主进程连接方案是否调整（[5.9.7](09-testing.md#597-sp-6-测试计划ui-渲染压力测试)） |
| Codex | `AgentRuntime`、Codex 提取层与选项对照表、`threadStatus`、资源回收；兼容矩阵跑一遍 Codex |
| Git 与 worktree | git 服务全部方法；新建 worktree（CP-5）；归档、恢复与清理（PS-3、GT-5）；审查面板（GT-1 至 GT-4，含行内评论 GT-2） |
| 输入框 | `@` 文件搜索、图片粘贴（`blob/put`）、斜杠命令菜单、模型/推理强度/权限模式选择（含完全访问的原生确认）、Steer、Enter/⌘Enter 设置、↑ 恢复 |
| 消息流 | 代码高亮（Shiki）、计划与待办、错误的恢复指引、按回合折叠与耗时 |
| 关注度与通知 | 系统通知（AP-3，依赖 SP-10 的通知部分）；跳到下一个需要处理的会话 |
| agent 状态 | 设置页的 agent 状态：CLI 路径与版本、已验证范围提示、登录状态、运行时、资源占用、重启（AG-2、AG-5） |
| 界面打磨 | Codex 式三栏布局、设置页各项、亮暗主题、快捷键、中英文、视觉（PS-1、UI-2 至 UI-6）；UI-0 在外部应用中打开 |
| 发布 | 发布构建、签名、公证、fuses 检查、electron-updater、升级演练、许可证门禁与第三方声明 |
| 其他 | 防休眠（NFR-13，M1 为 P1，成本很低，一并做）；SEC-8 的安全边界说明写进用户文档 |
| **退出标准** | 作者用两种 agent 完成 S1–S4 的完整流程；NFR-2、NFR-9 的验收测试通过（PRD 第 5 节） |

## 5.12.4 追溯矩阵（M1 的全部 P0 需求）

| 需求 | 要点 | 主要组件 | 协议 | 里程碑 | 验证 |
|---|---|---|---|---|---|
| AG-1 | 使用本机 CLI；适配器锁定版本；不再分发 Anthropic 组件 | 驱动层（CLI 发现）、运行时安装器、发布流水线 | `agents.updated` | Claude M1a；Codex M1b | 兼容矩阵；许可证门禁 |
| AG-2 | 检测 CLI 是否安装、是否登录，失败时给指引 | CLI 发现、提取层（`not_logged_in`、`cli_too_old`）、`error` 条目 | `agents/refresh`、`agents.updated`、`unavailable` 错误 | M1a 检测；M1b 设置页 | 假 agent `auth_none`；解析器单元测试 |
| AG-3 | 新建会话时选 agent，界面显示 agent 标识 | 会话编排、侧边栏、新建会话页 | `session/create`、`SessionSummary.agent` | M1a（Claude）/ M1b（两者） | e2e |
| AG-4 | 按能力显示功能；第一条消息前就能显示模型等 | 能力缓存、`SessionCapabilities`、新建会话页 | `agents/capabilities`、`session.capabilities` | M1b | 单元测试；e2e |
| AG-5 | 显示 CLI 和适配器版本，超出已验证范围时提示 | CLI 发现与版本检查、清单里的已验证范围 | `agents.updated` | M1b | 单元测试 |
| PS-1 | 三栏布局，可隐藏 | 桌面端布局 | — | M1b | 截图回归 |
| PS-2 | 添加、移除项目，会话按项目分组 | `IndexActor`、`projects` 表、侧边栏 | `project/add`、`project/remove`、`project.*` | 添加 M1a；移除 M1b | e2e |
| PS-3 | 新建、重命名、归档、恢复；worktree 会话的恢复规则 | 会话编排、git 服务 | `session/rename`、`session/archive`、`session/restore`、`worktree/check` | 新建 M1a；其余 M1b | 宿主集成测试；e2e |
| PS-4 | 状态指示、跳到下一个需要处理的会话 | `IndexActor` 的状态计算、侧边栏 | `SessionSummary.status/unread` | 状态 M1a；跳转 M1b | 单元测试；e2e |
| PS-8 | 重启后可以继续对话，历史以 cupper 的记录为准 | 事件存储（回合快照）、会话编排（resume） | `history/page`、`stream/subscribe` | M1a | NFR-1 第 5 项 |
| CP-1 | 多行输入，Enter/⌘Enter 可设置，↑ 恢复 | 输入框、界面偏好 | — | 基本 M1a；设置与 ↑ M1b | 组件测试 |
| CP-2 | 粘贴图片、`@` 引用文件 | 文件服务、blob | `files/search`、`blob/put`、`PromptContent` | M1b | e2e |
| CP-3 | 斜杠命令菜单，命令来自 agent | 能力缓存、输入框 | `SessionCapabilities.commands` | M1b | e2e |
| CP-4 | 模型、推理强度、权限模式选择 | 配置项映射、原生确认 | `session/set_config`、`session/create.config` | M1b | 单元测试；e2e |
| CP-5 | 新建会话时选本地或新建 worktree，显示分支名 | git 服务（worktree）、会话编排 | `session/create.workspace`、`workspace.*` | M1b | 宿主集成测试 |
| CP-6 | 运行中追加消息：宿主排队；支持时可以 Steer，失败转排队 | 会话编排（队列、Steer） | `session/send.mode`、`queue.updated` | 排队 M1a；Steer M1b | 状态机性质测试；e2e |
| CP-7 | 中断；agent 没响应时超时兜底 | 会话编排（`Cancelling`，10 s） | `session/interrupt` | M1a | 假 agent `wait_cancel`；e2e |
| MS-1 | 流式 Markdown、代码高亮、表格 | 消息流组件（streamdown、Shiki）；写入时合并 | `item.delta` | 基本 M1a；高亮 M1b | 截图回归 |
| MS-2 | 思考过程可折叠 | 提取层、`thought` 条目 | `item.*` | M1a | 回放金样例 |
| MS-3 | 工具活动单行摘要，可展开看输出、diff、退出码；按回合折叠显示耗时；拿不到时降级 | 提取层、`ToolCall`、`group_id` | `item.*`、`turn.completed.duration_ms` | M1a（Claude）/ M1b（Codex） | 回放金样例 |
| MS-4 | 计划和待办清单 | 提取层、`plan` 条目 | `item.*` | M1b | 回放金样例（Claude 受限，见 Q12） |
| MS-5 | 错误和恢复指引；agent 崩溃后自动重启并提示 | 统一错误条目、`notice`、`AgentRuntime` 重启 | `error`、`notice` 条目 | M1a | 假 agent `crash_cli`、`restartable` |
| AP-1 | 消息流内的审批卡片，Enter 批准、Esc 拒绝 | 交互代理、审批卡片、真实输入检查 | `interaction.*`、`interaction/respond` | M1a | e2e |
| AP-2 | 选项和文案以 agent 为准；持久选项要标明 | 选项对照表（`effect`、`requires`）、原生确认 | `Option.effect` | M1a（Claude）/ M1b（Codex） | 回放金样例 |
| AP-3 | 窗口不在前台时发系统通知 | `IndexActor` 通知意图、`notifier`、在场 | `notification.intent`、`client/presence` | M1b | e2e（意图）；人工（通知） |
| AP-4 | 先到先得，晚到提示"已在别处处理" | 会话 actor 串行处理、`resolved_by` | `conflict/already_resolved` | M1a | NFR-1 第 3 项 |
| AP-5 | 结构化提问以卡片呈现，任何客户端都能回答，状态为"需要处理" | 交互代理（`question`、`plan_confirm`）、状态计算 | `interaction.*` | M1a | 假 agent `form`、`plan_confirm` |
| GT-1 | 审查面板：未暂存、最近一回合；以仓库实际状态为准 | git 服务（快照、diff）、审查面板 | `git/diff`、`git/file_diff`、`snapshot.recorded` | M1b | git 服务测试；e2e |
| GT-2 | 行内评论一键发给 agent | 审查面板、`review_comment` 内容块 | `session/send` | M1b | e2e |
| GT-3 | 按全部、文件、hunk 暂存、取消暂存、还原 | git 服务（`DiffCache`、`apply`） | `git/stage`、`git/unstage`、`git/revert` | M1b | SP-5 的 82 个 hunk 用例 |
| GT-4 | 提交和推送 | git 服务 | `git/commit`、`git/push` | M1b | git 服务测试 |
| GT-5 | 归档时提示清理 worktree，有未提交或未推送内容时不删除 | git 服务（7 条规则） | `worktree/check`、`session/archive` | M1b | 7 条规则逐条测试 |
| UI-0 | 在外部终端或编辑器中打开工作区 | 宿主 `workspace/open` | `workspace/open` | M1b | e2e（mock 打开动作） |
| UI-2 | 设置页 | 桌面端、设置模块 | `settings/*`、`session/list_archived`、`agents.updated`、`clients.updated` | M1b | 截图回归 |
| UI-3 | 亮色和暗色，跟随系统 | 主题 token | — | M1b | 截图回归 |
| UI-4 | 核心快捷键 | 菜单与快捷键 | — | M1b | e2e |
| UI-5 | 中英文，跟随系统 | `@cupper/i18n` | 语义码 | M1b（M1a 先只有中文或英文一种） | 截图回归 |
| UI-6 | 视觉参照 Codex，不用其品牌素材 | 桌面端 | — | M1b | 人工评审 |
| SEC-6 | 默认不收集遥测 | 全部；crashReporter 本地保存 | — | M1a | 代码审查；e2e 抓包检查无外发（除更新检查和运行时安装） |
| SEC-8 | 只接受认证的本地客户端；agent 拿不到凭据；不监听网络端口 | 网关、`cupper-sys`、进程卫生、提权 | `initialize.auth`、`auth/elevate` | M1a | 进程卫生测试；签名校验测试；安全 e2e |
| SEC-10（M1 打数据基础） | 处理绑定到具体请求和内容；适配器退出后请求失效 | 交互代理（摘要、`adapter_instance`） | `request_digest`、`expired` | M1a | 宿主集成测试 |
| NFR-1 | 宿主持有一切；协议具备协商、授权、续传、幂等、按需获取 | 全部宿主模块、cupper-cli | 全部 | M1a | 5.9.4 的验收套件 |
| NFR-2 | 断线续传不丢不重；命令重试不重复执行 | 流（游标、快照）、账本 | `stream/*`、`command_id` | M1a 实现；M1b 验收 | e2e |
| NFR-4 | 会话数据只在本机 | 数据目录 | — | M1a | — |
| NFR-5 | 锁定适配器版本，升级经回归；已验证的 CLI 范围 | 运行时清单、兼容矩阵、金样例 | — | M1a（Claude）/ M1b（Codex） | 兼容矩阵 |
| NFR-6 | 冷启动 2 秒内可用；流式不卡；长会话流畅 | 启动路径、快照窗口、虚拟滚动 | — | 预算 M1a；SP-6 与优化 M1b | 性能预算 |
| NFR-7 | 宿主自己实现排队、状态、diff、worktree、索引、路由；私有扩展探测到才用 | 会话编排、git 服务、能力描述 | `SessionCapabilities` | M1a / M1b | — |
| NFR-8 | 单实例；界面崩溃不影响会话；退出前提示；升级时推迟重启宿主 | 进程监护、`lifecycle`、更新流程 | `host/status`、`host/exit`、`host/prepare_upgrade` | 单实例与退出提示 M1a；升级 M1b | e2e；升级演练 |
| NFR-9 | 宿主崩溃和 agent 崩溃后的恢复，不损坏历史 | 启动恢复、`AgentRuntime` 重启、SQLite WAL | `interrupted`、`expired`、`notice` | M1a 实现；M1b 验收 | e2e；断电测试 |
| NFR-10 | 数据有版本，升级自动迁移并备份；不支持降级；协议版本协商 | 迁移、升级器、`initialize` | `protocol` 区间 | M1a | 迁移测试 |
| NFR-11 | 同时运行至少 6 个会话（磁盘占用可见为 P1） | 共享适配器、资源采样 | `agents.updated` | M1b | e2e 6 个会话 |
| NFR-12（P0 部分） | 日志轮转并脱敏；收集适配器的错误输出 | 诊断、stderr 管道 | — | M1a | 脱敏测试 |
| 第 8 节合规 | 不改写 agent 的配置（"总是允许"由 agent 自己写入除外）；不使用商标 | 驱动层只通过 ACP 操作；兼容矩阵前后对比配置哈希 | — | M1a | 兼容矩阵的配置哈希对比 |

M1 里的 P1 只有 NFR-13（防休眠）顺带做进 M1b，其余 P1 留给 M2。
