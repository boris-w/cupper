# 5.13 待定问题与 ADR 修订建议

- 所属：`docs/02-design.md` 第 5 节
- 本设计只细化 ADR，没有推翻任何一项。下面列出设计阶段定不下来、需要在实现中验证的事项，以及 ADR 之间或 ADR 与实测之间的不一致，每项附建议
- 状态更新依据：设计评审会纪要（`docs/meetings/2026-10-16-design-review/minutes.md`）；用户在 `gate/design` 前的决定（`docs/meetings/2026-10-16-gate-design-decisions/minutes.md`，2026-10-16）

## 5.13.1 待定问题

**已关闭**

| # | 问题 | 结论 | 依据 |
|---|---|---|---|
| Q1 | ADR-03 写"主进程是唯一连接，所有窗口共用"，但主进程不解析入方向的 JSON，无法分发事件 | control 连接 + 每个窗口一条数据连接；补数据连接的生命周期、`initialize` 只接受一次、测量闸门前移到 M1a（[5.7.1](07-desktop.md#571-进程与连接)） | 纪要 D1 |
| Q3 | 重签后只带 `allow-jit` 的 Node 能否公证、首次执行多慢 | 直接采用后备方案：Node 作为普通 Mach-O 放进 `CupperHost.app/Contents/Helpers/`，随 App 一起公证（[5.10.1](10-release.md#5101-包结构)）；首次执行的耗时在第一次有 Developer ID 的构建上人工记录 | 已定（2026-10-16，U-R3） |
| Q4 | codex-acp 随包还是下载 | 随包；CI 按 SRI 校验、清单记录 sha-256、附内联依赖的许可证声明、每晚 `runtime-pack --verify`、test-harness 里 registry 地址可注入 | 纪要 D2 |
| Q5 | bundle identifier 的前缀 | `dev.cupper`。M1b 首次签名发布前可以无成本修改，之后不再改（[5.7.9](07-desktop.md#579-构建种类)） | 已定（2026-10-16） |
| Q6 | 只出 arm64 还是同时出 x64 | 只出 arm64；以后需要时加一个构建目标，不涉及兼容代码 | 已定（2026-10-16） |
| Q11 | ADR-06 写"Codex 有 `threadStatus` 时优先采用"，但 SP-1 s05 中计划确认请求发出时线程状态已是 `idle` | 有 prompt 在途时以 prompt 的响应为准；`threadStatus` 只用于显示状态和界定自发回合，变成 idle 不会让挂着的交互请求失效；界面只看 `SessionStatus`，"需要处理"只由 cupper 自己的待处理请求决定 | 纪要 D3 |
| Q12 | Claude 在 SDK 会话里拿不到 TodoWrite 的计划，haiku 上也没有思考流 | 兼容矩阵用 sonnet 或 opus 补测；仍然拿不到时，MS-4 对 Claude 按"计划模式的 Markdown 计划"验收，记录在 PRD v1.2 第 10 节 | 已定（2026-10-16） |
| Q13 | GT-3 要求取消暂存，必须能看到已暂存的内容，而 PRD 把 F2 放在 P1 | M1 提供最小的 `staged` 范围，只服务于取消暂存，记录在 PRD v1.2 第 10 节 | 已定（2026-10-16） |
| Q14 | ADR-07 写"丢弃晚到的工具输出"，完全丢弃会让用户不知道命令在取消后还在运行 | 回合快照不可变；取消时标 `may_still_run`；晚到的输出写成当前回合里的 `late_tool_output` 条目，原条目不改（[5.2.6](02-host.md#晚到的工具输出)） | 纪要 D4 |
| Q15 | SQLite 用 `synchronous=NORMAL` 还是 `FULL` | 按文档决定：`NORMAL` 加 `checkpoint_fullfsync`；`pending_external` 那一批用 `FULL` 加 `fullfsync`；引入 `data_epoch` 处理断电后 seq 被重用（[5.2.8](02-host.md#528-事件存储)） | 纪要 D5 |
| Q22 | 退化认证模式下，UI 崩溃重启后连不上旧宿主 | 接受，附两个条件：退化模式只由编译特性决定，不允许运行时回退；自行构建并日常使用的版本是"自用构建"，有独立的身份和数据目录（v1.3，[5.8.1](08-security.md#581-认证握手固定令牌)、[5.7.9](07-desktop.md#579-构建种类)）。NFR-8 的口径记录在 PRD v1.2 第 10 节 | 已定（2026-10-16） |
| Q9 | ACP Rust SDK 3.3 是否支持发送 `_session/steering` 这类扩展请求、能否收到 `_auth/status_update` 和 `$/cancel_request` | 三项都由 SDK 3.3.0 直接支持；线路分接层不再处理扩展消息，只负责编号与原始帧、结构化脱敏、K19 的容错改写；SDK 的两个坑写成驱动层的约束（K27、K28，[5.6.10](06-agent-driver.md#5610-线路分接层)） | T003（v1.6） |

**仍待定**

| # | 问题 | 涉及 | 影响 | 建议 | 何时、怎样定 |
|---|---|---|---|---|---|
| Q2 | ADR-01 规定系统通知全部由宿主 helper 弹出，但 helper 的通知身份要等 SP-10 才验证，而 AP-3 是 M1b 的 P0 | ADR-01、SP-10、AP-3 | M1b 能否交付通知 | 把 SP-10 中"通知身份"一项提前到 M1b 开始前做（需要用户在场）；不通过就启用主进程的 `notifications` 后备模块（[5.7.2](07-desktop.md#572-主进程模块)）：宿主照常写 `notification.intent`，由 UI 主进程弹，通知上不放"批准"按钮 | M1b 开始前 |
| Q7 | Claude 的 `allow-with-updates` 在文件编辑上只对本会话有效，在 Bash 上写持久规则，option_id 相同 | SP-1、AP-2 | 持久选项的标注是否准确 | 按工具区分，判断不出来时按持久处理；向 claude-agent-acp 上游反馈，建议用不同的 ID，并在 `_meta` 里透出规则和写入位置（[5.3.6](03-protocol-v1.md#536-交互请求模型)） | 实现时；上游 |
| Q8 | 同一个适配器上多个会话**并发** prompt 是否可靠（ADR-09 的前提） | ADR-09 | 多会话并行（S1、NFR-11） | **Claude 部分已关闭**（v1.6，T003）：在 claude-agent-acp 0.91.0 上 3 轮全部通过，候选 0.93.0 的 1 轮也通过；一个会话的审批挂起 15 秒时，另一个会话照常运行（上游 #830 没有复现）。ADR-09 的前提对 Claude 成立，后备 T057 不派发。T003 只测了 2 个会话和 haiku 的短回合，6 个会话的真实场景放进 T066 的兼容运行（[5.9.5](09-testing.md#595-兼容矩阵)）。**Codex 部分仍待定**：通过条件见 5.9.5，在 M1b 写 Codex 驱动之前由 T102 测完（U-R5 已定）；不行的后备是 Codex 的 prompt 全局串行，或只对 Codex 退回每个会话一个适配器进程 | M1b Codex 之前 |
| Q10 | Claude 没有运行状态信号，自发回合何时结束只能推断 | ADR-06 | 状态显示；出队过早会与 Claude 的自发工作并发（K1） | 以带 `cost` 的 `usage_update`，或 10 秒无更新且没有运行中的工具判定结束；由静默判定结束的，再静默 30 秒才出队（[5.2.6](02-host.md#回合状态机)）。阈值经 `Clock`，用录制、假 agent 的 `autonomous` 和兼容矩阵验证 | M1a |
| Q16 | 保留参数：会话视图窗口 20 个回合、日志保留 1 万条或 7 天、原始帧 14 天且不超过 512 MB | ADR-06 | 内存、磁盘、断线续传的覆盖范围 | 先按这些值实现，每个值都能在 test-harness 里覆盖，补边界的性质测试（例如游标正好落在保留边界上）。宿主在本地日志里记录每次订阅的模式（续传还是快照）和游标落后多少，M1b 据此调整 | M1b |
| Q17 | git 快照对象的回收 | ADR-06、ADR-10 | 磁盘占用；快照可能含密钥 | M1 只累积并显示大小；M1b 的设置页提供"清除快照对象"；M2 按"重新打包仍需要的 tree"回收（[5.4.5](04-data-model.md#545-保留与回收)） | M2 |
| Q18 | 从登录 shell 取环境：会执行用户的 rc 文件，可能很慢、可能有输出 | ADR-08、ADR-10 | 找不到 CLI、git、git-lfs、配置目录 | 只在宿主启动和用户刷新时执行，5 秒超时，用标记截取输出；失败时退回常见路径；取到的 `CLAUDE_CONFIG_DIR`、`CODEX_HOME` 校验后才使用（[5.6.6](06-agent-driver.md#566-cli-的发现与版本检查)） | M1a |
| Q19 | npm、Homebrew、原生安装器等安装方式下，CLI 原生二进制的位置各不相同 | ADR-08、SP-2 遗留 | CLI 发现 | 数据驱动的解析器，用目录树样例测试，实现时逐个核实；只有 JS 入口的安装提示改装原生版本 | M1a（Claude）、M1b（Codex） |
| Q20 | electron-updater 7 的迁移；ShipIt 的替换方式；UI 进程的本地代理端口 | ADR-01、ADR-03 | 升级安全 | 升级演练确认 ShipIt 是"移入新包、移走旧包"，并加两项负面测试；代理端口从下载完成一直开到 `quitAndInstall`，写进安全说明；迁移到 7.x 时配置改为 `manual`（[5.10.5](10-release.md#5105-更新流程)） | M1b 发布任务 |
| Q21 | TCC 的"负责进程"：宿主由 UI 拉起时可能算在 UI 上；agent 和它的后代触发的授权提示显示成 Cupper，授权也记在 Cupper 名下 | ADR-01、SP-10、SEC-8 | 系统授权的归属；agent 借 Cupper 的辅助功能权限点掉确认框 | SP-10 一并评估给适配器做 disclaim；cupper 永远不申请辅助功能、自动化、屏幕录制权限，Info.plist 不写用途说明；在 disclaim 落地前，安全边界说明必须在 M1b 发布前写明"不要因为 agent 的操作授予 Cupper 辅助功能权限"（[5.8.7](08-security.md#587-安全边界说明)）。仍有问题时按 ADR-01 的后备方案（M1 就用 LaunchAgent），或改用 LaunchServices 拉起 helper | SP-10（M2 前，通知部分提前到 M1b 前） |
| Q23 | 宿主环境下的 git 凭据：SSH agent、GPG 签名提交（终端 pinentry 没有 TTY 会失败）、credential helper | ADR-10 | GT-4 推送和提交（用户档） | 实现时逐项验证；失败时错误信息给出指引（例如改用图形版 pinentry） | M1b |
| Q24 | SP-4b 只在 macOS 26.4 上验证了对端校验的语义 | ADR-05 | 最低支持的 macOS 14、15 | macOS 15 由每个 PR 的 `rust` 作业覆盖；macOS 14 每晚跑，改动 `cupper-sys` 的 PR 按路径过滤直接跑；发布前最近一次 macOS 14 的校验必须通过（[5.1.6](01-repo-build-ci.md#516-ci-流水线)）。GitHub 下线 `macos-14` 镜像时要找替代（下线时间未核实） | M1a 起 |
| Q25 | Codex 的 `revise_plan` 是否结束回合、Codex 权限模式的完整取值和档位 | SP-1 | 选项标注、档位的识别 | 兼容矩阵补测；CI 检查录制里出现的选项和模式取值都在对照表和档位表里；在此之前不认识的取值一律按 `full_access` 处理 | M1b |
| Q26 | Electron 44 上的输入事件：Playwright 的键盘输入是否仍不触发 `before-input-event`；`sendInputEvent` 是否触发；`input-event` 的键盘事件在运行时是否带键码（Electron 34 上带，类型定义里没有）；`page.click()` 是否产生 `input-event`；用 `webContents.focusedFrame` 判断主框架是否可靠 | ADR-03 | 真实输入规则的实现和 e2e 的测法（[5.7.8](07-desktop.md#578-安全加固的落点)） | **已关闭**（T077，2026-10-20，Electron 44.9.0，脚本与结果在 `docs/tasks/T077-desktop-input-privileged/q26/`）：① Playwright 的键盘输入不触发 `before-input-event`，只产生 `input-event` 的 keyDown、keyUp（页面的 keydown 是 `isTrusted`），所以不会被记成真实输入；② `sendInputEvent` 触发 `before-input-event`，页面的 keydown 是 `isTrusted`，e2e 的按键照 5.7.8 的测法经它发出；③ `input-event` 的键盘事件运行时带 `key`、`code`、`isComposing`、`isAutoRepeat`、修饰键（类型定义里没有），留作后备；④ `page.click()` 产生同一按钮的 `mouseDown`、`mouseUp`；⑤ `focusedFrame` 可靠，按 `frameTreeNodeId` 与 `mainFrame` 比较。实体键盘的确认见 MAN-keyboard（待作者） | M1a |
| Q27 | 整个 App 的外发检查怎样做：整个 App 放进 Seatbelt 会让 Chromium 的子进程崩溃（测试工程师实测） | SEC-6、NFR-4 | 验收方法 | **已关闭**（T176，2026-10-20）：`nettop` 对整棵进程树采样可行，加正对照证明看得到树的深处；局限是快照采样会漏掉比采样周期短的连接，只能证明"没有持续的外发"。做法与局限写在 5.9.4 的 E2E-SEC-egress 一行 | M1a |
| Q28 | Claude Code 在会话进行中是否热加载项目配置（hooks 等） | 项目信任 | 信任之后配置被改写时的残余风险有多大 | 兼容矩阵加一个剧本：回合中改写 `.claude/settings.json` 的 hooks，看当前回合内是否生效；结果写进安全边界说明（[5.8.7](08-security.md#587-安全边界说明)） | M1a 兼容套件 |
| Q29 | 审查文档（独立源的 iframe 加原样的 @pierre/diffs）是否可行：constructable stylesheet 是否受 CSP 约束、Worker 和模块脚本在跨源 iframe 里能否加载、桌面确认的 8 点（[5.7.5](07-desktop.md#审查文档m1b)）、5,000 行 diff 的性能 | ADR-03、GT-1 至 GT-3 | 审查面板的实现方式 | 与 SP-6 的场景 9 一起验证；不通过就直接改用 `DiffHunksRenderer` 的后备方案 | M1b 开头 |
| Q30 | 兼容套件改用隔离配置目录前要确认：OAuth 刷新令牌会不会轮换（会的话，在副本里刷新可能让用户原来的登录失效）；Claude 的凭据存在钥匙串里时，隔离 `CLAUDE_CONFIG_DIR` 后是否仍然读得到 | U-R4、测试 #8 | 兼容套件的运行环境 | 确认之前用真实配置加前后哈希对比（U-R4 已定）；确认刷新令牌不轮换后，改用隔离目录（每个用户自己的临时目录下、0700、用完即删），报告里记下运行模式 | 改用隔离模式之前 |
| Q31 | codex-acp 是否经 `_meta` 透出 `agentMessage.phase` | 桌面 #8 | 最终回答的判定 | 透出就以它为准，否则用宿主的规则（[5.3.5](03-protocol-v1.md#535-事件类型目录)） | M1b Codex 提取层 |
| Q32 | 仓库档里用 `-c <键>=` 置空覆盖可执行类键，对每个键是否都有效；快照档里 LFS 的标准取值是否工作 | 安全 #1、纪要 D8 | 宿主自动调用 git 的安全 | D02 的实验已确认：置空的过滤器按原样处理内容，设了 `required` 时报错（按失败处理，不重试）；其余键（`diff.*.textconv`、`merge.*.driver` 等）用"恶意配置仓库"样例逐键确认；本机没有 git-lfs，LFS 路径在 M1b 实测（`docs/tasks/D02-design-revision/notes.md`） | M1a（快照档）、M1b（仓库档写操作、LFS） |
| Q33 | 测量闸门不达标时的后备方案"utilityProcess 宿主桥 + 委托票据" | ADR-03、ADR-05 | 主进程的连接方案 | 草案：主进程经 control 连接调用 `auth/delegate {role: window, ttl}` 取得一次性票据，经 MessagePort 交给 utilityProcess；utilityProcess 连接时以 `auth: ticket` 出示，宿主核对票据后把这条连接的固定令牌绑定到它，principal 仍是 `desktop_ui` 的 window 角色；5.8.1 的签名要求表加一行"helper 只能凭票据接入"。只在闸门不达标时启用，届时在契约冻结前补进协议 | M1a（闸门不达标时） |
| Q34 | Claude Code 和 Codex 会从哪些目录加载项目配置：是否按层级读取上级目录的 `.claude/`、`.mcp.json`、`.codex/`，是否会越过 git 根目录 | 项目信任（安全核对 1） | 信任扫描的范围是否够 | 在核实之前，扫描范围取"工作区到 git 根目录的每一级"（[5.2.6](02-host.md#项目信任)）；兼容矩阵加剧本：在上级目录和 git 根目录之外各放一份带 hook 的配置，看哪些会生效，据此调整扫描范围 | M1a 兼容套件（Claude）、M1b（Codex） |
| Q35 | 用户暂不开通 Apple Developer Program 账号，作者自用的构建没有 Developer ID：按现设计只能走退化认证：NFR-8 的"界面崩溃不影响会话"不成立，UI 只能用 `dev_key` 的双向证明校验宿主。（v1.3 更正：原文写的"cupper-cli 连不上宿主"有误。按 5.8.2，CLI 按 uid 只读接入，写操作经 UI 的原生确认提权，退化模式下照常工作；CLI 怎样校验宿主见 5.8.1。）Q35 通过后的收益：NFR-8 成立，UI 和 CLI 都按签名身份校验宿主 | ADR-05、5.8.1、5.7.9、NFR-8 | 作者日常使用的体验和本地安全 | 验证"本机签名身份"：钥匙串里自建的代码签名证书，或免费 Apple ID 在 Xcode 中得到的 Apple Development 证书。签名要求在本机构建时写入（证书摘要或 Team ID 只存在本机构建配置里，不进仓库）；评估 hardened runtime 与库校验在这种签名下的表现。通过就新增"本机签名构建"种类，并补设计和 ADR-05；不通过就维持退化模式 | M1b 技术验证（需要用户在场：会在钥匙串里建证书或登录 Xcode） |
| Q36 | Codex 左栏默认 340 是否包含导航栏：ui-spec 由截图推断"导航栏约 52 + 侧边栏约 238"，"包含"属于推测 | F1、F3（导航栏不做）、[5.7.4](07-desktop.md#574-窗口与布局) | cupper 侧边栏的默认宽度（现取 288 = 340 − 52） | T106 静态核对本机包里左栏宽度的计算方式；不包含时改为 340，范围随之调整 | T106（M1a 期间） |
| Q37 | 暗色主题的取值只有按本机包规则推算的值，没有截图核对；暗色下输入框底色是 #2D2D2D 还是 #363636，两条样式规则的生效条件没有核清；半透明侧边栏在暗色下的实际观感未实测 | F1、UI-3、[5.7.10](07-desktop.md#5710-设计参数) | T084 的暗色 token、T152 的暗色主题、截图基线 | 作者补截一张暗色主界面（第 6 节的补截清单），T106 逐项核对并给出修正值，编排者更新 5.7.10 | T106；T152 之前 |
| Q38 | 会话行尾的相对时间和 diff 统计（如"4d""+1 −9"）只在旧公开截图里见过，本机包里没有核实 | F1、[5.7.4](07-desktop.md#574-窗口与布局) 侧边栏 | 侧边栏行的布局；diff 统计需要宿主在 `SessionSummary` 里多给数据 | 核实前不显示；确认存在时再评估数据来源（回合快照的 `stat`），需要协议字段的按附加式加入 | T106 |
| Q39 | 输入框为空时按 ↑ 恢复上一条：官方快捷键表有，本机包的输入框按键表里没找到 | CP-1、UI-4 | T145 的 ↑ 恢复 | PRD CP-1 要求，照做；T106 实机或静态核对 Codex 是否有，结论只影响文案和细节 | T106；T145 之前 |
| Q40 | ui-spec 第 7.3 节其余未核实的细节：回到底部按钮只有截图实测约 28；斜杠命令的完整列表来自官方文档（cupper 的命令来自 agent，只核对菜单外观）；分栏和统一 diff 自动切换的像素阈值；运行中输入框有内容时发送按钮的形态 | F1、[5.7.4](07-desktop.md#574-窗口与布局)、[5.7.5](07-desktop.md#575-组件树) | 对应界面任务的细节 | T106 一并核对；核对前按 5.7.4 的写法实现 | T106 |
| Q41 | 界面复刻 Codex 后与安全设计有关的几处。安全工程师已确认"附条件可以"（2026-10-19，`docs/meetings/2026-10-19-ui-fidelity/security-engineer.md`），数字键直接提交被接受，两处有意保留的差异（通知不放"批准"、还原不提供"不再询问"）理由成立；附加的 6 条已在 v1.5 写进 5.7.8（RP-3、RP-6、RP-11、RP-12，第 2、7 条）。剩下一项未定：Codex 的数字键是否"选中即提交"，ui-spec 没有核实，设计里原写的"与 Codex 相同"已改为"未核实" | F6、AP-1、AP-5、AP-3、GT-3、ADR-03、[5.7.8](07-desktop.md#578-安全加固的落点) | 只影响复刻度：Codex 不是"选中即提交"时，是否跟着改由编排者决定，安全上两种都可以 | T106 静态核对本机包；结论交编排者 | T106 |
| Q42 | claude-agent-acp 0.93.0 新增的 `/remote-control` 命令：一条只含这条命令的 prompt，会把会话交给 Claude 的网页版或手机 App 继续操作，等于在宿主之外多开一条控制通道，审批和审计都绕过 cupper | AG-1、SEC-8、ADR-07 | 升级适配器的时机 | M1a 保持 0.91.0（没有这条命令）。升级到 0.93 及以上之前：交安全工程师评审；宿主拦截这条命令（发送前识别并拒绝），并把它从发给客户端的命令列表里去掉；兼容套件加一个剧本断言它被拦下 | 升级 claude-agent-acp 之前（T066 的兼容运行只把 0.93 作为候选版本） |
| Q43 | 上游 #1011（未关闭，修复 PR #1220 未合并）：长期运行的适配器反复 resume 同一个会话后，会累积孤儿 `claude` 子进程 | ADR-09、K6 | 内存与进程数；K6 的"下次发送时自动 resume"加上共享适配器正好会触发 | 资源采样记录每种 agent 的 `claude` 进程数，超过会话数时写日志（[5.6.8](06-agent-driver.md#568-资源回收)）；T063 用假 agent 断言多次 resume 后宿主侧不泄漏进程，T066 用真实 CLI 观察进程数；问题确认后，按适配器版本决定是否在 resume 前先 `session/close` | T063、T066 |

## 5.13.2 对 ADR 的修订建议

全部是细化或补充，没有推翻任何一项。编排者据此修订 ADR，随 `gate/design` 一起交用户确认。v1.4 新增 ADR-03、ADR-07 两行，来自界面复刻 Codex 的用户决定（`docs/meetings/2026-10-19-ui-fidelity/minutes.md`）。

| ADR | 建议修订 | 出处 |
|---|---|---|
| ADR-01 | 运行时载荷放在 `CupperHost.app/Contents/Resources/runtime/`，Node 作为普通 Mach-O 放在 `CupperHost.app/Contents/Helpers/` 随 App 公证（U-R3）；"运行中不读包内资源"的唯一例外是自检通过后、按内嵌清单逐文件核对哈希的复制；UI 经原生扩展用 `cupper-sys::spawn`（SETSID、`CLOEXEC_DEFAULT`）拉起宿主，启动载荷经 stdin；Electron 的 userData 设到数据目录下的 `ui/`；宿主退出码约定（3、4、5、6）；套接字路径过长时回退到 `DARWIN_USER_TEMP_DIR`；通知点击经 `ui/navigate` 并按 bundle id 激活 App，通知的后备方案（Q2）；更新时的安装空窗与安装位置检查；TCC 归属的范围扩大到 agent 的全部后代（Q21） | 5.2.12、5.2.13、5.4.1、5.10 |
| ADR-02 | crate 表补充：`cupper-sys`（集中全部 `unsafe`）、`cupper-desktop-native`（napi-rs）、`xtask`；协议的 `Method` trait 声明 `SCOPE` 和 `IDEMPOTENCY`；写进 `CLAUDE.md` 的约束加两条：**时间只经 `Clock`、ID 只经 `IdGen`，有状态的决策写成纯核心**；子进程一律经 `cupper-sys::spawn` | 5.1.3、5.1.7、5.2.16 |
| ADR-03 | 连接模型改为 control 连接 + 每窗口数据连接，数据连接绑定到文档实例（Q1）；主进程转发的测量前移到 M1a，判定负载按订阅上限定义（纪要 D1）；打包工具定为 electron-builder + electron-updater；Electron 44.8.0 及跟进策略；Vite 构建主进程、单文件 CommonJS 的 preload、主文档和审查文档两个渲染入口；**Trusted Types 的落法**：主文档禁止 `default` 策略，只有具名策略 `cupper-worker`，放行 `style-src 'unsafe-inline'` 但 agent 的字符串不能进入样式；审查面板的 diff 视图放进独立源的 iframe（纪要 D6）；**真实输入的细则**（纪要 D7）：`before-input-event` 识别键盘、按键白名单、打字中的按键不算、任何出方向命令都会用掉输入、只记录主框架、窗口激活后的冷却、`requestPrivileged` 也要求真实输入并逐次加长冷却；宿主侧 400 ms 最短回答间隔并要求 `input` 证据；e2e 构建的宿主开 `dev-auth` 和 `test-harness`，ad-hoc 签名且关闭 hardened runtime 和 cookie 加密；三种构建的 URL scheme 不同 | 5.7、5.10 |
| ADR-04 | 增加 `host/hello`（nonce）；`initialize` 每条连接只接受一次，增加 `nonce`、`role` 和绑定信道的退化模式证明；命令为"受理"语义，结果与它产生的事件同事务落盘；账本按 principal 稳定 ID；`session/create` 必须带第一条消息，且这条消息随受理事务落盘；`history/page` 读回合快照，游标细到回合内的位置；`blob/put` 先发分片再发请求；`item.delta` 带 `field`；hunk 的前置条件具体化为 `diff_token`；错误码增加 `git_error` 并给出 reason 清单；引入 `data_epoch`；规范 JSON 用 RFC 8785，摘要保留完整的 256 位；要批准的内容不截断；**v1 契约在 M1a 冻结**，冻结清单见 5.12.5；方法清单以 5.3.4 为准 | 5.3、5.12.5 |
| ADR-05 | principal 增加角色（control、window）和 `privileged`、`ui_local`、`test` 三个 scope，给出完整的权限矩阵；**审批档位**纳入 scope 规则：放宽到 `elevated` 需要 approve，`full_access` 只能经原生确认，会话的默认档位视为由创建者设定，不符时进入 `config_unconfirmed`（纪要 D10）；**项目信任**作为 SEC-8 之外的又一道边界（U-R1）：扫描覆盖工作区到 git 根目录的各级目录并跟随符号链接，摘要按链接目标的内容计算，工作区外的目标醒目标出（安全核对 1，Q34）；回合进行中发现档位越权时，先改回原值，改不回就中断当前回合，再进入 `config_unconfirmed`（安全核对 4）；提权的全局限频、被拒后的静默期、确认框区分核实信息与申请方自述；血缘检查改为只认适配器的会话（适配器用 SETSID 拉起）、进程组和祖先链；传给适配器的环境变量增加禁用清单，`CLAUDE_CONFIG_DIR`、`CODEX_HOME` 由宿主校验后设置（纪要 D14）；UI 校验宿主时之后每次读取都比对令牌；**退化模式改为经 stdin 的启动载荷和绑定信道的双向证明**（纪要 D11），局限与条件见 Q22；发布版 UI 和 CLI 拒绝 `weak_auth` 的宿主；写明"授予 Cupper 的系统权限会被 agent 继承" | 5.8 |
| ADR-06 | 增加回合快照表和会话视图窗口（最近 20 个回合），历史分页从回合快照读取；**回合快照写入后不可变**，事后的信息写成新事件；blob 改为按内容寻址的文件；原始帧放在单独的 `raw.db`，**只做结构化脱敏**（替换"脱敏要求和日志相同"）；工具调用 ID 细化为"`toolCallId` 是 upsert 键，协议里用宿主生成的 ID"；`threadStatus` 的使用范围（Q11）；索引流不含已归档会话，另加 `session/list_archived`；交互请求表记录所属的适配器实例（SEC-10）；**持久性定案**（替换"M1 实测后再定"）：`NORMAL` 加 `checkpoint_fullfsync`，`pending_external` 用 `FULL` 加 `fullfsync`，并引入 `data_epoch`（Q15） | 5.2.8、5.4 |
| ADR-07 | **晚到的工具输出**由"丢弃"改为"不进入新回合的工具流程，写成当前回合里单独的 `late_tool_output` 条目"，取消时标 `may_still_run`（Q14）；加入线路分接层处理原始帧、封闭枚举和扩展消息，未知的选项 kind 按最严处理；上一个 prompt 一直不返回时，先 `session/close` 再 `session/resume` 才发新 prompt；规避清单以 5.6.9 为准 | 5.6、5.2.6 |
| ADR-08 | codex-acp 随包（Q4 及附带条件）；清单在编译期内嵌进宿主二进制，覆盖所有会被执行或加载的文件；已验证的 CLI 范围由兼容报告生成并写进清单；**Node 随 App 一起公证**，放在 `Helpers/` 里，安装时复制并核对 cdhash（U-R3，替换"Node xz 压缩后约 24 MB"和"首次运行时复制 tar.xz"的说法）；未信任的项目经 `_meta.claudeCode.options` 传 `settingSources: ["user"]`；`runtime-pack` 检查适配器读取的环境变量；CLI 发现只在 PATH 的可信条目里查找（不在任何项目、工作区、临时目录下的绝对路径，安全核对 2） | 5.2.12、5.6.6、5.10.3 |
| ADR-09 | 多会话并发 prompt 的实测时机（Claude 在 M1a 驱动层之前，Codex 在 M1b 之前）、通过条件与后备（Q8）；Codex 回收阈值（常驻内存 2 GiB 或空闲 30 分钟）；适配器崩溃后的重启退避；能力探测前重建探测目录，探测后回收 Codex 适配器；适配器以 SETSID 拉起 | 5.6.7、5.6.8、5.9.5 |
| ADR-10 | **宿主自动调用 git 分三档**（纪要 D8）：快照档用 cupper 自己的 GIT_DIR，配置只放行白名单（D02 评估后采用）；仓库档每次调用前读配置并覆盖可执行类键、配置来源在工作区内时拒绝、调用后比对指纹、覆盖后报错不重试；只有用户点"提交""推送"时运行仓库的 hooks；仓库档的配置来源检查对仓库自己的 git 目录例外，由覆盖和指纹兜底（安全核对 5）；仓库可执行配置的指纹；复制 index 时保留 mtime；快照档复制 `info/attributes`，自定义过滤器的检测覆盖全部属性来源，这类路径在快照里只存占位，`git/stage` 前再核对一次（安全核对 3）；**git 和 git-lfs 在宿主启动时解析成绝对路径**，只从 PATH 的可信条目里查找，子进程的 PATH 只留可信条目、工作目录设为 cupper 自己的目录，快照档的 LFS 过滤器写成绝对路径（安全核对 2）；hunk 操作用 `diff_token` 指向宿主缓存的补丁；M1 提供最小的 `staged` 范围（Q13）；按仓库加锁的范围；git 子进程的环境变量约定 | 5.2.10 |
| ADR-11 | 门禁工具具体化：`cargo deny`、cargo-about、`tools/license-gate`；**门禁分两层**：对每个 JS 入口求生产依赖闭包，对最终产物按内容指纹扫描、锁文件按哈希放行，加负面测试（替换"打包输入中发现 SDK 就失败"的写法）；CI 计算 claude-agent-acp 打包哈希时用隔离目录，不进入任何打包输入 | 5.1.6、5.10.7 |
| ADR-03（v1.4） | "审批卡片出现后约 500 ms 的输入保护；Enter 只在焦点位于卡片上时生效"改为请求面板的规则：面板替换输入框的位置，500 ms 待命期覆盖按键和点击，焦点规则，真实输入的白名单增加数字键 1–9（宿主核对与选项序号相符），↑ ↓ ← → 作为导航键不算打字键（Q41） | 5.7.4、5.7.8 |
| ADR-07（v1.4） | 第 4 条"默认行为是排队"改为：宿主只在客户端明确要求时 steer；桌面端默认插话、⌘Enter 排队，设置里可以改为默认排队，Claude 的副作用在设置说明和第一次插话时提示（PRD CP-6 v1.3，用户决定 F5） | 5.3.7、5.6.9 的 K10、5.7.4 |

## 5.13.3 对 PRD 的影响

已在 PRD v1.2 落实（编排者在 main 上修订，D02 分支不改 PRD）：

- **PS-2** 增加项目信任：添加项目时确认是否信任项目自带的 agent 配置（U-R1，[5.2.6](02-host.md#项目信任)）。
- **SEC-9** 增加"不能切到更宽松的审批档位"（U-R2，[5.8.2](08-security.md#582-principal角色与-scope)）。
- **第 10 节**记录验收口径：MS-4 对 Claude 可能只能显示计划模式的 Markdown 计划（Q12）；GT-3 的取消暂存依赖一个最小的"已暂存"范围（Q13）；NFR-8 的"界面崩溃不影响会话"只对发布构建成立（Q22）。

设计层面细化、不需要改 PRD 文字的口径：

- UI-5 在 M1a 只做一种语言，但全部文案从第一天起走 i18n 的键；M1b 补齐中英文。
- M1a 的退出标准"NFR-1 的验收项全部通过"细化为连续 20 次全绿等条件；M1b 的"作者能用它完成 S1–S4"细化为逐步检查表（[5.12](12-milestones.md#5122-m1a宿主骨架)）。

v1.4（界面复刻 Codex）对 PRD 的影响：

- AP-1、AP-5、GT-2、CP-6、UI-4、UI-6 已在 PRD v1.3 改写，设计 v1.4 逐条对应（5.7.4、5.7.5、5.7.7、5.7.8、5.12.4）。
- **CP-8**：按 F3，排队消息的预览、编辑和插话提前到 M1，归入 CP-6；CP-8（P1）只剩重排和删除（编排者已在 PRD v1.3 改写，`aa36b41`）。设计对应：M1b 实现 `session/queue_edit` 的 `replace` 和 `steer`，`remove`、`move` 仍属于 CP-8。`session/queue_edit` 在 v1 的类型里本来就有，这次只是定义 `ops` 的形状并把实现提前，属于附加，而且在 T016 冻结之前写进契约（5.12.5 第 2 组）。
- **Codex 有、PRD 没有对应需求的元素**，设计的处理：
  - 回合改动卡片上的"撤销"：映射到 GT-3 的还原，作用于本回合的改动（M1b）；
  - 审批面板的自由输入：做成"拒绝并附一条消息"（AP-1 加 CP-6），没有拒绝类选项时不显示；
  - 审查面板的选项菜单：换行、diff 视图、展开收起全部、刷新进 M1，其余是 P1；
  - agent 选择器：新建会话页的动作条最左边（AG-3）；
  - 推荐徽标：驱动层拿不到，不显示；
  - ⌘.：保留为中断的别名（UI-4，cupper 的别名单列）。
