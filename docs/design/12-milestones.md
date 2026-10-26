# 5.12 M1a 与 M1b 的范围划分

- 所属：`docs/02-design.md` 第 5 节
- 依据：PRD v1.2 第 5 节（分期与退出标准）、第 6、7 节（P0 需求）、第 10 节（验收口径）；ADR-03（SP-6 在 M1b 打磨界面之前）；设计评审会纪要 D15、D16（`docs/meetings/2026-10-16-design-review/minutes.md`）
- 本节只划分范围，不拆任务（任务拆分是阶段 ③）

## 5.12.1 划分原则

- **M1a 先把"宿主是唯一事实来源"做实**：协议、事件存储、会话编排、交互代理、本地认证、项目信任、崩溃恢复，用 Claude 一个 agent 跑通，配一个最简的界面和完整的 cupper-cli。退出标准是 NFR-1 的验收项全部自动化通过，并且稳定。
- **M1b 横向扩展**：第二个 agent（Codex）、Git 与 worktree 的其余部分、审查面板、其余 P0 的输入和消息流功能、界面打磨，以及作者自用的本机构建。Developer ID 签名、公证和自动更新推迟到用户开通 Apple Developer Program 账号之后（2026-10-16 用户决定，见 5.12.3"发布（推迟）"）。SP-6 放在 M1b 开头、打磨界面之前，只测渲染。
- **先打地基**：M1a 的第一个宿主任务是"核心骨架与测试基础设施"（时钟、纯核心、传输接口、故障点、测试配置通道），其他宿主任务都依赖它；驱动层之前先做技术验证（纪要 D15）。
- **架构按最终形态写，视觉从简**：M1a 的 store、订阅策略、行虚拟化、streamdown 配置、HostPort 生命周期、ShellAPI 都按终态实现，SP-6 测的就是最终的代码，M1b 的界面打磨不变成重写（桌面 #13）。
- **契约在 M1a 冻结**：协议和事件模型在 M1a 就按完整的 v1 设计，M1b 只加方法、事件、可选字段和开放枚举的取值，不改已有的契约；这本身也是对 NFR-1"新增而不改写"的一次演练。冻结清单和机制见 [5.12.5](#5125-契约冻结清单)。

## 5.12.2 M1a：宿主骨架

| 部分 | 内容 |
|---|---|
| 工程 | 仓库结构、Rust 和 pnpm 工作区、工具链锁定；CI 的 `rust`（含金样例）、`protocol`、`ts`、`ui`、`license`（第一层）、`testdata-scan`、`e2e`、`security-macos14`、`compat-framework`（[5.1.6](01-repo-build-ci.md#516-ci-流水线)） |
| **核心骨架与测试基础设施**（第一个宿主任务） | `Clock` 和 `IdGen`、`disallowed-methods`；纯核心的框架；`AcpTransport`（stdio 和内存）；故障点；`--test-config` 和 stdin 启动载荷；`--build-info`；写线程的组提交（[5.2.16](02-host.md#5216-时间与可测性)） |
| **技术验证**（驱动层之前；v1.6：T003 已完成，K1 通过，结论见 5.6.10、5.8.4、5.13） | Q8 的 Claude 部分：用 SP-1 的 TS 客户端补跑并发剧本（U-R5，已定（2026-10-16），允许运行）；Q9：SDK 3.3 的扩展方法；SP-4b 遗留第 7 条：真实适配器拉起工具命令时会不会 setsid；Q26：Electron 44 上 Playwright、`sendInputEvent`、`input-event` 与 `before-input-event` 的关系，并用实体键盘确认一次（在 windowBridge 任务里）；Q27：整个 App 的外发检查方法 |
| `cupper-protocol` | v1 的全部类型（含 M1b 才用的方法，先定契约）；schema 导出，含 `x-scope`、`x-idempotent`、`x-mergeable`、`x-id`、`x-time`、`x-path` 注解和视图的序列化格式；TS 生成链及后处理；合并时冻结（[5.12.5](#5125-契约冻结清单)） |
| `cupper-sys` | `spawn`（SETSID、`CLOEXEC_DEFAULT`、环境变量白名单）、进程组、libproc、`LOCAL_PEERTOKEN` 和 `LOCAL_PEERPID`、SecCode 校验、csops、`kern.bootsessionuuid`、`DARWIN_USER_TEMP_DIR` |
| 宿主 | 网关（固定令牌握手、`initialize` 只接受一次、`codesign`/`dev_key`/`harness` 三种认证及双向证明、control 和 window 角色）；策略与账本（档位规则、真实输入证据、项目信任的门控）；事件存储（组提交与 savepoint、按脚本更新版本号的迁移与备份、检查点、不可变的回合快照、blob、原始帧、持久性配置、`data_epoch`）；会话编排（纯核心的状态机、排队、中断兜底、自发回合与出队保护、崩溃后 resume、第一条消息随受理落盘、准备阶段的恢复、晚到的工具输出、`config_unconfirmed`）；**项目信任**（扫描、摘要、`settingSources`、stale 检测）；交互代理（权限、提问、计划确认、未知 kind）；关注度（状态、未读、只采信桌面端的在场、通知意图，暂不弹系统通知）；驱动层（**只有 Claude**：`AgentRuntime`、线路分接、提取层、选项对照表、CLI 发现、环境变量规则、能力缓存与探测）；**git 的快照档**（独立 GIT_DIR 的回合快照，`GitCmd` 从第一次调用起就分三档）；运行时安装器（Node、esbuild-wasm、claude-agent-acp）；进程监护（清理、自检、空闲退出、有序退出）；日志与脱敏 |
| cupper-cli | NFR-1 需要的全部命令、`elevate`、`repl`、`--json`、`--build-info`；`--harness-key-fd` 只在测试构建里有 |
| 假 agent | 启动配置（persona、能力、登录状态、故障注入）、剧本命令（含 5.9.2 新增的命令）、原始行通道、全局不变式、回放模式；录制格式规格和 SP-1 录制的一次性转换；A 层金样例；第一批 B 层金样例（维护者用 Claude 跑兼容套件产出） |
| 桌面端（最简） | **主进程**：拉起宿主（经原生扩展）、两类连接、原生扩展校验宿主（含双向证明）、数据连接的生命周期、出方向的解析与白名单、真实输入检查、原生确认框、基本加固（sandbox、CSP 与 Trusted Types、`app://cupper`、命令行开关白名单、导航限制）；**windowBridge 的测量闸门**（[5.7.1](07-desktop.md#571-进程与连接)，不达标就在界面任务之前改用后备方案）。**ShellAPI**：M1a 的完整接口（冻结）。**界面**（按最终形态的架构；v1.4 起 token 和外框直接按 [5.7.10](07-desktop.md#5710-设计参数) 的 Codex 取值）：三栏外框与窄窗口行为；侧边栏（项目、会话、状态的转圈、蓝点和"等待批准""需要用户输入"标签、"未信任"标记）；新建会话（Claude、本地模式）；消息流（按行虚拟化、streamdown 按 5.7.8 的配置、工具单行摘要、思考、错误、提示、**带状态的回合标题**，显示已取消、已中断、失败，不做折叠）；**请求面板**（替换输入框的位置；待命期、数字键与焦点规则；含"已失效""已在 cupper-cli 处理"等状态、不可见字符的转义、"已截断"；消息流里的记录行）；**排队消息的预览**；输入框（发送按钮三态；运行中发送，M1a 没有 Steer，一律排队；两次 Esc 中断）；重连状态；**Toaster**（顶部居中）；**提权横幅与原生确认框**；**添加项目时的信任确认**；**会话档位待确认的横幅**；弱认证标记。所有文案从第一天起走 i18n 的键，先只做一种语言。只有开发构建和 e2e 构建 |
| 测试 | 宿主集成测试（含故障点、`data_epoch` 用例、6 个会话共用假适配器）；性质测试；NFR-1 的端到端验收套件；安全 e2e（e2e 构建上能跑的部分）；发布 CSP 下 `securitypolicyviolation` 为 0 |
| **退出标准** | NFR-1 的验收项（[5.9.4](09-testing.md#594-端到端测试与-nfr-1-验收) 的 E2E-NFR1-1、E2E-NFR1-2、E2E-NFR1-3、E2E-NFR1-4、E2E-NFR1-5）全部由自动化测试通过，并且：在退出候选的**同一个提交**上、按平时的并行度**连续 20 次全绿**（v1.3：仓库没有远端时，由 `xtask ci e2e --suite nfr1` 在作者机器上运行，另加 5 次降速变体，口径见 `docs/03-plan.md` 1.8 和 T099）；不允许重试、跳过或隔离用例；中途失败一次，查明根因后计数清零（测试 #17，纪要 D15）。协议和 ShellAPI 的契约已冻结（[5.12.5](#5125-契约冻结清单)） |

## 5.12.3 M1b：桌面 MVP

| 部分 | 内容 |
|---|---|
| SP-6 | M1b 开头完成，只测渲染（主进程的连接方案已在 M1a 的测量闸门定下），场景见 [5.9.7](09-testing.md#597-sp-6-测试计划ui-渲染压力测试)；同时完成 Q29（审查文档）的验证 |
| SP-10 的通知部分 | M1b 开始前完成（Q2），AP-3 依赖它 |
| Codex | 先跑 Q8 的 Codex 部分（U-R5，已定（2026-10-16））；`AgentRuntime`、Codex 提取层与选项对照表、`threadStatus`、晚到的工具输出（`may_still_run`、`late_tool_output`）、未信任项目上拒绝启动、资源回收；兼容矩阵跑一遍 Codex，并用宿主重新录到取消场景，补上 K3 的 B 层金样例 |
| Git 与 worktree | git 服务全部方法（三档调用，Q32 逐键确认覆盖的效果，LFS 路径实测）；新建 worktree（CP-5）；归档、恢复与清理（PS-3、GT-5）；审查面板（GT-1 至 GT-4；行内评论作为附件进入输入框，GT-2 v1.3；文件树和底部的整组操作，F3）：审查文档（Q29 通过时）或 `DiffHunksRenderer` 的后备方案；"清除快照对象" |
| 输入框 | `@` 文件搜索、拖入文件、图片粘贴（`blob/put`）、斜杠命令菜单、模型/推理强度/权限模式选择（档位规则、完全访问的原生确认）、Steer（默认插话、⌘Enter 排队、"跟进消息默认排队"设置）、排队消息的编辑与插话（`session/queue_edit`，F3）、Enter/⌘Enter 设置、↑ 恢复 |
| 消息流 | 代码高亮（Shiki）、计划与待办、计划药丸（F3）、错误的恢复指引、按回合折叠与耗时（最终回答、始终显示在折叠之外的条目）、回合改动卡片与"撤销"（F3）、晚到输出的显示 |
| 关注度与通知 | 系统通知（AP-3）、通知点击后的 `ui/navigate`；跳到下一个需要处理的会话 |
| agent 状态 | 设置页的 agent 状态：CLI 路径与版本、已验证范围提示、登录状态、配置目录的校验结果、运行时、资源占用、重启（AG-2、AG-5） |
| 界面打磨 | 100% 复刻 Codex（UI-6 v1.3，[5.7.4](07-desktop.md#574-窗口与布局)、[5.7.10](07-desktop.md#5710-设计参数)）：暗色与未核实项的核对（Q36–Q40）、新建会话页布局、半透明侧边栏、设置页各项（含跟进行为、存储、安装命令行工具）、亮暗主题、快捷键（按 Codex 的默认值）、中英文（PS-1、UI-2 至 UI-6）；UI-0 的"打开"按钮 |
| 自用构建 | 作者日常使用的本机构建：`release-config` 每晚构建（fuses、调试开关、asar 清单、产物扫描，不签名）；许可证门禁第一层；**Q35 的技术验证**：用本机签名身份给自用构建签名，通过则自用构建走签名校验认证（CLI 可用，NFR-8 成立），不通过则自用构建按退化模式运行。自用构建单独一套身份和数据目录，见 [5.7.9](07-desktop.md#579-构建种类) 的"自用构建"一列 |
| 发布（推迟） | **前提：用户开通 Apple Developer Program 账号**（2026-10-16 用户决定暂不开通，先自用）。内容：发布构建（`dev.cupper` 前缀）、Developer ID 签名、公证（Node 在 `Helpers/` 里随 App 公证）、许可证门禁第二层、签名后冒烟、RC 标签、electron-updater（安装空窗、安装位置检查）、升级演练、第三方声明。不在 M1b 的退出标准内 |
| 其他 | 防休眠（NFR-13，M1 为 P1，成本很低，一并做）；SEC-8 的安全边界说明写进用户文档，含"系统权限会被 agent 继承"的警示，发布前必须完成（MAN-security-doc） |
| **退出标准** | ① 作者用两种 agent 走完 S1–S4，按逐步检查表（MAN-S1、MAN-S2、MAN-S3、MAN-S4）记录步骤、通过条件、CLI 和适配器的版本、日期，结果存档；② NFR-2 的用例 E2E-NFR2-retry、IT-NFR2-epoch 通过；③ NFR-9 的用例 E2E-NFR9-adapter-crash、E2E-NFR9-cli-crash、IT-NFR9-failpoints、IT-NFR9-crash-under-load 通过（[5.9.4](09-testing.md#594-端到端测试与-nfr-1-验收)）。PRD 第 5 节 |

## 5.12.4 追溯矩阵（M1 的全部 P0 需求）

"验证"一列引用 [5.9.10](09-testing.md#5910-测试编号与人工验证清单) 的用例编号；后缀说明在哪个套件里跑：〔PR〕每个 PR，〔晚〕每晚，〔发〕发布流水线，〔人〕人工。

| 需求 | 要点 | 主要组件 | 协议 | 里程碑 | 验证 |
|---|---|---|---|---|---|
| AG-1 | 使用本机 CLI；适配器锁定版本；不再分发 Anthropic 组件 | 驱动层（CLI 发现）、运行时安装器、发布流水线 | `agents.updated` | Claude M1a；Codex M1b | MAN-compat〔人〕；`license` 第一层〔PR〕；许可证门禁第二层〔发〕 |
| AG-2 | 检测 CLI 是否安装、是否登录，失败时给指引 | CLI 发现、提取层（`not_logged_in`、`cli_too_old`）、`error` 条目 | `agents/refresh`、`agents.updated`、`unavailable` 错误 | M1a 检测；M1b 设置页 | IT-auth-none、IT-K14-old-cli-auth〔PR〕；UNIT-cli-resolver〔PR〕 |
| AG-3 | 新建会话时选 agent，界面显示 agent 标识 | 会话编排、侧边栏、新建会话页 | `session/create`、`SessionSummary.agent` | M1a（Claude）/ M1b（两者） | E2E-new-session-agent〔PR〕 |
| AG-4 | 按能力显示功能；第一条消息前就能显示模型等 | 能力缓存、`SessionCapabilities`、新建会话页 | `agents/capabilities`、`session.capabilities` | M1b | UNIT-capabilities〔PR〕；E2E-new-session-before-first-message〔晚〕 |
| AG-5 | 显示 CLI 和适配器版本，超出已验证范围时提示 | CLI 发现与版本检查、清单里的已验证范围 | `agents.updated` | M1b | UNIT-version-classify〔PR〕；`testdata-scan` 的范围比对〔PR〕 |
| PS-1 | 三栏布局，可隐藏 | 桌面端布局 | — | M1b | UI-layout〔PR〕 |
| PS-2 | 添加、移除项目，会话按项目分组；添加时确认是否信任项目配置（PRD v1.2） | `IndexActor`、`projects` 表、`trust` 模块、侧边栏 | `project/add`、`project/trust`、`project/remove`、`project.*` | 添加与信任 M1a；移除 M1b | E2E-project-add-trust〔PR〕；IT-trust-stale、IT-trust-rescan-before-prompt、IT-trust-settingsources〔PR〕；COMPAT-untrusted-project〔人〕 |
| PS-3 | 新建、重命名、归档、恢复；worktree 会话的恢复规则 | 会话编排、git 服务 | `session/rename`、`session/archive`、`session/restore`、`worktree/check` | 新建 M1a；其余 M1b | IT-session-archive-restore〔PR〕；E2E-archive-cleanup〔晚〕 |
| PS-4 | 状态指示、跳到下一个需要处理的会话 | `IndexActor` 的状态计算、侧边栏（转圈、蓝点、文字标签，v1.4） | `SessionSummary.status/unread/pending_kind` | 状态 M1a；跳转 M1b | UNIT-index-status〔PR〕；UI-sidebar〔PR〕；E2E-next-needs-action〔晚〕 |
| PS-8 | 重启后可以继续对话，历史以 cupper 的记录为准 | 事件存储（回合快照）、会话编排（resume） | `history/page`、`stream/subscribe` | M1a | E2E-NFR1-5〔PR〕；IT-history-from-host〔PR〕 |
| CP-1 | 多行输入，Enter/⌘Enter 可设置，↑ 恢复 | 输入框、界面偏好 | — | 基本 M1a；设置与 ↑ M1b | UNIT-composer-ime（`isComposing`、keyCode 229）〔PR〕；E2E-send-key-setting〔晚〕；MAN-ime〔人〕 |
| CP-2 | 粘贴图片、`@` 引用文件 | 文件服务（快照档）、blob | `files/search`、`blob/put`、`PromptContent` | M1b | E2E-image-paste、E2E-file-ref〔晚〕 |
| CP-3 | 斜杠命令菜单，命令来自 agent | 能力缓存、输入框 | `SessionCapabilities.commands` | M1b | E2E-slash-commands〔晚〕 |
| CP-4 | 模型、推理强度、权限模式选择 | 配置项映射、档位规则、原生确认 | `session/set_config`、`session/create.config`、`session/confirm_config` | M1b（`config_unconfirmed` 的处理 M1a） | UNIT-tier-rules〔PR〕；IT-config-unconfirmed〔PR〕；E2E-full-access-native〔晚〕 |
| CP-5 | 新建会话时选本地或新建 worktree，显示分支名 | git 服务（worktree）、会话编排 | `session/create.workspace`、`workspace.*` | M1b | IT-worktree-create〔PR〕；E2E-S1〔晚〕 |
| CP-6 | 运行中追加消息：默认插话、⌘Enter 排队，不支持或失败时转为排队（v1.3）；排队消息可以编辑、插话（F3） | 会话编排（队列、Steer、`queue_edit`）、输入框 | `session/send.mode`、`session/queue_edit`、`queue.updated` | 排队 M1a；Steer、编辑与插话 M1b | PROP-queue-fifo〔PR〕；IT-K10-steer-explicit-only、IT-K11-steer-idle-queued、IT-queue-edit〔PR〕；E2E-NFR1-2〔PR〕；E2E-composer-steer-default、E2E-queue-edit〔晚〕 |
| CP-7 | 中断；agent 没响应时超时兜底 | 会话编排（`Cancelling`，10 s） | `session/interrupt` | M1a | PROP-interrupt-10s〔PR〕；IT-K1-close-resume（`hang`）〔PR〕；E2E-NFR1-2〔PR〕 |
| MS-1 | 流式 Markdown、代码高亮、表格 | 消息流组件（streamdown、Shiki）；写入时合并 | `item.delta` | 基本 M1a；高亮 M1b | UI-stream-markdown、UI-code-highlight〔PR〕；UNIT-md-injection（Markdown 注入金样例）〔PR〕 |
| MS-2 | 思考过程可折叠 | 提取层、`thought` 条目 | `item.*` | M1a | IT-thought-claude（claude 形态的 `think`）〔PR〕；GOLD-A-codex-s02（Codex 的录制里有 `agent_thought_chunk`）〔PR〕。SP-1 的 Claude 录制里没有思考帧（s02d 也没有），真实的 Claude 思考流录制由兼容矩阵用 sonnet 或 opus 补（COMPAT-claude-thinking〔人〕） |
| MS-3 | 工具活动单行摘要，可展开看输出、diff、退出码；按回合折叠显示耗时；拿不到时降级 | 提取层、`ToolCall`、`group_id`、`final_item_ids` | `item.*`、`turn.*` | M1a（Claude）/ M1b（Codex、折叠） | GOLD-A-tool-fields（各 `tool_kind` 的必填字段）〔PR〕；UNIT-final-items、UNIT-grouping〔PR〕；UI-stream-tools、UI-stream-fold〔PR〕 |
| MS-4 | 计划和待办清单；运行中的计划药丸"第 n / m 步"（F3） | 提取层、`plan` 条目 | `item.*` | M1b | GOLD-A-claude-s05、GOLD-A-codex-s05（两边的 `plan_update` 都在 s05 里）〔PR〕；IT-plan-entries（假 agent 的 `plan_update`）〔PR〕；Claude 的口径见 PRD v1.2 第 10 节（Q12） |
| MS-5 | 错误和恢复指引；agent 崩溃后自动重启并提示 | 统一错误条目、`notice`、`AgentRuntime` 重启 | `error`、`notice` 条目 | M1a | IT-K6-crash-resume、IT-K6-output-then-crash、IT-K7-restartable、IT-K17-text-error〔PR〕；E2E-error-guidance〔晚〕 |
| AP-1 | 审批以请求面板呈现，替换输入框的位置，选项带编号，Enter 批准、Esc 拒绝（v1.3） | 交互代理、请求面板、真实输入检查 | `interaction.*`、`interaction/respond`（`input.key` 含 `digit`） | M1a | E2E-SEC-real-input、E2E-SEC-panel-arming、E2E-SEC-panel-numbering、E2E-approve-enter-esc、E2E-reject-with-message〔PR〕；MAN-keyboard〔人〕 |
| AP-2 | 选项和文案以 agent 为准；持久选项要标明 | 选项对照表（`effect`、`requires`）、原生确认 | `Option.effect` | M1a（Claude）/ M1b（Codex） | GOLD-A-option-map（选项对照表）〔PR〕；UNIT-option-map〔PR〕；E2E-persistent-option-native〔晚〕 |
| AP-3 | 窗口不在前台时发系统通知 | `IndexActor` 通知意图、`notifier`、在场 | `notification.intent`、`client/presence`、`ui/navigate` | M1b | IT-notification-intent、UNIT-notifier-no-actions（系统通知没有任何操作按钮）〔PR〕；MAN-notification〔人〕 |
| AP-4 | 先到先得，晚到提示"已在别处处理" | 会话 actor 串行处理、`resolved_by` | `conflict/already_resolved` | M1a | E2E-NFR1-3〔PR〕 |
| AP-5 | 结构化提问以请求面板呈现（与 AP-1 相同的形态），任何客户端都能回答，状态为"需要处理"（v1.3） | 交互代理（`question`、`plan_confirm`）、状态计算、请求面板 | `interaction.*`、`SessionSummary.pending_kind` | M1a | IT-form、IT-plan-confirm〔PR〕；E2E-question-panel〔PR〕 |
| GT-1 | 审查面板：未暂存、最近一回合；以仓库实际状态为准 | git 服务（快照档、diff）、审查面板 | `git/diff`、`git/file_diff`、`snapshot.recorded` | 回合快照 M1a；其余 M1b | IT-git-diff-scopes〔PR〕；IT-git-snapshot-parity〔PR〕；E2E-S3〔晚〕 |
| GT-2 | 行内评论作为附件进入输入框，随下一条消息发给 agent（v1.3） | 审查文档的评论草稿、输入框的附件条、`review_comment` 内容块 | `session/send` | M1b | E2E-review-comment〔晚〕 |
| GT-3 | 按全部、文件、hunk 暂存、取消暂存、还原；回合改动卡片的"撤销"（F3） | git 服务（`DiffCache`、仓库档的 `apply`）、整组操作、回合改动卡片 | `git/stage`、`git/unstage`、`git/revert` | M1b | IT-git-hunk（SP-5 的 82 个用例）〔PR〕；E2E-turn-changes-undo、E2E-SEC-revert-confirm〔晚〕；口径见 PRD v1.2 第 10 节（Q13） |
| GT-4 | 提交和推送 | git 服务（用户档） | `git/commit`、`git/push` | M1b | IT-git-commit-push〔PR〕 |
| GT-5 | 归档时提示清理 worktree，有未提交或未推送内容时不删除 | git 服务（7 条规则） | `worktree/check`、`session/archive` | M1b | IT-worktree-check-rules〔PR〕 |
| UI-0 | 在外部终端或编辑器中打开工作区 | 宿主 `workspace/open` | `workspace/open` | M1b | E2E-workspace-open（mock 打开动作）〔晚〕 |
| UI-2 | 设置页 | 桌面端、设置模块 | `settings/*`、`session/list_archived`、`agents.updated`、`clients.updated` | M1b | UI-settings〔PR〕 |
| UI-3 | 亮色和暗色，跟随系统 | 主题 token | — | M1b | UI-theme〔PR〕 |
| UI-4 | 核心快捷键，按 Codex 的默认值（中断是运行中按两次 Esc，v1.3） | 菜单与快捷键、输入框 | — | 两次 Esc 与请求面板按键 M1a；其余 M1b | E2E-shortcuts〔晚〕 |
| UI-5 | 中英文，跟随系统 | `@cupper/i18n` | 语义码 | M1b（M1a 先只有一种语言，但全部文案走键） | UI-locale（两种语言）〔PR〕 |
| UI-6 | 100% 复刻本机安装的 Codex：布局、尺寸、配色、字体与字号、动效、交互、文案结构、快捷键；只复刻，不复制（v1.3） | 桌面端（[5.7.4](07-desktop.md#574-窗口与布局)、[5.7.10](07-desktop.md#5710-设计参数)） | — | token 与外框 M1a；其余 M1b | UI-shell、UI-layout〔PR〕；MAN-ui6〔人〕 |
| SEC-6 | 默认不收集遥测 | 全部；crashReporter 本地保存 | — | M1a | E2E-SEC-egress（`nettop` 采样，Q27）〔晚〕；宿主的 Seatbelt 外发检查〔PR〕 |
| SEC-8 | 只接受认证的本地客户端；agent 拿不到凭据；不监听网络端口 | 网关、`cupper-sys`、进程卫生、提权、项目信任 | `initialize.auth`、`auth/elevate` | M1a | UNIT-policy-matrix、UNIT-release-features（无特性运行）〔PR〕；IT-process-hygiene〔PR〕；E2E-SEC-csp、E2E-SEC-real-input、E2E-SEC-privileged、E2E-SEC-fake-host、E2E-SEC-review-iso〔PR〕；E2E-SEC-switches、E2E-SEC-egress〔晚〕；SMOKE-SIGNED-codesign-auth、SMOKE-SIGNED-no-weak-auth、SMOKE-SIGNED-cli-readonly、SMOKE-SIGNED-reattach、SMOKE-SIGNED-handshake-matrix、SMOKE-SIGNED-reject-fake-host、SMOKE-SIGNED-reject-weak-host〔发〕 |
| SEC-10（M1 打数据基础） | 处理绑定到具体请求和内容；适配器退出后请求失效 | 交互代理（摘要、`adapter_instance`） | `request_digest`、`expired` | M1a | IT-interaction-expire〔PR〕；UNIT-digest-jcs〔PR〕 |
| NFR-1 | 宿主持有一切；协议具备协商、授权、续传、幂等、按需获取 | 全部宿主模块、cupper-cli | 全部 | M1a | E2E-NFR1-1、E2E-NFR1-2、E2E-NFR1-3、E2E-NFR1-4、E2E-NFR1-5，连续 20 次全绿〔PR〕 |
| NFR-2 | 断线续传不丢不重；命令重试不重复执行 | 流（游标、快照、`data_epoch`）、账本 | `stream/*`、`command_id` | M1a 实现；M1b 验收 | E2E-NFR2-retry〔晚〕；IT-NFR2-epoch〔PR〕 |
| NFR-4 | 会话数据只在本机 | 数据目录 | — | M1a | E2E-SEC-egress〔晚〕；代码审查：宿主只访问 npm 仓库 |
| NFR-5 | 锁定适配器版本，升级经回归；已验证的 CLI 范围 | 运行时清单、兼容矩阵、金样例 | — | M1a（Claude）/ M1b（Codex） | MAN-compat〔人〕；`testdata-scan` 的范围比对、选项和档位取值检查〔PR〕 |
| NFR-6 | 冷启动 2 秒内可用；流式不卡；长会话流畅 | 启动路径、快照窗口、行虚拟化 | — | 预算与测量闸门 M1a；SP-6 与优化 M1b | PERF（5.9.6）〔晚〕；SP-6〔人〕 |
| NFR-7 | 宿主自己实现排队、状态、diff、worktree、索引、路由；私有扩展探测到才用 | 会话编排、git 服务、能力描述 | `SessionCapabilities` | M1a / M1b | IT-degrade-steering、IT-degrade-plan、IT-degrade-thinking：假 agent 分别关掉 steering、计划、思考能力时，功能按 AG-4 隐藏或降级（以后新增可降级的能力时按同样格式加编号）〔PR〕 |
| NFR-8 | 单实例；界面崩溃不影响会话；退出前提示；升级时推迟重启宿主 | 进程监护、`lifecycle`、更新流程 | `host/status`、`host/exit`、`host/prepare_upgrade` | 单实例与退出提示 M1a；升级 M1b | E2E-single-instance、E2E-quit-confirm〔晚〕；SMOKE-SIGNED-reattach〔发〕（"界面崩溃不影响会话"只对发布构建成立，PRD v1.2 第 10 节，Q22）；MAN-upgrade〔人〕 |
| NFR-9 | 宿主崩溃和 agent 崩溃后的恢复，不损坏历史 | 启动恢复、`AgentRuntime` 重启、SQLite WAL 与持久性配置 | `interrupted`、`expired`、`notice` | M1a 实现；M1b 验收 | E2E-NFR9-adapter-crash、E2E-NFR9-cli-crash〔晚〕；IT-NFR9-failpoints、IT-NFR9-crash-under-load〔PR〕 |
| NFR-10 | 数据有版本，升级自动迁移并备份；不支持降级；协议版本协商 | 迁移、升级器、`initialize` | `protocol` 区间 | M1a | IT-migrations〔PR〕；从第二个发布版起 IT-n-minus-1-cli〔PR〕 |
| NFR-11 | 同时运行至少 6 个会话（磁盘占用可见为 P1） | 共享适配器、资源采样 | `agents.updated` | M1b | IT-six-sessions-shared-adapter〔PR〕；E2E-six-sessions〔晚〕 |
| NFR-12（P0 部分） | 日志轮转并脱敏；收集适配器的错误输出 | 诊断、stderr 管道 | — | M1a | UNIT-redaction〔PR〕 |
| 第 8 节合规 | 不改写 agent 的配置（"总是允许"由 agent 自己写入除外）；不使用商标 | 驱动层只通过 ACP 操作；兼容矩阵前后对比配置哈希 | — | M1a | MAN-compat 报告里的配置哈希对比〔人〕 |

M1 里的 P1 只有 NFR-13（防休眠）顺带做进 M1b，其余 P1 留给 M2。SEC-9 属于 M3；PRD v1.2 在 SEC-9 里加的"不能切到更宽松的审批档位"，在 M1 体现为 [5.8.2](08-security.md#582-principal角色与-scope) 档位规则里 `remote_device` 一列（U-R2）。

## 5.12.5 契约冻结清单

纪要 D16。规则：协议契约和 ShellAPI 在 M1a 冻结，冻结时机见 `docs/03-plan.md`（T016，排在 windowBridge 测量闸门之后，v1.3）；之后 M1b 只加方法、事件、可选字段和开放枚举的取值，不改已有契约。下面第 1、2 组必须在冻结之前写进设计（本次修订已写）并在 M1a 实现。

**第 1 组：改变已有契约的形状或语义，必须在冻结前完成**

| # | 改动 | 定义在 |
|---|---|---|
| 1 | `initialize`：每条连接只接受一次，角色确定后不可变；`dev_key` 和 `harness` 认证改为 `{c_nonce, proof}`，结果带 `host_proof` 和 `data_epoch`；**证明绑定信道**：方向标签、双方的 nonce、各自在套接字上看到的对端 pid（纪要 D11） | [5.3.2](03-protocol-v1.md#532-握手与版本协商)、[5.8.1](08-security.md#581-认证握手固定令牌) |
| 2 | `stream/subscribe` 带 `epoch`；`epoch_changed`、`cursor_ahead` 时走快照，客户端整体重置视图 | [5.3.3](03-protocol-v1.md#533-流与同步) |
| 3 | `history/page` 的参数和结果改为 `{turn_no, op_index?}` 游标 | [5.3.3](03-protocol-v1.md#533-流与同步) |
| 4 | `item.delta` 增加 `field: text\|output`，合并键随之改为 `(item_id, field)`；命令输出的 `output_state` 语义 | [5.3.5](03-protocol-v1.md#535-事件类型目录) |
| 5 | 摘要改为完整的 256 位，规范 JSON 改为 RFC 8785（影响 `request_digest`、`params_hash`、`config_digest`） | [5.3.1](03-protocol-v1.md#531-通用约定)、[5.3.6](03-protocol-v1.md#536-交互请求模型) |
| 6 | 权限请求里要批准的内容（命令、路径、详情）不再按 1,000 字符截断，超长改为 blob 并设 `content_truncated` | [5.3.6](03-protocol-v1.md#536-交互请求模型) |
| 7 | 策略语义：档位规则（`set_config`、`session/create`、`mode_switch` 选项）及"默认档位视为由创建者设定"；项目信任对会话启动的门控；**window 角色的 `interaction/respond` 和 `git/revert` 必须带 `input` 证据，缺了就拒绝**，`escape` 只能用于拒绝类选项（纪要 D7）；kind 未知的选项一律 privileged；v1.4 增加数字键证据 `input.key: digit`，须与所选选项的序号相符 | [5.2.5](02-host.md#525-策略与命令账本)、[5.8.2](08-security.md#582-principal角色与-scope) |
| 8 | ShellAPI：`init()` 和 `host.open()` 改为异步并返回握手结果；`HostPort` 按字节块收发、`sendBlob`；`onCommand`（命令 ID 是开放枚举）、`onNavigate`；`requestPrivileged` 返回 `{status, result?}`；`ShellInfo` 带审查文档的地址；`PrivilegedAction` 包含 `add_project`、`trust_project`、`confirm_config`（桌面 #4 的三项补充） | [5.7.3](07-desktop.md#573-shellapi) |
| 9 | 宿主进程的启动契约：stdin 的 JSON 启动载荷与 2 s 超时、退出码 6、`--build-info`、`--test-config` | [5.2.13](02-host.md#5213-进程监护与宿主生命周期)、[5.8.1](08-security.md#581-认证握手固定令牌) |

**第 2 组：属于新增，但金样例和界面从 M1a 起就依赖，一并冻结**

- 时间字段：`turn.started.started_at`、`turn.completed.ended_at`、`item.started.started_at`、`item.completed.ended_at`；
- `turn.completed.final_item_ids` 及判定规则；活动组的规则；
- `ToolCall` 的 `query`、`url`、`parent_item_id`、`output_state`、`output_tail`、`output_bytes`，以及按 `tool_kind` 的必填字段表；
- `late_tool_output` 条目、`item.completed.may_still_run`、能力里的 `turn.interrupt_stops_commands`；
- `project/trust`、`project/trust_info`、`project/add` 结果里的 `trust`、`project.upserted.trust_state`、`session.configured.project_config`；
- `session/confirm_config`、`session.configured.confirmation_required`、`conflict/config_unconfirmed`；
- `snapshot.recorded` 的 `stat` 和 `reason`；`git/diff` 的 `caveats`、`stat`、`filter_unsupported`（方法本身是 M1b 的，字段一并定）；
- `Option.effect` 的 `unknown`、`tier_after`、`rule`；
- 新的错误 reason 和 notice code；`ui/navigate`；`host/status` 在测试构建里的 `pid`；
- schema 注解 `x-id`、`x-time`、`x-path` 和视图的序列化格式；
- v1.4（界面复刻 Codex）：`SessionSummary.pending_kind`；`queue.updated` 条目的 `content`；`session/queue_edit` 的 `ops`（`replace`、`steer`、`remove`、`move`，M1b 实现前两种，方法本身在 M1a 已有）；
- v1.5（请求面板的安全确认）：`session/send.reply_to`、`user_message.reply_to`、排队条目的 `reply_to`。

**第 3 组：M1b 才做也不违反规则**（界面、内部实现或流水线，不涉及协议形状）

- 审查文档及其与主文档之间的消息协议（[5.7.5](07-desktop.md#审查文档m1b)，不属于宿主协议）；
- git 的仓库档写操作和用户档（快照档在 M1a）；
- Codex 的提取层、选项对照表和"未信任项目上拒绝启动"（reason 的取值已在第 2 组）；
- 安装空窗、安装位置检查、签名后冒烟、许可证门禁第二层；
- ShellAPI 的 `pathForFile`（新增成员）。

**冻结机制**

- M1a 协议 crate 合并时，把 `protocol/schema/v1.json` 复制成 `protocol/released/v1-m1a.json`；`tools/schema-compat` 对它做方向敏感的比对，M1b 期间出现破坏性改动，CI 的 `protocol` 作业直接失败。
- 同时为 `@cupper/shell-api` 的类型声明生成一份快照（`packages/shell-api/api-m1a.d.ts`），CI 比对：只允许新增成员和联合类型的新分支。
- 冻结之后确实需要破坏性改动时，按 ADR-04 升主版本，不在 v1 里改。
- **已冻结（T016，2026-10-26）**：`ci protocol` 多了三步：`schema-compat` 对 `v1-m1a.json` 比对；`integers` 检查导出文件里的整数都有界、不超过 ±(2^53−1)、不带 `format`；`shell-api-m1a` 由 `packages/shell-api/src/frozen.ts` 核对冻结成员还在、旧的调用仍能通过类型检查、冻结联合的旧分支都还在。
  - **判为附加**：加方法、加可选字段、开放枚举加取值、开放联合加分支（含新增一种事件、视图里新增成员）。
  - **判为破坏**：其余的改动。
  - **方向**：由引用推出；没有被引用到的定义按两个方向从严判。
  - **不比较**：描述、数值界的放宽、`x-errors` 的增减、`x-milestone`。
