# R01 资料清单

本文件登记 `report.md` 引用的全部资料：编号、链接、日期、获取方式，以及每条资料支撑了哪些结论。报告正文用方括号编号引用（如 [D10]）。

- 采集日期：2026-10-16
- 摘录方式：以中文转述为主。只有界面文字、命令名、协议方法名等标识符保留英文原文。第三方图片均未下载，只给链接。

## 时效说明

- 官方文档已从 `developers.openai.com/codex/...` 迁到 `learn.chatgpt.com/docs/...`，旧地址 308 重定向到新地址。文档页本身不标注日期，本报告按采集日 2026-10-16 的内容记录。功能上线时间以 changelog 和 What's new 中的日期为准。
- 采集时的最新版本：桌面 App 为 26.1007（2026-10-08 发布），Codex CLI 为 0.163.0（2026-10-15 发布）。
- 许多功能按套餐、地区、灰度和工作区设置开放，文档里多处注明"可用性取决于 rollout"。报告描述的是功能全开时的形态。

## 一、官方文档（learn.chatgpt.com，2026-10-16 采集）

所有页面都有 Markdown 版本，在 URL 后加 `.md` 即可获取。完整导出见 <https://learn.chatgpt.com/docs/llms-full.txt>。

| 编号 | 页面 | 链接 | 支撑的内容 |
|---|---|---|---|
| D1 | ChatGPT desktop app | <https://learn.chatgpt.com/docs/app> | 桌面 App 定位、支持的平台、上手步骤（选 ChatGPT 或 Codex，New chat、Quick chat） |
| D2 | Quickstart | <https://learn.chatgpt.com/docs/quickstart> | 支持 macOS、Windows、Linux；可用 ChatGPT 账号或 API key 登录；从 ChatGPT 下拉菜单选 Codex |
| D3 | Use ChatGPT | <https://learn.chatgpt.com/docs/use-chatgpt> | Chat / Work / Codex 三种模式的区别；桌面端 Work 与 Codex 的对比表；分享 Codex 会话的只读快照 |
| D4 | Projects and chats | <https://learn.chatgpt.com/docs/projects> | Projects 视图；本地项目和多文件夹（主文件夹）；置顶、重命名、搜索、归档；无项目会话；Quick chat |
| D5 | Commands | <https://learn.chatgpt.com/docs/reference/commands> | 三个平台的全部默认快捷键；`codex://` 深链接；会话搜索与会话内查找 |
| D6 | Slash commands | <https://learn.chatgpt.com/docs/reference/slash-commands> | 桌面 App 的斜杠命令表；`$` 调用技能；`/goal` 进度条 |
| D7 | Settings | <https://learn.chatgpt.com/docs/reference/settings> | 设置页各分区：General（发送键、防休眠、Follow-up behavior）、Profile、Keyboard shortcuts、Notifications、Appearance、Pets、Browser、Computer Use、Personalization、Memories、Archived chats；弹出窗口与 Always on top |
| D8 | Developer settings（app） | <https://learn.chatgpt.com/docs/developer-settings?surface=app> | 文件打开方式、命令输出显示量、终端位置；审查结果的投递方式；IDE context 同步；Git 设置；MCP；浏览器开发者模式 |
| D9 | Troubleshooting | <https://learn.chatgpt.com/docs/reference/troubleshooting> | 审查面板反映整个 Git 状态；侧边栏过滤（Chronological）；归档位置；方向键 ↑ 找回提示词；内置 codex 二进制的路径；日志位置；代码字体统一 |
| D10 | Code review（app） | <https://learn.chatgpt.com/docs/code-review?surface=app> | Code Review 插件（PR 收件箱、Summary/Changes、Submit review、Checks 里的 Fix）；本地审查面板的范围切换、多仓库、行内评论、暂存/还原、`/review` |
| D11 | Integrated terminal | <https://learn.chatgpt.com/docs/integrated-terminal> | 每个会话有一个终端，作用于当前项目或 worktree；Codex 能读取终端输出 |
| D12 | Codex environments | <https://learn.chatgpt.com/docs/environments/modes> | Local / Worktree / Cloud 三种运行位置；Work in 菜单（This computer / Cloud）；单独的 Worktree 开关 |
| D13 | Worktrees | <https://learn.chatgpt.com/docs/environments/git-worktrees> | worktree 的创建流程、Handoff、Create branch here、受管 worktree 与永久 worktree、`.worktreeinclude`、清理策略与快照恢复 |
| D14 | Local environments | <https://learn.chatgpt.com/docs/environments/local-environment> | setup 脚本、顶栏 actions；内置 Git 工具（暂存、提交、推送、创建 PR） |
| D15 | Codex Cloud | <https://learn.chatgpt.com/docs/cloud> | 云环境的创建、发布与云任务流程 |
| D16 | Permissions（模式） | <https://learn.chatgpt.com/docs/permission-modes> | 输入框下方的权限控件；Ask for approval / Approve for me 的含义 |
| D17 | Sandbox | <https://learn.chatgpt.com/docs/sandboxing> | 沙箱与审批的分工；App 的权限菜单项；审批范围（本次、本会话）；`sandbox_mode` 和 `approval_policy` 的取值 |
| D18 | Auto-review | <https://learn.chatgpt.com/docs/sandboxing/auto-review> | Approve for me 交给审查 agent 处理；Full Access 警告 |
| D19 | Agent approvals & security | <https://learn.chatgpt.com/docs/agent-approvals-security> | 自动审查条目的状态（Reviewing / Approved / Denied / Aborted / Timed out）和风险等级；常见的沙箱与审批组合 |
| D20 | Scheduled tasks | <https://learn.chatgpt.com/docs/automations> | Scheduled 视图作为收件箱；独立任务与会话内任务；在本地或 worktree 运行；RRULE |
| D21 | Skills & Plugins | <https://learn.chatgpt.com/docs/skills-and-plugins> | 技能与插件的概念；Codex 用 `$` 提及技能 |
| D22 | Plugins | <https://learn.chatgpt.com/docs/plugins> | 插件目录；Plugins 标签页；用 `@` 调用 |
| D23 | Subagents | <https://learn.chatgpt.com/docs/agent-configuration/subagents> | App 展示每个子代理的线程和它返回的摘要 |
| D24 | Long-running work | <https://learn.chatgpt.com/docs/long-running-work> | Goal 模式；输入框上方的进度条（暂停、继续、编辑、清除）；并行运行多个会话 |
| D25 | Prompting | <https://learn.chatgpt.com/docs/prompting> | Steer 与 Queue；排队消息显示在输入框上方；Dictate 听写 |
| D26 | Notifications | <https://learn.chatgpt.com/docs/notifications> | 完成通知的三档设置；Activity 视图（铃铛）；会话状态 |
| D27 | Pets | <https://learn.chatgpt.com/docs/pets?surface=app> | 悬浮宠物和 Quick Chat；四种状态（Running / Needs input / Ready / Blocked） |
| D28 | Codex Micro | <https://learn.chatgpt.com/docs/features/codex-micro> | 实体按键灯色与会话状态的对应（Idle / Thinking / Complete / Requires input / Error） |
| D29 | Composer Predictions | <https://learn.chatgpt.com/docs/features/composer-predictions> | 下一条消息预测，按 Tab 接受 |
| D30 | Browser（app） | <https://learn.chatgpt.com/docs/browser?surface=app> | 内置浏览器：标签页、全屏与分屏、标注模式（Annotate / Adjust）、Computer Use、开发者模式 |
| D31 | Work with files | <https://learn.chatgpt.com/docs/artifacts-viewer> | 生成文件预览（文档、表格、PDF、HTML、LaTeX）；会话侧栏显示计划、来源、生成文件、摘要 |
| D32 | Image inputs | <https://learn.chatgpt.com/docs/image-inputs> | 按住 Shift 拖入图片 |
| D33 | Remote connections | <https://learn.chatgpt.com/docs/remote-connections> | 手机或其他设备控制主机；扫码配对；安全中继层；通过 SSH 启动远端 app server；跨主机 Handoff |
| D34 | Codex on mobile | <https://learn.chatgpt.com/docs/mobile> | 移动端的四类操作：看任务、审批、看 diff、新建任务 |
| D35 | Windows app | <https://learn.chatgpt.com/docs/windows/windows-app> | Microsoft Store 包 ID；原生沙箱 / WSL；Open 默认编辑器；终端选择；Git 与 GitHub CLI 支撑审查面板和 GitHub 功能 |
| D36 | Linux app | <https://learn.chatgpt.com/docs/linux/linux-app> | Linux 预览版（deb / rpm，x64 / ARM64） |
| D37 | Codex App Server | <https://learn.chatgpt.com/docs/app-server> | JSON-RPC 协议；thread / turn / item 原语；事件与 item 类型；审批的请求时序；错误码；`review/start` |
| D38 | Open Source | <https://learn.chatgpt.com/docs/open-source> | 开源组件清单：IDE 扩展和 Codex cloud 不开源；桌面 App 不在清单内 |
| D39 | Glossary | <https://learn.chatgpt.com/docs/glossary> | 界面术语叫 Chat，协议术语叫 Thread；Turn、Handoff 的定义 |
| D40 | Import | <https://learn.chatgpt.com/docs/import> | 从 Claude Code、Claude Cowork、Cursor 导入配置和最近会话 |
| D41 | Codex IDE extension | <https://learn.chatgpt.com/docs/codex/ide> | IDE 扩展的形态 |
| D42 | Codex CLI | <https://learn.chatgpt.com/docs/codex/cli> | CLI 的形态 |
| D43 | Speed | <https://learn.chatgpt.com/docs/agent-configuration/speed> | Fast 与 Ultrafast 模式 |
| D44 | Computer Use | <https://learn.chatgpt.com/docs/computer-use> | Computer Use 插件与设置 |
| D45 | Appshots | <https://learn.chatgpt.com/docs/appshots> | 截取前台窗口作为附件 |
| D46 | Best practices | <https://learn.chatgpt.com/guides/best-practices> | Plan 模式用 `/plan` 或 Shift+Tab 切换 |
| D47 | Visualizations | <https://learn.chatgpt.com/docs/visualizations> | 会话内交互式可视化（预览中） |

## 二、发布说明

| 编号 | 资料 | 链接 | 支撑的内容 |
|---|---|---|---|
| W1 | What's new（每周汇总，2026-02 至 2026-10） | <https://learn.chatgpt.com/docs/whats-new> | 时间线：02-02 macOS 发布；03-04 Windows；03-12 主题；04-16 内置浏览器与 Computer Use；05-14 移动端 Remote；07-09 并入 ChatGPT 桌面 App；07-30 Activity 视图；08-11 Linux 预览和导入功能；09-29 DevDay |
| W2 | DevDay 2026（2026-09-29） | <https://learn.chatgpt.com/docs/whats-new/devday-2026> | Plugin Extensions 可以向 ChatGPT 添加侧边栏应用、面板、编辑器 |
| C | ChatGPT & Codex changelog | <https://learn.chatgpt.com/docs/changelog> | 下表列出引用的具体条目 |

引用的 changelog 条目如下。条目锚点格式为 `#codex-YYYY-MM-DD-app`。

| 编号 | 日期 / 版本 | 关键内容 |
|---|---|---|
| C0202 | 2026-02-02 Codex app 发布 | macOS 版；项目侧边栏、会话列表、审查面板；worktree、听写、Git、技能、自动化 |
| C0227 | 2026-02-27 | 会话行和 PR 按钮上的 PR 状态徽标；受管 worktree 保留数量的设置 |
| C0304 | 2026-03-04 | Windows 版：原生 PowerShell 与沙箱，可切换到 WSL |
| C0311 | 2026-03-11 | Codex 可读取集成终端；记住 local / worktree / cloud 选择；从 diff 槽位添加行内评论 |
| C0312 | 2026-03-12（26.312） | Themes：基础主题、强调色/背景色/前景色、UI 与代码字体、分享主题；自动化可选本地或 worktree |
| C0316 | 2026-03-16（26.313） | 标题栏前进/后退按钮；在 Finder 中打开；错误 toast |
| C0318 | 2026-03-18 | 从任意更早的消息分叉；切换模型和推理强度的斜杠命令；plan 模式提问时发通知 |
| C0319 | 2026-03-19 | `@` 菜单加入技能；Cmd/Ctrl+F 以当前选中文字开始搜索；品牌化的启动 loading shimmer |
| C0325 | 2026-03-25 | 技能与插件页面改版；每窗口独立缩放；侧边栏显示自动化标题和图标；内置 Raycast 主题 |
| C0401 | 2026-04-01 | MCP 审批面板加入"不再询问"；Windows MSIX 更新与托盘；自动化输入框支持 @ 提及；输入框显示子代理 diff 统计；生成文件的引用卡片 |
| C0409 | 2026-04-09 | 可折叠的行内审查评论；inline / detached 两种审查方式；侧面板的 Git 摘要和 Sources；New Quick Chat |
| C0410 | 2026-04-10 | PR 活动时间线、在 PR 页面评论、push 弹窗的选项；侧面板的文件标签页、标签拖拽排序、编辑 run action |
| C0412 | 2026-04-12 | 命令菜单文件搜索（Cmd+P）；侧边文件查看器的富预览；每会话多个终端标签；选中文字后出现 Ask Codex 浮层 |
| C0416 | 2026-04-16 | 内置浏览器；Computer Use；无项目会话；会话内定时任务；任务侧栏（计划、来源、产物、摘要）；PR 工作流；产物查看器；Memories；SSH 远程（alpha）；多终端；菜单栏和托盘；多窗口；Intel Mac |
| C0420 | 2026-04-20 | 本地分支搜索；输入框可粘贴非图片文件；可折叠的侧边栏分区；托盘显示用量限额；命令面板切换主题 |
| C0505 | 2026-05-05 | 听写清理和自定义词典；图片 lightbox |
| C0528 | 2026-05-28 | Windows 上的 Computer Use；远程控制 Windows；会话搜索覆盖内容和分支名；后台子代理的稳定 identicon；快捷键设置支持按键搜索 |
| C0604 | 2026-06-04 | Profile 中的活动洞察；修复"动画 diff 统计对齐" |
| C0609 | 2026-06-09 | 从 Claude Code 导入；插件页改版；在不支持半透明背景的系统（如 Windows 10）上改进窗口渲染 |
| C0611 | 2026-06-11 | 浏览器开发者模式；`/init`；macOS Dock 图标提供浅色和深色两种 Codex 变体；命令菜单增加 Unread chats；Cmd/Ctrl+Enter 提交自定义审批反馈；用量超限错误显示重置时间 |
| C0618 | 2026-06-18 | 本地与远程主机之间的会话 Handoff；SSH 连接管理的深链接 |
| C0709 | 2026-07-09 | Codex 并入 ChatGPT 桌面 App；Markdown 和代码可直接编辑；PR Chat；更清晰的 Full access 警告 |
| C0723 | 2026-07-23 | 多文件夹本地项目；ChatGPT Voice |
| C0730 | 2026-07-30 | Activity 视图（铃铛，Cmd/Ctrl+Opt+U）；跨仓库审查；图片的 Focused / Canvas 视图 |
| C0811 | 2026-08-11 | Linux 预览；从 Claude Code、Claude Cowork、Cursor 导入 |
| C0820 | 2026-08-20 | 分享只读会话快照；桌面端和 iOS 的置顶会话同步 |
| C0911 | 2026-09-11 | Pets 悬浮 Quick Chat；Windows Appshots |
| C0922 | 2026-09-22（26.917） | 通知声音设置（Default / Classic / None） |
| C1001 | 2026-10-01 | 批量把会话移入项目 |
| C1007 | 2026-10-07（26.1002） | 修复侧边栏分区（Remove from section）问题 |
| C1008 | 2026-10-08（26.1007） | 修复排队消息卡在 Sending、副窗口的 Steer；Changes 面板导航；权限阻止审批时给出恢复指引 |
| R0162 | Codex CLI 0.162.0（2026-10-08） | <https://github.com/openai/codex/releases/tag/rust-v0.162.0>：受管 worktree 的创建和列出工具（需开启 feature flag） |

## 三、官方博客（openai.com 对直接抓取返回 403，经 Wayback Machine 存档读取）

| 编号 | 文章 | 日期 | 链接（原文 / 存档） | 支撑的内容 |
|---|---|---|---|---|
| B1 | Introducing the Codex app | 2026-02-02 | <https://openai.com/index/introducing-the-codex-app/> / [存档 20260203222235](https://web.archive.org/web/20260203222235/https://openai.com/index/introducing-the-codex-app/) | 定位为"agent 指挥中心"；会话按项目组织；在会话里评审 diff 并评论；worktree；技能管理界面；Automations 的结果进入审查队列；personality；系统级沙箱 |
| B2 | Unlocking the Codex harness: how we built the App Server（作者 Celia Chen） | 2026-02-04 | <https://openai.com/index/unlocking-the-codex-harness/> / [存档 20261006031136](https://web.archive.org/web/20261006031136/https://openai.com/index/unlocking-the-codex-harness/) | App Server 的架构（stdio reader、message processor、thread manager、core thread）；thread / turn / item 原语；桌面 App 和 VS Code 扩展内置固定版本的 Codex 二进制，通过 stdio 走 JSON-RPC；Web 版在容器里跑 App Server，浏览器经 HTTP+SSE 连后端；文中认为跨厂商的 harness 协议容易收敛到能力的公共子集 |
| B3 | ChatGPT is now a partner for your most ambitious work | 2026-07-09 | <https://openai.com/index/chatgpt-for-your-most-ambitious-work/> / [存档 20260710191206](https://web.archive.org/web/20260710191206/https://openai.com/index/chatgpt-for-your-most-ambitious-work/) | Codex app 升级后直接变成新的 ChatGPT 桌面 App；可设 Codex 为默认视图、使用 Codex 图标；旧 ChatGPT 桌面 App 更名为 ChatGPT Classic；配图说明中有菜单项 Work / Codex / Scheduled / Sites |

## 四、开源代码（openai/codex，main 分支，提交 `e790ce27`，2026-10-16T02:39Z）

仓库许可证为 Apache-2.0（GitHub API 返回的 `license.spdx_id`），不接受外部 PR（见 [D38]）。

| 编号 | 路径 | 链接 | 支撑的内容 |
|---|---|---|---|
| G1 | 仓库根 | <https://github.com/openai/codex> | Apache-2.0；`codex-rs/` 下有 app-server、app-server-protocol、app-server-daemon、app-server-transport、worktree、thread-store 等 crate |
| G2 | `codex-rs/app-server/README.md` | <https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md> | MCP App UI 的展示偏好（inline / fullscreen）、项目信任、线程删除规则等补充协议说明 |
| G3 | `codex-rs/app-server-daemon/README.md` | <https://github.com/openai/codex/blob/main/codex-rs/app-server-daemon/README.md> | 守护进程为桌面和移动端等远程客户端提供 `codex app-server` 生命周期命令；`remote_control` 开关；自动更新 |
| G4 | `codex-rs/app-server-transport/src/transport/remote_control/protocol.rs` | <https://github.com/openai/codex/blob/main/codex-rs/app-server-transport/src/transport/remote_control/protocol.rs> | 远程控制：主机主动连 `wss://chatgpt.com/backend-api/wham/remote/control/server`；enroll / pair / pair status 接口；信封字段（client_id、stream_id、seq_id、ack、分片）里直接装 JSON-RPC 消息 |
| G5 | `codex-rs/cli/src/doctor/desktop/windows_security.rs` | <https://github.com/openai/codex/blob/main/codex-rs/cli/src/doctor/desktop/windows_security.rs> | Windows 诊断代码把 `OpenAI.Codex_` 包内的 `electron.exe` 或 `chatgpt.exe` 识别为 `codex_desktop` |
| G6 | `codex-rs/otel/src/auth_storage/originator.rs` | <https://github.com/openai/codex/blob/main/codex-rs/otel/src/auth_storage/originator.rs> | 客户端名 "Codex Desktop" 对应 originator `codex_desktop` |

## 五、第三方资料（用作补充，结论标为"推测"）

| 编号 | 资料 | 日期 | 链接 | 支撑的内容 |
|---|---|---|---|---|
| T1 | Simon Willison：Introducing the Codex app | 2026-02-02 | <https://simonwillison.net/2026/Feb/2/introducing-the-codex-app/> | App 用 Electron 和 Node.js 构建，选 Electron 是为了以后支持 Windows；自动化状态存在 SQLite；截图 alt 文字描述了深色侧边栏 |
| T2 | kitze：A technical breakdown of the Codex desktop app | 约 2026-05（拆的是 26.519 版） | <https://www.kitze.io/posts/codex-electron-app-technical-breakdown> | Electron 42.1.0；React 19 与 React Compiler；TanStack Query；Tailwind v4 风格样式加 CSS Modules；OKLCH 色彩 token 和 `--vscode-*` 变量；透明窗口背景；better-sqlite3、node-pty、Sparkle；主进程以子进程方式常驻运行 `codex app-server`；preload 暴露 `electronBridge` |
| T3 | codex-web-ui（npm 0.1.1）README | 2026 | <https://cdn.jsdelivr.net/npm/codex-web-ui@0.1.1/README.md> | 逆向结论：渲染层是 React；主进程与渲染层之间用 IPC 和 `window.electronBridge` 通信 |
| T4 | xdLawless2/codex-desktop-linux | 2026 | <https://github.com/xdLawless2/codex-desktop-linux> | 代码主要在 app.asar 里；需要重编译的原生模块是 better-sqlite3 和 node-pty；安装包内置 CLI 二进制 |
| T5 | Every：Vibe Check | 2026-02-02 | <https://every.to/vibe-check/codex-vibe-check> | 评测者认为右侧 diff 面板是亮点，界面整洁 |
| T6 | Push To Prod：Complete Beginner's Guide | 2026-04-11 | <https://getpushtoprod.substack.com/p/complete-beginners-guide-to-openais> | 认为观感接近 IDE；侧边栏能看出会话属于 local、cloud 还是 worktree |
| T7 | Daniel Vaughan：Inside the Codex App Workspace | 2026-04-17 | <https://codex.danielvaughan.com/2026/04/17/codex-app-workspace-pr-review-task-sidebar-artifact-viewer/> | 任务侧栏、PR 审查面板、产物查看器的使用说明 |

## 六、获取过程中的问题

- openai.com 对 curl 和 WebFetch 都返回 Cloudflare 403。改从 Wayback Machine 读取同一 URL 的存档快照，部分快照是 gzip 压缩，解压后再提取正文。
- 浏览器扩展未连接，无法打开页面截图，也没有实际安装或运行 App。界面细节全部来自文档、发布说明、官方配图说明和第三方资料。
- 官方 changelog 只有 HTML 版本（约 1.7 MB），按条目锚点切分后阅读。部分条目只能取到标题，正文用纯文本版本补齐。
