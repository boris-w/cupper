# R01 汇报：Codex 桌面客户端 UI 拆解

- 执行者：子 agent（产品调研员）
- 分支：`research/R01-codex-ui-teardown`
- 完成日期：2026-10-16

## 做了什么

调研的对象是 OpenAI Codex 桌面客户端的现状。截至 2026-10-16，Codex 已不再是独立 App，而是 ChatGPT 桌面 App 中的 Codex 视图。本报告拆解了它的信息架构、会话界面、审批与权限、运行模式、代码审查与 Git、其他功能、视觉风格、快捷键、端到端交互流程、与其他 Codex 形态的区别和技术线索，并在此基础上给出带复刻优先级的功能清单，以及"对架构的启示"。

资料以官方为主：learn.chatgpt.com 的文档（含全量导出）、changelog 与 What's new、openai.com 的三篇博客（通过 Wayback 存档读取），以及 openai/codex 仓库的源码。第三方评测和逆向分析只作补充。所有资料登记在 [sources.md](sources.md)，正文用方括号编号引用。

### 标注约定

- 正文默认是"已核实"：有官方文档、发布说明、官方博客或开源代码直接支撑，并给出编号。
- 带【推测】标记的内容来自第三方资料、逆向分析，或者由多条间接证据推断而来。
- 功能清单表单独列出"核实"一栏。

### 核心结论

1. 独立的 Codex app 已于 2026-07-09 并入 ChatGPT 桌面 App。Codex app 升级后就是新的 ChatGPT 桌面 App，Codex 作为与 Chat、Work 并列的专用视图保留下来 [B3][C0709]。因此"复刻 Codex UI"实际要复刻的是这个 Codex 视图，ChatGPT 的 Chat、Work、Sites、Space、dots 等不在范围内。
2. 主界面是三块：左侧"项目 → 会话"侧边栏，中间消息流和输入框，右侧或底部的工作区面板（审查、文件、浏览器、终端）。并行跑多个会话、在 App 内审查 diff 并推进到提交和 PR，是这个产品的核心闭环 [B1][D4][D10]。
3. 输入框是主要的控制台：模型、推理强度、权限模式、运行位置（本机或云端、是否用 worktree、起始分支）、`@` 提及、`$` 技能、斜杠命令、Steer 或 Queue 跟进消息，都集中在输入框上下 [D6][D12][D16][D25]。
4. 审查面板显示的是整个仓库的 Git 状态，而不只是 agent 的改动。它支持 Unstaged / Staged / Commit / Branch / Last turn 五种范围，可以添加行内评论再让 agent 处理，可以按全部、文件或 hunk 粒度暂存或还原，也能直接提交、推送、建 PR [D10][D14]。
5. 审批是消息流中的一类请求：Enter 批准，Esc 拒绝，可以选择只批准本次或整个会话。权限模式在输入框下方切换，有 Ask for approval、Approve for me（交给审查 agent）、Full access 三种 [D5][D16][D17]。
6. 所有会话共用一套状态模型：空闲、运行中、需要输入、完成未读、出错。侧边栏、Activity 视图、系统通知、悬浮宠物、实体键盘 Codex Micro 和移动端都基于它 [D26][D27][D28]。
7. 技术上，桌面 App 是 Electron 外壳加一个内置的 `codex` 二进制。外壳通过 stdio 与 `codex app-server` 进程交换 JSON-RPC，协议原语是 thread / turn / item。app-server 开源（Apache-2.0），桌面 App 本身不开源 [B2][D9][D37][D38][G5]。
8. 同一套 app-server 协议同时服务本机桌面（stdio）、SSH 远端、手机远控（主机主动连 chatgpt.com 中继）和 Web（容器内）。从开源代码看，中继信封里装的是明文 JSON-RPC，没有发现应用层端到端加密 [B2][D33][G3][G4]。对 cupper 来说，最关键的架构启示是：会话状态、审批、Git、worktree 和终端都应该放在一个与界面分离的"主机"层，桌面界面和移动端都只是这个主机的客户端。详见第十四节。

### 一、产品现状与时间线

| 日期 | 事件 | 来源 |
|---|---|---|
| 2026-02-02 | Codex app 在 macOS 发布。官方定位是"agent 指挥中心"，提供项目侧边栏、会话列表、审查面板、worktree、技能、Automations 和听写 | [B1][C0202] |
| 2026-02-04 | OpenAI 发文介绍 App Server：桌面 App、VS Code 扩展、Web、CLI 共用同一个 harness | [B2] |
| 2026-03-04 | Windows 版发布，原生运行在 PowerShell 和 Windows 沙箱上，可切换到 WSL | [C0304] |
| 2026-03-12 | 自定义主题上线；Automations 正式可用，可以选择在本地或 worktree 运行 | [C0312][W1] |
| 2026-04-16 | 大版本更新：内置浏览器、Computer Use、无项目会话、会话内定时任务、任务侧栏、PR 工作流、产物查看器、SSH 远程（alpha）、多终端、多窗口 | [C0416] |
| 2026-05-14 | 手机端 Remote 预览：在 ChatGPT 手机 App 里控制运行着桌面 App 的 Mac | [W1] |
| 2026-07-09 | Codex app 并入 ChatGPT 桌面 App（macOS 和 Windows）。可以把 Codex 设为默认视图并沿用 Codex 图标；旧的 ChatGPT 桌面 App 更名为 ChatGPT Classic | [B3][C0709] |
| 2026-07-30 | Activity 视图（铃铛）上线；支持跨仓库审查 | [C0730] |
| 2026-08-11 | Linux 预览版发布；支持从 Claude Code、Claude Cowork、Cursor 导入配置和最近会话 | [C0811][D40] |
| 2026-09-29 | DevDay 2026：dots、ChatGPT Space、Plugin Extensions（插件可以向 App 添加侧边栏应用、面板和编辑器） | [W2] |
| 2026-10-08 | 桌面 App 26.1007 发布，为采集时的最新版本 | [C1008] |

几点时效说明：

- 文档已从 `developers.openai.com/codex` 迁到 `learn.chatgpt.com/docs`，文档页本身不标日期，本报告的内容快照日期为 2026-10-16。
- 界面术语有过变化。原来的 thread 现在叫 Chat，协议层仍然叫 Thread [D39]；Automations 现在叫 Scheduled tasks，入口叫 Scheduled [D20]；iOS 上的 Remote 正在改名为 Codex [D33]。
- 很多功能受套餐、地区、灰度和工作区策略限制 [D3]。

### 二、信息架构

#### 窗口结构示意

位置是根据文档描述整理的示意，像素比例和部分控件的确切位置属于【推测】。

```
+--------------------+--------------------------------------+---------------------------+
| Sidebar            | Chat header: title | Hand off |       | Workspace tabs (right)    |
|  [ChatGPT v|Codex] |   Create branch here | Open | Share   |  Review | Files | Browser |
|  New chat (+Quick) |   env actions | back/forward          |  | Terminal | ...         |
|  Search chats      +--------------------------------------+  - review: scope switch, |
|  Skills / Plugins  | Message stream                       |    file tree, diff,       |
|  Scheduled / Sites |  user msg / reasoning summary        |    inline comments,       |
|  Code Review (pin) |  plan steps / commands / file edits  |    stage/revert/commit    |
|  Activity (bell)   |  approval cards / errors / subagents |  - task side panel: plan, |
|  Pinned            +--------------------------------------+    sources, artifacts,    |
|  Projects          | [goal progress row] [queued msgs]    |    summary, Git summary   |
|   > proj A: chats  | Composer: text, @, $, /, +attach,    |                           |
|   > proj B: chats  |   model, reasoning, Dictate, IDE ctx |  layout: full / split /   |
|  Chats (filter)    | Below: Work in (This computer/Cloud) |   hidden tabs             |
|  ...               |   Worktree, branch, env, permissions |                           |
|  [profile menu]    +--------------------------------------+---------------------------+
|                    | Bottom panel: integrated terminal (Cmd+J / Ctrl+`)               |
+--------------------+------------------------------------------------------------------+
```

#### 各区域说明

- 产品切换：顶部的 ChatGPT 下拉菜单可以选 ChatGPT 或 Codex [D2][D12]。在 ChatGPT 模式下，输入框上方还有 Chat / Work 切换；Ctrl+1/2/3 依次切到 Chat、Work、Codex [D5]。cupper 不需要这一层，这个位置可以改成 agent 切换【推测】。
- 侧边栏：
  - Codex 侧边栏只放 Codex 会话和项目，ChatGPT 会话不出现在这里 [D4]。
  - Recents 旁有 New chat，悬停时右侧出现 Quick chat 图标 [D4]。Chats 旁有 Add new project 按钮和过滤图标，可以按项目分组或按时间（Chronological）排列 [D9]。
  - 其他入口：Search chats [D4]，Skills [D21]，Scheduled [D20]，Code Review（从已安装插件中置顶）[D10]，Activity 铃铛 [D26]，Pinned 区 [D4][D10]。
  - 项目的三点菜单里有 Edit project（加文件夹、设主文件夹）、Archive chats、创建永久 worktree、Remove [D4][D9][D13]。
  - 支持可折叠分区和手动分区 [C0420][C1007]，最底部是个人资料菜单 [D27]。
- 项目与会话：
  - Projects 视图同时容纳 ChatGPT 项目和连接本机文件夹的本地项目。一个本地项目可以挂多个文件夹，主文件夹决定新会话的工作目录、Git 操作，以及 `AGENTS.md`、技能和 `config.toml` 的自动发现位置。远程项目目前只支持一个文件夹 [D4]。
  - 会话可以没有项目（standalone chat）[D4][C0416]。
  - 会话支持置顶、重命名、归档（在设置里恢复）、标记未读，搜索能匹配会话内容和 Git 分支名 [D4][D5][C0528]。
  - 置顶会话在桌面端和 iOS 之间同步 [C0820]，可以把多个会话一次移入项目 [C1001]。
- 会话标题栏：有 Hand off（在 Local 和 Worktree 之间移交）、Create branch here、Open（用编辑器打开）[D13]；还有前进/后退按钮 [C0316]、PR 状态徽标 [C0227]、Share [D3]。顶栏显示本地环境定义的 actions [D14]。会话底部可以选择当前运行在哪台主机，用于跨主机移交 [D33]。
- 右侧工作区：以标签页组织，包括审查标签（Ctrl+Shift+G）、文件标签、浏览器标签（Cmd+T）和终端标签（New tab → Terminal）[D5][D11][D30][C0410]。有三种布局：全屏、分屏、隐藏标签，用 Cmd+Shift+B 循环切换 [D5][D30]。另有文件树（Cmd+Shift+E）和任务侧栏（计划、来源、生成文件、摘要、Git 摘要）[D5][D31][C0409][C0416]。
- 底部面板：集成终端抽屉，Cmd+J 或 Ctrl+` 切换，位置可调 [D5][D11][W1]。
- 窗口与系统层：
  - 支持多窗口、把会话弹出为独立窗口并置顶 [D7][C0416]。
  - 有 macOS 菜单栏和 Windows 托盘，托盘显示用量限额 [C0416][C0420]。
  - 有悬浮宠物和 Quick Chat 浮层 [D27]。
  - 支持 `codex://` 深链接（打开新会话、设置、技能、插件等）[D5]。

### 三、会话界面

#### 输入框

| 能力 | 说明 | 来源 |
|---|---|---|
| 发送方式 | 可设置 Enter 直接发送，或要求 Cmd/Ctrl+Enter；输入框为空时按 ↑ 恢复上一条提示词，选错运行位置取消后也能用 ↑ 找回 | [D7][D5][D9] |
| 附件 | 按住 Shift 拖入图片；可粘贴非图片文件；有"Add"菜单【推测：文档配图说明提到 Add menu】；用 Appshots 截取前台窗口 | [D32][C0420][D45] |
| `@` 提及 | 文件（模糊搜索）、技能、插件和应用；自动化输入框也支持 `@` | [C0319][C0401][D22][D37] |
| `$` 技能 | 显式调用技能，已启用的技能也会出现在斜杠命令列表里 | [D6][D21] |
| 斜杠命令 | `/model` `/reasoning` `/plan` `/goal` `/review` `/compact` `/fork` `/side` `/status` `/init` `/new` `/rename` `/archive` `/pin` `/worktree` `/local` `/cloud` `/mcp` `/memories` `/fast` `/feedback` 等，可以在草稿中间输入 | [D6][C0318] |
| 模型与推理强度 | 输入框内有模型选择器，Ctrl+Shift+M 打开，Cmd+Ctrl+M 打开最近用过的模型；推理强度用 `/reasoning` 设置；`/fast` 切换 Fast 档 | [D5][D6][D43] |
| 权限控件 | 位于输入框下方，选项有 Ask for approval、Approve for me、Full access 和自定义 profile | [D16][D17] |
| 运行位置 | 位于输入框下方：Work in 选择 This computer 或 Cloud；单独的 Worktree 开关、起始分支选择、本地环境选择 | [D12][D13] |
| 跟进消息 | 运行中发送的消息可以 Steer（并入当前回合）或 Queue（排到下一回合），默认行为在设置里选；排队消息显示在输入框上方，可编辑、重排、发送、删除 | [D25][D7][C1008] |
| 目标模式 | `/goal` 设定后，输入框上方出现进度条，可暂停、继续、编辑、清除 | [D6][D24] |
| Plan 模式 | `/plan` 或 Shift+Tab 切换；Plan 模式下 agent 提问时会发通知 | [D46][C0318] |
| 其他 | Dictate 听写（可自定义词典）；IDE context 开关；Composer predictions 用 Tab 接受建议（Pro 测试中） | [D25][C0505][D8][D29] |

#### 消息流中各类内容的呈现

协议依据来自 app-server 的 item 类型 [D37]。界面形态有官方描述的写出来源，没有的标为【推测】。

| 内容 | 协议依据 | 界面呈现 |
|---|---|---|
| 用户消息 | `userMessage`（文本、图片、本地图片） | 气泡或块状展示；可以从任意一条历史消息分叉 [C0318] |
| 助手文本 | `agentMessage` 加流式 `item/agentMessage/delta`，有 `phase` 字段（commentary、partial_answer、final_answer） | 流式渲染 Markdown，支持代码高亮、表格、Mermaid 图 [C0709]；数学公式和 KaTeX【推测，见 T2】；回复中的文字可以选中后发起评论或追问 [C0412][C0911] |
| 推理过程 | `reasoning`（summary、content）加 `summaryTextDelta` | 显示推理摘要，有"思考中"计时【推测：iOS 修过"thinking timer"和"flashing thinking summaries"[C]，桌面端形态类似】；摘要可折叠【推测】 |
| 命令执行 | `commandExecution`：命令、cwd、状态、输出、退出码、耗时，输出流式追加 | 设置项控制消息里显示多少命令输出 [D8]；命令活动以摘要形式呈现 [C0611]；展开后看完整输出【推测】 |
| 文件修改 | `fileChange`（路径、类型、diff），以及整回合聚合 diff `turn/diff/updated` | 显示修改了哪些文件和行数统计（diff stat 带动画）[C0604]；输入框显示子代理的 diff 统计 [C0401]；点开进入审查面板【推测】 |
| 计划 / 待办 | `turn/plan/updated`：每步有 pending、inProgress、completed 三种状态；Plan 模式下另有 `plan` item | 任务侧栏显示 agent 的计划 [D31][C0416]；消息流里显示勾选清单【推测】 |
| 工具调用 | `mcpToolCall`、`webSearch`、`imageView`、`dynamicToolCall` | 浏览器、集成和来源活动以摘要形式呈现 [C0611]；MCP 应用可以内嵌界面（inline 或全屏）[G2] |
| 子代理 | `collabToolCall`（发送方线程、接收方线程、新线程） | App 为每个子代理展示线程和它返回的摘要 [D23]，后台子代理有稳定的 identicon [C0528] |
| 审批请求 | 由服务端发起的请求，见第四节 | 消息流中的审批卡片，Enter 批准，Esc 拒绝 [D5] |
| 错误 | 回合以 `failed` 结束，错误信息附带分类（上下文超限、用量超限、连接失败、沙箱错误等） | 用量超限时内联给出套餐指引和重置时间 [C0611]；恢复和报错用 toast 提示 [C0316]；权限阻止审批时给出恢复指引 [C1008] |
| 上下文压缩 | `contextCompaction` | 消息流中显示压缩标记【推测】；`/compact` 可手动触发 [D6] |
| 审查模式 | `enteredReviewMode` 和 `exitedReviewMode` | 审查发现以行内评论的形式出现在审查面板 [D10] |
| 完成摘要 | 无对应 item | 已完成的会话更清楚地展示生成的文件、自动化等持久产物 [C0611]；任务侧栏有摘要 [D31] |

### 四、审批与权限

- 权限模式由两件事共同决定：沙箱限定能访问的文件和网络，审批策略决定什么时候停下来问人 [D16][D17]。
  - Ask for approval 等于 `workspace-write` 沙箱加 `on-request` 审批，是推荐的默认值。
  - Approve for me 保持同样的沙箱边界，只是把越界请求交给一个独立的审查 agent 判断 [D18]。
  - Full access 等于 `danger-full-access` 加 `never`，开启时有警告弹窗 [D17][C0709]。
  - 另外还有命名或自定义的权限 profile [D17]。
- 审批交互：
  - 审批请求以卡片形式出现在消息流中，Enter 批准，Esc 拒绝 [D5]。可以附带自定义反馈，用 Cmd/Ctrl+Enter 提交 [C0611]。
  - 有多种范围可选时，文档建议选最窄的一种，比如只批准本次或本会话 [D17]。
  - 自定义 MCP 审批面板支持"不再询问" [C0401]。
- 协议层的审批类型 [D37]：
  - 命令执行：accept、acceptForSession、decline、cancel，或者批准时顺带修改 execpolicy 规则。
  - 文件修改：accept、acceptForSession、decline、cancel。
  - 网络访问：按目标主机和端口合并成一个请求。
  - 权限申请：只授予请求中的一部分，范围可以是本回合或本会话。
  - MCP elicitation 表单，以及 `tool/requestUserInput`（1 到 3 个问题，可以设自动超时）。
  - 每个请求被处理后，服务端都会发 `serverRequest/resolved`。
- 自动审查：审查结果作为条目显示在消息流中，状态有 Reviewing、Approved、Denied、Aborted、Timed out，并附带风险等级 [D19]。`/approve` 可以对最近一次被拒的请求放行一次 [D6]。
- 规则：可以对命令前缀设置 allow、prompt 或 forbid，作用于沙箱之外的命令 [D17]。
- 通知：需要审批或回答问题时，可以发系统通知并播放提示音 [D26][C0922]。也可以从手机上审批 [D34]。

### 五、运行模式

| 模式 | 行为 | 切换方式 | 来源 |
|---|---|---|---|
| Local | 直接在当前项目目录工作 | 输入框下方 Work in 选 This computer，关闭 Worktree；或用 `/local` | [D12][D6] |
| Worktree | 在 `$CODEX_HOME/worktrees` 下创建 Git worktree（detached HEAD），可以选起始分支（包括带未提交改动的当前分支）。会运行本地环境的 setup 脚本，并按 `.worktreeinclude` 复制被忽略的文件。默认保留最近 15 个受管 worktree，删除前会存快照，之后可以恢复 | 输入框下方的 Worktree 开关加分支选择；`/worktree` | [D13][D14] |
| Handoff | 在 Local 和 Worktree 之间移动会话及其代码，Git 操作由 App 完成。在 worktree 上可以用 Create branch here 把它变成正式分支 | 会话标题栏的 Hand off | [D13] |
| 永久 worktree | 作为一个独立项目存在，不会被自动删除，可以在上面开多个会话 | 项目三点菜单 | [D13] |
| Cloud | 基于已发布的云环境，每个任务有独立工作区，电脑合上后任务继续运行，可以在 Web 或手机上接着看 | Work in 选 Cloud，再选环境；`/cloud`、`/cloud-environment` | [D12][D15] |
| SSH 远程主机 | App 通过 SSH 启动远端的 app server，命令和文件读写都发生在远端；会话可以在本机和远端主机之间移交 | 设置 → Connections 添加主机；会话底部选择主机 | [D33][C0618] |

App 会记住上次选的 local、worktree 或 cloud [C0311]。定时任务也可以选择在本地还是在专用的后台 worktree 里运行 [D20]。

### 六、代码审查与 Git

- 本地审查面板 [D10]：
  - 显示仓库的真实 Git 状态，包括用户自己改的内容。
  - 范围可以选 Unstaged（默认）、Staged、Commit、Branch、Last turn。
  - 项目挂了多个仓库时，有仓库选择器；在 Last turn 范围下显示所有仓库的改动 [C0730]。
  - 有文件树，点击文件名在编辑器中打开，Cmd+点击某一行跳转到对应行，点击文件名背景折叠或展开 diff。
- 行内评论：悬停在某行上点 "+"，写下意见，然后发一条消息让 agent 处理。评论可以折叠 [D10][C0409]。
- 暂存与还原：可以按整个 diff、单个文件或单个 hunk 暂存、取消暂存或还原 [D10]。
- 提交、推送与 PR：在 App 内提交、推送分支、创建 PR [D14]。提交信息和 PR 描述可以自动生成，Git 设置里能配置生成用的提示词、分支命名前缀，以及是否允许 force push [D8]。推送弹窗提供几种推送选项 [C0410]。会话行上显示 PR 状态徽标（draft、open、merged、closed）[C0227]。
- `/review`：可以审查未提交的改动或与基础分支的差异，协议还支持审查单个提交和自定义指令 [D37]。审查不会改动工作区，发现以行内评论的形式出现。默认在当前会话里运行（Inline），也可以设为另开会话（Detached）[D8][D10]。
- Code Review 插件（PR 级别）[D10][C0709]：
  - 收件箱分为 Assigned to me、Assigned to my team、Authored by me。
  - Summary 页显示描述、活动、评论、检查结果和 Stack；Changes 页显示 diff 和评论，可以把文件标记为已看过。
  - 点 Review with Codex 开一个审查会话；在 Checks 里点 Fix，会把失败的检查附加到会话中。
  - 用 Submit review 提交 Comment、Approve 或 Request changes。
  - PR Chat 里可以查看、编辑、接受或拒绝补丁。
- 在 diff 中直接编辑：可以直接改 Markdown 和代码，也可以选中内容后让 Codex 改写 [C0709]。
- 合并：文档没有描述在 App 内合并 PR，合并大概率在 GitHub 上完成【推测】。

### 七、其他功能

| 功能 | 要点 | 来源 |
|---|---|---|
| 集成终端 | 每个会话有自己的终端，作用于当前项目或 worktree；支持多个标签；Codex 能读取终端输出；可以选默认 shell（Windows 上有 PowerShell、CMD、Git Bash、WSL） | [D11][C0311][C0412][D35] |
| 本地环境 | `.codex` 目录下的 setup 脚本和 actions（可分平台定义），actions 显示在顶栏，在终端中执行，Cmd+Shift+D 运行第一个 action | [D14][D5] |
| 文件 | Cmd+P 搜索文件，文件树，图片、PDF、Markdown 的富预览，文件标签页 | [D5][C0412][C0410] |
| 技能 | 侧边栏有 Skills 页面，可以浏览、创建、启用技能；用 `$skill-creator` 创建新技能 | [D21][B1][C0325] |
| 插件与 MCP | 插件目录与安装（插件管理已移入设置）；`/mcp` 查看 MCP 状态；需要 OAuth 时 App 会发起认证；Plugin Extensions 可以向 App 添加侧边栏、面板、编辑器 | [D22][D8][C0709][W2] |
| 子代理 | 可以要求并行派出子代理，App 展示每个子线程 | [D23] |
| 定时任务 | 侧边栏的 Scheduled 页面相当于收件箱；分独立任务和会话内任务；支持 RRULE；可以选模型和推理强度；在本地或 worktree 运行 | [D20] |
| 内置浏览器 | 本地页面预览，标注（Annotate，Adjust 可调样式），Computer Use，开发者模式（CDP） | [D30] |
| Computer Use 和 Appshots | 操作 macOS 和 Windows 上的应用；同时按两个 Cmd 键截取前台窗口 | [D44][D45] |
| 生成文件预览 | 文档、表格、演示文稿、PDF、HTML、LaTeX 的预览，以及标注后修改 | [D31] |
| 图片 | 图片生成，lightbox（Focused / Canvas 两种视图） | [C0730][C0505] |
| 记忆、语音、导入 | Memories；ChatGPT Voice；从 Claude Code、Cursor 导入配置和会话 | [D7][C0723][D40] |
| 宠物与 Codex Micro | 悬浮动画宠物（运行中、需要输入、就绪、受阻四种状态）；实体按键灯显示会话状态 | [D27][D28] |
| 分享 | 生成会话的只读快照，会脱敏密钥，不包含工具调用和 shell 输出 | [D3] |
| 设置 | General、Profile（token 统计）、Keyboard Shortcuts、Notifications、Appearance、Pets、Browser、Computer Use、Personalization（`AGENTS.md`）、Memories、Archived chats、Git、Worktrees、Connections、Import；支持设置搜索 | [D7][D8][D13][D33][D40][C0609] |
| 状态与用量 | `/status` 显示会话 ID、上下文用量和限额；`/usage` 查看用量；Profile 显示 token 活动 | [D6][D7] |

### 八、视觉风格要点

官方能核实的部分：

- 主题可以完全自定义：选择基础主题，调整强调色、背景色、前景色，更换 UI 字体和代码字体，还能把自定义主题分享给别人 [D7][C0312]。App 内置 Raycast 主题包 [C0325]，命令面板里可以直接切换主题 [C0420]。
- 代码字体是统一的：审查面板、集成终端和其他代码展示都使用设置里的 Code font [D9]。字号可以用 Cmd +/-/0 缩放，每个窗口可以单独缩放 [D5][C0325]。
- 品牌和动效：
  - 启动时有带品牌标识的 loading shimmer [C0319]。
  - 运行中的标题用 shimmer 动画，开源 CLI 有一个 PR 专门让终端界面的 shimmer 节奏与桌面端标题对齐 [C 2026-10，CLI PR #48757]。
  - diff 统计数字带动画 [C0604]。
  - 宠物是精灵图动画，可以自定义 [D27][D5]。
  - 后台子代理用 identicon 区分 [C0528]。
- 窗口材质：默认使用半透明背景，在不支持半透明的系统（如 Windows 10）上会退化处理 [C0609]。macOS Dock 图标有浅色和深色两种 Codex 变体，也可以继续使用 Codex 图标 [C0611][B3]。
- 状态配色：Codex Micro 用白色表示空闲、蓝色表示运行中、绿色表示完成未读、琥珀色表示需要输入、红色表示出错 [D28]。App 内的状态徽标很可能用同样的语义配色【推测】。
- 信息密度：界面以面板和标签页组织。审查面板、文件树、终端、浏览器可以全屏、分屏或隐藏 [D5][D30]，整体接近 IDE 的工作台布局。第三方评测也把它描述为观感接近 IDE [T6]。

第三方资料补充（全部为【推测】）：

- 实现层面：Tailwind v4 风格的工具类加 CSS Modules，色彩 token 使用 OKLCH，同时存在 `--vscode-*` 变量，说明可能与 VS Code 扩展共用 UI 代码；窗口背景透明 [T2]。
- 观感：2026-02 版本的截图里侧边栏是深色的 [T1]，评测者称界面整洁 [T5]。
- 亮色和暗色：基础主题本身就有亮暗之分，Dock 图标也有浅深两种变体，推断 App 支持亮色和暗色，并可能默认跟随系统。文档没有明确写出"跟随系统"。

### 九、快捷键

以下列出 macOS 默认值。Windows 和 Linux 上大多把 ⌘ 换成 Ctrl、⌥ 换成 Alt，完整三平台对照见 [D5]。所有快捷键都可以在 Settings → Keyboard Shortcuts 中修改，支持按命令名或直接按键搜索 [D7][C0528]。

| 类别 | 快捷键 |
|---|---|
| 全局 | 命令菜单 ⌘K 或 ⌘⇧P；设置 ⌘,；快捷键列表 ⌘/；打开文件夹 ⌘O；前进/后退 ⌘] / ⌘[；切换侧边栏 ⌘B；底部面板 ⌘J；终端 ⌃`；清除所有未读 ⇧Esc；撤销/重做 App 操作 ⌘Z / ⌘⇧Z |
| 会话 | 新建 ⌘N；无项目会话 ⌘⌥O；归档 ⌘⇧A；标记未读 ⌘⇧U；置顶 ⌘⌥P；重命名 ⌘⌥R；旁路会话 ⌘⌥S；会话内查找 ⌘F；上一个/下一个会话 ⌃⇧Tab / ⌃Tab；下一个需要关注的会话 ⌘⌥A；第 1 到 9 个会话 ⌘1–9 |
| 输入与模式 | 模型选择器 ⌃⇧M；最近用过的模型 ⌃⌘M；项目选择器 ⌘⌥⇧O；语音 ⌃⇧V；恢复上一条提示词 ↑；批准 ⏎；拒绝 Esc；Activity 视图 ⌘⌥U |
| 工作区 | 文件搜索 ⌘P；文件树 ⌘⇧E；审查标签 ⌃⇧G；循环切换布局 ⌘⇧B；全屏视图 ⌘⇧F；浏览器标签 ⌘T；跳转到行或地址栏 ⌘L；运行第一个环境 action ⌘⇧D；复制工作目录 ⌘⇧C；复制会话深链接 ⌘⌥L；Appshot 同时按两个 ⌘ |
| 全局热键 | 显示宠物和 Quick Chat：macOS 上 ⌥Space，Windows 上 Win+Alt+P [D27] |

### 十、完整交互流程：从新建会话到合并改动

下面以"在 worktree 中修一个 bug，最后合入主分支"为例。

1. 打开 App，在产品切换中选 Codex（可以设为默认视图）[D12][B3]。按 ⌘O 添加项目文件夹，或在侧边栏选一个已有项目，然后按 ⌘N 新建会话 [D4][D5]。
2. 在新会话页的输入框下方，Work in 选 This computer，打开 Worktree 开关，选起始分支（比如 `main`），如有需要再选一个本地环境来跑 setup 脚本 [D12][D13]。
3. 在输入框里用 ⌃⇧M 或 `/model` 选模型，用 `/reasoning` 设推理强度，在下方权限控件里选 Ask for approval [D5][D6][D16]。
4. 描述任务：用 `@` 引用相关文件，按住 Shift 拖入报错截图。任务比较复杂时，先用 `/plan`（或 Shift+Tab）进入 Plan 模式，回答 agent 的澄清问题，确认计划后再开始实现 [D32][C0319][D46]。
5. 发送后，App 创建 worktree 并运行 setup。消息流里依次出现推理摘要、计划步骤的状态变化、命令执行及其输出、文件修改及 diff 统计。侧边栏显示这个会话正在运行 [D13][D37]。
6. agent 需要越过沙箱边界（比如访问网络装依赖）时，消息流里出现审批卡片，按 ⏎ 批准或 Esc 拒绝，也可以选择对本会话生效。如果 App 在后台，会收到系统通知；用 ⌘⌥A 可以跳到下一个需要关注的会话 [D5][D17][D26]。
7. 运行期间可以随时追加消息：Steer 会把消息并入当前回合，Queue 会排到下一回合。同时也可以在其他会话里并行开展别的任务 [D25][D24]。
8. 回合结束后，会话显示为完成未读。在任务侧栏查看摘要和生成的文件，用 ⌃⇧G 打开审查标签，切到 Last turn 或 Unstaged 查看 diff [D10][D31]。
9. 发现问题时，在 diff 上悬停点 "+" 写行内评论，然后发"处理行内评论，保持范围最小"，agent 会继续修改 [D10]。
10. 验证：在集成终端（⌃`）里跑测试，或用 ⌘⇧D 运行环境 action；如果是 Web 应用，在内置浏览器中预览并对页面做标注 [D11][D14][D30]。可以再执行 `/review`，让 agent 审查与基础分支的差异，审查发现会以行内评论出现 [D10]。
11. 整理改动：按文件或 hunk 暂存需要的部分，还原不需要的部分 [D10]。
12. 落盘：在 worktree 上点 Create branch here，提交（提交信息可自动生成），推送，创建 PR（描述可自动生成）[D13][D14][D8]。如果更想在自己常用的 IDE 或开发服务器里验证，可以先用 Hand off 把会话和代码移回 Local [D13]。
13. PR 阶段：会话行上显示 PR 状态徽标。CI 失败时，在 Code Review 插件的 Checks 里点 Fix，把失败信息附加到会话让 agent 修复；也可以在 App 里提交审查意见 [C0227][D10]。合并操作在 GitHub 上完成【推测】。
14. 收尾：归档会话（⌘⇧A）。对应的受管 worktree 会被自动清理，清理前会存快照，以后可以恢复 [D13]。

### 十一、与其他 Codex 形态的区别

| 形态 | 特点 | 与桌面 App 的主要差别 | 来源 |
|---|---|---|---|
| CLI（TUI） | 在终端里运行，以启动目录为项目；同样有斜杠命令、`/review`、`/goal`、`/side` 等 | 没有 Projects 视图、Scheduled 管理界面和文件预览；可以用 `--remote` 连接远端的 app-server | [D42][D4][D20][D31][D37] |
| IDE 扩展 | 运行在 VS Code、Cursor、Windsurf 的侧边栏里，可以把编辑器中打开的文件和选中内容作为上下文；JetBrains 和 Xcode 有各自的集成 | 没有 Projects 视图；在同一项目中与桌面 App 共享会话和编辑器上下文（IDE context）；扩展不开源 | [D41][D4][D8][D38] |
| Web（Codex Cloud） | 任务在云端容器中运行，浏览器通过 HTTP+SSE 接收事件 | 不能访问本机文件和进程；以云环境为单位；任务可以在 Web、手机、桌面之间接续 | [D15][B2] |
| 移动端 | ChatGPT 手机 App 里的 Codex（原名 Remote）：控制已连接的电脑或云任务，可以审批、查看 diff、新建任务 | 本身不执行代码，工作发生在被控的主机或云端 | [D34][D33] |

### 十二、技术线索

| 线索 | 结论 | 核实 | 来源 |
|---|---|---|---|
| 外壳框架 | Electron。开源代码中的 Windows 诊断逻辑把 `OpenAI.Codex_` 包里的 `electron.exe` 识别为 `codex_desktop` | 已核实 | [G5][T1] |
| Electron 版本与渲染层 | Electron 42.1.0（26.519 版），React 19，TanStack Query，Tailwind v4 风格样式加 CSS Modules，原生模块有 node-pty、better-sqlite3、Sparkle | 推测（第三方逆向） | [T2][T3][T4] |
| 与 agent 的连接 | 安装包内置固定版本的 `codex` 二进制，路径为 `Codex.app/Contents/Resources/codex`；外壳把 app-server 作为常驻子进程启动，通过 stdio 交换 JSONL 格式的 JSON-RPC | 已核实 | [B2][D9] |
| 协议 | app-server：JSON-RPC 2.0（线上省略 `jsonrpc` 字段），原语为 thread / turn / item，item 有 started、delta、completed 三段生命周期，服务端可以主动发请求（审批、用户输入）；可以生成 TypeScript 类型和 JSON Schema | 已核实 | [D37][B2] |
| 远端与移动 | SSH 远程时由 App 通过 SSH 启动远端 app server；有守护进程 `app-server daemon` 为桌面和移动端提供生命周期管理；远程控制时主机主动用 WebSocket 连接 chatgpt.com 的中继，信封里装 JSON-RPC，带 seq、ack、分片和配对码 | 已核实 | [D33][G3][G4] |
| 中继是否端到端加密 | 已读的 remote_control 源文件中没有应用层加密的相关代码，传输安全依赖 TLS | 推测（只读了部分文件） | [G4] |
| Git 操作的实现位置 | app-server 的公开 API 里没有 stage、commit、push、PR 这类方法；审查面板依赖本机 Git，GitHub 相关功能依赖 GitHub CLI。推断这些操作由桌面 App 自己执行 | 推测 | [D37][D35][C0401] |
| 开源范围与许可证 | openai/codex 仓库（CLI、app-server、协议、SDK）采用 Apache-2.0；IDE 扩展和 Codex cloud 明确不开源；桌面 App 不在开源清单中，推断闭源；仓库不接受外部 PR | 已核实（桌面 App 闭源为推断） | [D38][G1] |
| 分发与更新 | macOS 用 dmg 并保留 Codex.app 包名，Windows 用 Microsoft Store 的 MSIX，Linux 提供 deb 和 rpm；版本号格式为 26.MDD，如 26.1007；URL scheme 沿用 `codex://` | 已核实 | [D1][D35][D36][C0401][D5] |
| 本地数据 | 会话记录在 `~/.codex/sessions`，归档在 `archived_sessions`，macOS 日志在 `~/Library/Logs/com.openai.codex` | 已核实 | [D9] |

界面元素与 app-server 协议的对应关系如下，可以作为 R02 对照 ACP 能力时的参考。

| 界面元素 | app-server 方法或事件 |
|---|---|
| 侧边栏会话列表、搜索、置顶、归档 | `thread/list`（可按 cwd、isPinned、archived、searchTerm 过滤）、`thread/metadata/update`、`thread/archive`、`thread/unarchive`、`thread/name/set` |
| 会话运行状态 | `thread/status/changed`、`turn/started`、`turn/completed` |
| 发送、Steer、中断 | `turn/start`（可覆盖 cwd、模型、推理强度、沙箱、审批策略）、`turn/steer`、`turn/interrupt` |
| 分叉、回滚、压缩 | `thread/fork`（`lastTurnId`、ephemeral）、`thread/revert`、`thread/compact/start` |
| 目标进度条 | `thread/goal/set`、`thread/goal/get`、`thread/goal/clear` 以及对应的 updated 事件 |
| 消息流 | `item/started`、`item/*/delta`、`item/completed`、`turn/plan/updated`、`turn/diff/updated`、`thread/tokenUsage/updated` |
| 审批卡片 | `item/commandExecution/requestApproval`、`item/fileChange/requestApproval`、`item/permissions/requestApproval`、`mcpServer/elicitation/request`、`tool/requestUserInput`、`serverRequest/resolved` |
| `/review` | `review/start`（目标可以是 uncommittedChanges、baseBranch、commit、custom；投递方式 inline） |
| 模型选择、技能、MCP、`@` 文件搜索 | `model/list`、`skills/list`、`mcpServerStatus/list`、`fuzzyFileSearch/*` |
| 终端与命令 | `command/exec`（支持 PTY、写入 stdin、调整尺寸）、`thread/shellCommand`、`process/spawn`（实验性） |

### 十三、功能清单表（含复刻优先级建议）

优先级的判断标准：P0 是复刻 Codex 编码闭环（会话、审批、审查、提交）必需的功能；P1 是体验上重要、可以在 MVP 之后紧跟的功能；P2 可以后置，或者与 Codex/OpenAI 自身服务强绑定。"核实"一栏说明功能本身是否有官方出处，界面细节中的推测部分已在前文单独标注。

#### A. 信息架构与导航

| # | 功能 | 描述 | 优先级 | 来源 | 核实 |
|---|---|---|---|---|---|
| A1 | 三栏工作台布局 | 左侧边栏、中间会话区、右侧标签工作区，加底部终端面板；支持全屏、分屏、隐藏标签三种布局 | P0 | [D5][D10][D11][D30] | 已核实（比例为推测） |
| A2 | 项目分组的会话列表 | 本地文件夹即项目，会话挂在项目下；可以添加或移除项目（⌘O） | P0 | [D4][D9] | 已核实 |
| A3 | 会话基础管理 | 新建、重命名、归档、恢复，按项目分组或按时间排序 | P0 | [D4][D5][D9] | 已核实 |
| A4 | 会话状态指示 | 运行中、需要输入、完成未读、出错；可以清除未读，可以跳到下一个需要关注的会话 | P0 | [D5][D26][D28] | 已核实 |
| A5 | 置顶、未读标记、自定义分区 | 置顶会话和项目，手动标记未读，可折叠和手动排序的分区 | P1 | [D4][C0420][C1007] | 已核实 |
| A6 | 会话搜索 | 按标题、内容和 Git 分支名搜索；会话内用 ⌘F 查找 | P1 | [D5][C0528] | 已核实 |
| A7 | 命令菜单 | ⌘K 打开，可以执行命令、搜索文件、找未读会话、切换主题 | P1 | [D5][C0412][C0611][C0420] | 已核实 |
| A8 | Activity 视图 | 侧边栏铃铛，集中显示未读、运行中、等待回复的会话 | P1 | [D26][C0730] | 已核实 |
| A9 | 无项目会话 | 不选文件夹就能开始的会话 | P1 | [D4][C0416] | 已核实 |
| A10 | 多文件夹项目 | 一个项目挂多个文件夹，其中一个为主文件夹 | P2 | [D4][C0723] | 已核实 |
| A11 | 多窗口、弹出窗口、置顶 | 会话可以弹出为独立窗口并保持在最上层 | P2 | [D7][C0416] | 已核实 |
| A12 | 前进/后退导航 | 标题栏按钮，⌘[ 和 ⌘] | P1 | [C0316][D5] | 已核实 |
| A13 | 深链接 | `codex://` 打开新会话、设置、技能等 | P2 | [D5] | 已核实 |
| A14 | 菜单栏或托盘常驻 | 显示用量，关掉窗口后仍在后台运行 | P2 | [C0416][C0401][C0420] | 已核实 |
| A15 | 产品切换（Chat / Work / Codex） | ChatGPT 层面的模式切换，cupper 不适用，可以把这个位置改成 agent 切换 | 不复刻 | [D3][D12] | 已核实 |

#### B. 输入框

| # | 功能 | 描述 | 优先级 | 来源 | 核实 |
|---|---|---|---|---|---|
| B1 | 多行输入与发送设置 | 可选 Enter 或 ⌘Enter 发送，↑ 恢复上一条 | P0 | [D7][D5] | 已核实 |
| B2 | 图片附件 | 拖入或粘贴图片 | P0 | [D32] | 已核实 |
| B3 | 文件附件 | 粘贴或附加非图片文件 | P1 | [C0420] | 已核实 |
| B4 | `@` 文件提及 | 模糊搜索项目文件并插入 | P0 | [C0319][D37] | 已核实 |
| B5 | `@` 技能、插件、应用，`$` 技能 | 显式调用某个能力 | P1 | [D6][D21][D22] | 已核实 |
| B6 | 斜杠命令菜单 | 至少包括 /model /reasoning /plan /review /compact /fork /status /new /rename；可以在草稿中间使用 | P0 | [D6][C0318] | 已核实 |
| B7 | 模型与推理强度选择 | 输入框内的选择器加快捷键 | P0 | [D5][D6] | 已核实 |
| B8 | 权限模式选择 | 输入框下方的控件 | P0 | [D16][D17] | 已核实 |
| B9 | 运行位置选择 | Work in（本机或云端）、Worktree 开关、分支、本地环境 | P0（Local）/ P1（Worktree）/ P2（Cloud） | [D12][D13] | 已核实 |
| B10 | Steer 和 Queue | 运行中追加消息，选择并入当前回合还是排到下一回合 | P0 | [D25][D7] | 已核实 |
| B11 | 排队消息管理 | 输入框上方显示排队消息，可编辑、重排、删除 | P1 | [D25][C1008] | 已核实 |
| B12 | 中断当前回合 | 停止正在运行的回合 | P0 | [D9][D37] | 已核实 |
| B13 | Plan 模式切换 | `/plan` 或 Shift+Tab | P1 | [D46][D6] | 已核实 |
| B14 | 目标模式 | `/goal`，输入框上方的进度条可以暂停、继续、编辑、清除 | P1 | [D6][D24] | 已核实 |
| B15 | 语音听写 | Dictate，可配自定义词典 | P2 | [D25][C0505] | 已核实 |
| B16 | IDE context 开关 | 把编辑器里打开的文件作为上下文 | P2 | [D8] | 已核实 |
| B17 | 下一条消息预测 | 回复结束后给出建议，按 Tab 接受 | P2 | [D29] | 已核实 |

#### C. 消息流呈现

| # | 功能 | 描述 | 优先级 | 来源 | 核实 |
|---|---|---|---|---|---|
| C1 | 流式 Markdown 渲染 | 支持代码高亮和表格 | P0 | [D37][B2] | 已核实 |
| C2 | Mermaid 与数学公式 | 图表和公式渲染 | P1 | [C0709][T2] | Mermaid 已核实，KaTeX 为推测 |
| C3 | 推理摘要 | 流式显示推理摘要，可折叠 | P0 | [D37][D3] | 已核实（折叠为推测） |
| C4 | 命令执行项 | 显示命令、状态、输出、退出码和耗时，可以设置显示多少输出 | P0 | [D37][D8] | 已核实 |
| C5 | 文件修改项 | 显示改了哪些文件和行数统计，可以进入 diff | P0 | [D37][C0604] | 已核实 |
| C6 | 计划和待办清单 | 每一步的状态（待办、进行中、完成），任务侧栏同步显示 | P0 | [D37][D31] | 已核实 |
| C7 | 错误与恢复指引 | 用量超限附带重置时间，连接失败，权限受阻时的指引；用 toast 提示 | P0 | [D37][C0611][C0316][C1008] | 已核实 |
| C8 | 工具调用 | MCP、Web 搜索、图片查看等以摘要形式展示 | P1 | [D37][C0611] | 已核实 |
| C9 | 子代理活动 | 子线程卡片、identicon，可以打开子线程查看 | P1 | [D23][C0528] | 已核实 |
| C10 | 上下文压缩 | `/compact` 加压缩标记 | P1 | [D6][D37] | 已核实（标记的样式为推测） |
| C11 | 会话分叉 | 从任意一条消息分叉，可以分叉到 worktree | P1 | [D6][C0318] | 已核实 |
| C12 | 任务侧栏 | 显示计划、来源、生成文件、摘要、Git 摘要 | P1 | [D31][C0409][C0416] | 已核实 |
| C13 | 旁路会话 | `/side` 开一个临时会话，不打扰主会话 | P2 | [D6][D5] | 已核实 |
| C14 | 回复文字评论 | 选中回复中的文字后发起评论或追问 | P2 | [C0412][C0911] | 已核实 |
| C15 | 只读快照分享 | `/share` 生成链接，自动脱敏密钥 | P2 | [D3] | 已核实 |

#### D. 审批与权限

| # | 功能 | 描述 | 优先级 | 来源 | 核实 |
|---|---|---|---|---|---|
| D-1 | 消息流内的审批卡片 | 覆盖命令、文件修改、网络、权限、MCP 表单；⏎ 批准，Esc 拒绝 | P0 | [D5][D37] | 已核实 |
| D-2 | 审批范围 | 本次或本会话；可以附带反馈，⌘Enter 提交 | P0 | [D17][D37][C0611] | 已核实 |
| D-3 | 权限模式 | Ask for approval 和 Full access（带警告） | P0 | [D16][D17][C0709] | 已核实 |
| D-4 | 审批通知 | 系统通知和提示音 | P0 | [D26][C0922] | 已核实 |
| D-5 | 命令规则与"不再询问" | 按命令前缀设置 allow、prompt 或 forbid | P1 | [D17][C0401] | 已核实 |
| D-6 | 自定义权限 profile | 命名的权限组合 | P2 | [D17] | 已核实 |
| D-7 | Approve for me | 由审查 agent 审批，条目显示状态和风险等级 | P2 | [D18][D19] | 已核实 |

#### E. 运行模式

| # | 功能 | 描述 | 优先级 | 来源 | 核实 |
|---|---|---|---|---|---|
| E1 | Local | 直接在项目目录工作 | P0 | [D12] | 已核实 |
| E2 | Worktree | 创建 worktree、选起始分支、setup 脚本、`.worktreeinclude`、清理与快照恢复 | P1 | [D13][D14] | 已核实 |
| E3 | Handoff | 在 Local 和 Worktree 之间移动会话，Create branch here | P1 | [D13] | 已核实 |
| E4 | 本地环境 actions | 顶栏按钮，在终端中执行 | P1 | [D14] | 已核实 |
| E5 | 永久 worktree | 作为独立项目存在的长期 worktree | P2 | [D13] | 已核实 |
| E6 | SSH 远程主机项目与跨主机移交 | 在远端主机上运行会话，可以在主机之间移动 | P2 | [D33][C0618] | 已核实 |
| E7 | Cloud | 云环境和云任务 | P2 | [D12][D15] | 已核实 |

#### F. 代码审查与 Git

| # | 功能 | 描述 | 优先级 | 来源 | 核实 |
|---|---|---|---|---|---|
| F1 | 审查面板（diff 查看） | 文件列表加 diff；至少提供 Unstaged 和 Last turn 两种范围 | P0 | [D10][D9] | 已核实 |
| F2 | 其余审查范围 | Staged、Commit、Branch | P1 | [D10] | 已核实 |
| F3 | 行内评论交给 agent | 悬停点 "+" 写评论，再发消息让 agent 处理 | P0 | [D10] | 已核实 |
| F4 | 暂存、取消暂存、还原 | 按全部、文件或 hunk 操作 | P0 | [D10] | 已核实 |
| F5 | 提交与推送 | 自动生成提交信息，推送选项 | P0 | [D14][D8][C0410] | 已核实 |
| F6 | 创建 PR | 自动生成 PR 描述，会话行显示 PR 状态徽标 | P1 | [D14][D8][C0227] | 已核实 |
| F7 | `/review` agent 审查 | 审查发现以行内评论出现，可选 inline 或 detached | P1 | [D10][D8][D37] | 已核实 |
| F8 | 在编辑器中打开 | Open 菜单，点击文件名或 ⌘ 点击某行 | P1 | [D10][D35] | 已核实 |
| F9 | Git 设置 | 分支前缀、force push、提交信息和 PR 描述的提示词 | P1 | [D8] | 已核实 |
| F10 | 多仓库审查 | 仓库选择器，All repos 视图 | P2 | [D10][C0730] | 已核实 |
| F11 | 在 diff 中直接编辑 | 编辑 Markdown 和代码，选中内容让 agent 改写 | P2 | [C0709] | 已核实 |
| F12 | PR 审查插件 | 收件箱、Summary 和 Changes、Checks 里的 Fix、Submit review、Stack | P2 | [D10] | 已核实 |

#### G. 其他功能与设置

| # | 功能 | 描述 | 优先级 | 来源 | 核实 |
|---|---|---|---|---|---|
| G1 | 集成终端 | 每个会话一个，作用于项目或 worktree，⌃` 切换 | P0 | [D11] | 已核实 |
| G2 | 多终端标签，agent 读取终端输出 | 一个会话可开多个终端；agent 能看到终端输出 | P1 | [C0412][C0311] | 已核实 |
| G3 | 文件树、文件搜索、文件预览 | ⌘⇧E、⌘P，图片、PDF、Markdown 预览 | P1 | [D5][C0412] | 已核实 |
| G4 | 设置页基础 | General、Notifications、Appearance、Archived chats | P0 | [D7] | 已核实 |
| G5 | 亮色和暗色主题 | 基础主题切换 | P0 | [D7][C0611] | 推测（跟随系统未见官方说明） |
| G6 | 主题自定义 | 强调色、背景色、前景色，UI 和代码字体，主题分享 | P1 | [D7][C0312] | 已核实 |
| G7 | 默认快捷键全集 | 见第九节 | P0（核心子集）/ P1 | [D5] | 已核实 |
| G8 | 快捷键自定义 | 支持按键搜索和重置 | P2 | [D7][C0528] | 已核实 |
| G9 | 通知设置 | 完成通知分三档，审批和问题通知可单独开关，可选提示音 | P1 | [D26][C0922] | 已核实 |
| G10 | 技能管理页 | 浏览、创建、启用技能 | P1 | [D21][B1] | 已核实 |
| G11 | MCP 状态与配置 | `/mcp`，OAuth 认证 | P1 | [D6][D8] | 已核实 |
| G12 | 上下文和用量显示 | `/status` 显示上下文用量和限额 | P1 | [D6] | 已核实 |
| G13 | 插件目录与安装 | 插件市场 | P2 | [D22] | 已核实 |
| G14 | 定时任务 | Scheduled 收件箱，独立任务和会话内任务 | P2 | [D20] | 已核实 |
| G15 | 内置浏览器 | 预览、标注、Computer Use、开发者模式 | P2 | [D30] | 已核实 |
| G16 | Computer Use 和 Appshots | 操作桌面应用，截取前台窗口 | P2 | [D44][D45] | 已核实 |
| G17 | 生成文件预览 | 文档、表格、PDF、HTML、LaTeX | P2 | [D31] | 已核实 |
| G18 | 图片生成与 lightbox | 生成图片，Focused 和 Canvas 两种查看方式 | P2 | [C0730][C0505] | 已核实 |
| G19 | 记忆、语音、导入其他 agent 配置 | Memories、ChatGPT Voice、从 Claude Code 等导入 | P2 | [D7][C0723][D40] | 已核实 |
| G20 | 宠物和 Codex Micro | 悬浮动画宠物，实体控制器 | P2（不建议复刻） | [D27][D28] | 已核实 |
| G21 | 防止休眠 | 本地会话运行时阻止电脑睡眠 | P2 | [D7] | 已核实 |
| G22 | 手机远控 | 扫码配对，审批、查看 diff、新建任务 | 桌面阶段 P2（架构需预留，见第十四节） | [D33][D34] | 已核实 |

按上表，P0 共约 30 项，集中在三块：会话、输入框与消息流（第三节），审批（第四节），审查、提交与终端（第六、七节）。这与官方对 Codex app 核心价值的描述一致：并行会话、在会话里评审 diff、worktree [B1]。

### 十四、对架构的启示

本节从"桌面端先行、移动端平滑接入"的角度，说明上述发现对整体架构意味着什么，作为 PRD 产品-架构对齐会的输入。这里只讨论结构，不涉及具体技术选型。

1. Codex 自己就是"界面与执行分离"的先例。桌面 App 只是外壳，真正管理会话的是一个独立进程（app-server），双方用带类型的事件流协议通信。同一份协议通过 stdio 服务本机桌面，通过 SSH 服务远端主机，通过中继服务手机，在容器里服务 Web [B2][D33][G3][G4]。
   - 对 cupper 的含义：应该有一个"主机"层，负责持有会话、驱动 agent（通过 ACP）、持久化历史，并对外暴露一套客户端协议。桌面界面从第一天起就当作这个主机的一个客户端来写，不要在界面进程里直接持有 agent 会话。这样第二阶段接入移动端时，只需要给主机加一种传输方式，不必改造界面。
2. 要区分两套协议：cupper 与 agent 之间走 ACP；cupper 主机与各个界面之间走 cupper 自己的客户端协议。
   - Codex 的 item 模型可以作为客户端协议的参照，它基本对应了第三节的消息类型：userMessage、agentMessage、reasoning、plan、commandExecution、fileChange、mcpToolCall、webSearch、子代理、审查、压缩。生命周期统一为 started、delta、completed，服务端可以主动发请求，处理后发 resolved 通知 [D37]。
   - OpenAI 在 App Server 一文中提到，跨厂商的 harness 协议往往只能覆盖各家能力的公共子集 [B2]。这意味着通过 ACP 复刻 Codex UI，很可能有部分能力缺少协议支撑，需要由 R02 逐项核对，包括计划更新、整回合 diff、推理摘要、带范围的审批、Steer、分叉和回滚、压缩、审查模式、子代理线程。
3. 状态必须存在主机上，并且支持断线重连后追平。
   - Codex Web 的做法是：浏览器标签可能随时关闭，所以任务状态放在服务端，客户端重连后靠已保存的线程和事件流追上进度 [B2]。远程控制的信封带 seq_id 和 ack，用于重连后补发未确认的数据 [G4]。
   - 对 cupper 的含义：会话历史、未完成的审批、排队消息都要以主机为准，事件要带序号，支持从某个序号开始重放。移动端网络不稳定，这一点是硬性要求。
4. 审批要作为一等公民，并且能路由到任意一个客户端。
   - 手机端的核心用例之一就是审批 [D34]。Codex 的审批是服务端发起的请求，任一端处理后会广播 resolved [D37]。
   - 对 cupper 的含义：审批要能同时推送到桌面和手机，先到先得，处理后其他端自动收起；同时要和系统通知、推送通知打通 [D26]。
5. 会话状态模型应该统一定义在主机上。Codex 的侧边栏徽标、Activity 视图、通知、宠物、Codex Micro 和移动端，都基于同一组状态：空闲、运行中、需要输入、完成未读、出错 [D26][D27][D28]。cupper 应该在主机层定义这组状态，以及"未读"和"需要关注"的语义，所有客户端只负责渲染。
6. 很多核心界面功能其实不是 agent 的能力，而是客户端能力。
   - 审查面板的 Git 状态、暂存和提交、推送和 PR、worktree 的创建与移交、终端 PTY、文件树和预览、本地环境 actions，在 Codex 中由桌面 App 自己实现【推测，依据见第十二节】。
   - 但 Codex 的移动端后来也陆续加上了文件浏览、新建 worktree、运行 setup 脚本、行内审查评论 [W1]，说明这些能力最终都需要能被远程调用。
   - 对 cupper 的含义：Git、worktree、终端、文件这些操作应该实现在主机层，并通过客户端协议暴露出去，不要写死在桌面界面进程里。否则第二阶段移动端想用同样的能力，就得重写一遍。
7. agent 之间能力有差异，界面需要按能力降级。
   - 有些 Codex 界面功能依赖 Codex 自身 harness 的专有能力，比如 Approve for me 的审查 agent、Goal 模式、`/review` 审查器、云任务、Memories、`thread/fork` 的语义和子代理协作 [D18][D24][D37]。Claude Code 不一定有对应能力，或者语义不同。
   - 对 cupper 的含义：客户端协议需要按 agent、按会话做能力协商，界面根据能力显示或隐藏控件，而不是假定所有 agent 都一样。
8. 移动端与端到端加密。
   - Codex 手机远控的链路是：主机主动连接 OpenAI 的中继，信封里装 JSON-RPC 明文，依赖 TLS 保护传输；在已读的源码中没有看到应用层加密【推测】[G4]。配对使用二维码和配对码 [D33][G4]。
   - 对 cupper 的含义：信封层面的设计（client_id、stream_id、seq_id、ack、分片）可以借鉴，但载荷部分应该先加密，只让中继看到路由字段。二维码配对正好可以同时完成密钥交换。具体方案留给 R03 展开。
9. 主机应该能脱离桌面界面独立运行。
   - Codex 有一个守护进程（app-server daemon），可以运行在常开的机器或 SSH 主机上，带自动更新和远程控制开关 [G3]；文档也建议用一台常开电脑作为主机，并保持唤醒 [D33]。
   - 对 cupper 的含义：主机设计成可以独立运行的守护进程。第一阶段由桌面 App 拉起它，第二阶段打开远程传输，移动端就能直接连上。
10. 多个客户端之间要保持一致。Codex 支持多窗口、IDE 扩展与桌面 App 共享会话、桌面和 iOS 同步置顶会话 [C0416][D8][C0820]。cupper 的主机需要把项目、会话元数据（置顶、分区、未读、运行位置、关联的 worktree）同步推送给所有已连接的客户端。
11. 数据模型可以直接参照 Codex 的现有设计：
    - 项目：一个或多个文件夹，其中一个是主文件夹。
    - 会话：属于某个项目或不属于任何项目；记录运行位置（本机、worktree、远端主机）和关联的 worktree；状态包括置顶、未读、归档。
    - 回合和条目：见第二条中的 item 模型。
    - 审批请求：带范围（本次或本会话）。
    - 目标和计划：附属于会话。

    这些都在主机侧建模，桌面端和移动端共用 [D4][D13][D37]。
12. 关于范围：Codex 现在已经是一个超级 App（Chat、Work、Sites、Space、dots 等都在里面）。cupper 要复刻的是其中的 Codex 编码视图（见核心结论第 1 条）。建议 PRD 以第十三节的 P0 清单为 MVP 基线。对于云任务、浏览器、Computer Use 这类与 OpenAI 服务强绑定的功能，在架构上只保留扩展点，比如 Plugin Extensions 所代表的"插件可以加侧边栏、面板、编辑器"的思路 [W2]，不进入首期范围。

## 关键取舍

- 调研对象以"当前形态"为准。独立的 Codex app 已在 2026-07-09 并入 ChatGPT 桌面 App。报告以 2026-10-16 的 Codex 视图为准，同时用时间线交代它是怎么演变来的。这样既回答了"最新版本长什么样"，也为用户的待澄清问题 3（"Codex UI 指哪一个形态"）提供了事实依据。
- 官方资料优先，第三方只作补充。官方文档的全量导出（llms-full.txt，约 2.3 MB）和 changelog 是主要依据，开源代码用于核实技术线索。第三方逆向分析（Electron 版本、React、Tailwind 等）一律标为推测。
- openai.com 被 Cloudflare 拦截。改读 Wayback Machine 上同一 URL 的存档快照，存档日期已在 sources.md 中注明。
- 没有实际安装和运行 App，也没有截图：浏览器扩展未连接，而且安装和运行第三方 App 不在本任务的授权范围内。因此界面布局示意中的位置和比例、部分控件形态标为推测，视觉风格以官方的设置项和发布说明为主。
- 没有克隆 openai/codex 仓库，只通过 GitHub API 读取了与结论相关的少数文件（README 和 remote_control 相关源码），并记录了提交号，便于复查。
- 复刻优先级是按 cupper 的定位判断的（通过 ACP 接入 Claude Code 和 Codex，桌面先行、移动后接），不是按 Codex 自身功能的重要性排的。与 OpenAI 服务强绑定的功能一律放在 P2 或"不复刻"。

## 验证方式

对照 brief 的验收标准逐条检查：

| 验收标准 | 完成情况 |
|---|---|
| 功能清单表：功能 / 描述 / 复刻优先级建议（P0 / P1 / P2）/ 来源链接 | 已完成。第十三节共 7 张分表约 90 项，每项有优先级和来源编号，编号链接到原始 URL（文末链接定义和 sources.md），另附"核实"一栏 |
| 至少一条从新建会话到完成改动的完整交互流程描述 | 已完成。第十节共 14 步，从新建会话一直到合并和归档，每步附来源 |
| 视觉风格要点 | 已完成。第八节分为官方可核实部分和第三方推测部分 |
| 每条事实都有来源，区分"已核实"和"推测" | 已完成。正文默认已核实并附编号，推测内容用【推测】标出；功能表有单独的核实列；技术线索表逐行标注 |
| 注明资料的时效 | 已完成。第一节有时间线和时效说明；sources.md 记录了采集日期、最新版本号、每条资料的日期和存档时间戳、开源代码的提交号 |
| 「对架构的启示」一节 | 已完成。第十四节共 12 条，从桌面先行、移动端平滑接入的角度展开 |

自查项：

- 报告中引用的编号都能在文末链接定义和 sources.md 中找到对应 URL。
- 关键 URL 都在采集时实际访问过：文档用 curl 获取 `.md` 版本，均返回 200；博客通过 Wayback 存档读取；源码通过 `gh api` 读取。
- 本任务只产出文档，没有构建或测试步骤。

## 提交列表

```
a390c81 docs(task): add R01 source registry for Codex UI teardown
<编排者存档提交>
```

## 遗留问题 / 范围外发现

1. 视觉细节没有一手观察。没有安装 App 也没有截图，默认配色、字体族、间距、亮暗模式是否跟随系统等都属于推测。如果 PRD 需要像素级的视觉规范，建议由用户本人安装 ChatGPT 桌面 App，截取关键界面（新会话页、运行中的会话、审批卡片、审查面板、设置 → Appearance），存放到本目录或另开任务。
2. 需要 R02 核对 ACP 的覆盖面。第十二节给出了界面与 app-server 协议的对照表。R02 应逐项核对 ACP 和两个适配器能否提供：计划更新、整回合 diff、推理摘要、带范围的审批、Steer 和排队、分叉和回滚、压缩、审查模式、子代理线程、goal。
3. 一个可供 R02 评估的事实：openai/codex 采用 Apache-2.0，app-server 官方定位就是"嵌入到你自己产品里"的接口 [D37][G1]。接入 Codex 时，直接对接 app-server 和走 ACP 适配器是两条可比较的路线。本报告不做选型，只记录这一事实。
4. 需要 R03 确认 E2EE 结论。Codex 远控中继是否有应用层端到端加密，本报告只读了 remote_control 的部分源文件（protocol.rs、enroll.rs、websocket.rs、mod.rs），结论标为推测，需要 R03 通读确认。
5. 竞争态势值得在 PRD 中考虑。ChatGPT 桌面 App 已经支持从 Claude Code、Claude Cowork、Cursor 导入配置和最近会话，并且可以保持同步 [D40][C0811]。这对 cupper 的差异化定位（用户待澄清问题 2）是一个相关信号。
6. 可用性受限：很多功能受套餐、地区（如 EEA、英国、瑞士）和工作区策略限制。如果要拿 Codex 做体验对照，需要注意账号是否能看到全部功能。
7. 范围外的发现：openai/codex 仓库不接受外部代码贡献 [D38]。如果将来想向上游提改动，只能通过 issue。

[D1]: https://learn.chatgpt.com/docs/app
[D2]: https://learn.chatgpt.com/docs/quickstart
[D3]: https://learn.chatgpt.com/docs/use-chatgpt
[D4]: https://learn.chatgpt.com/docs/projects
[D5]: https://learn.chatgpt.com/docs/reference/commands
[D6]: https://learn.chatgpt.com/docs/reference/slash-commands
[D7]: https://learn.chatgpt.com/docs/reference/settings
[D8]: https://learn.chatgpt.com/docs/developer-settings?surface=app
[D9]: https://learn.chatgpt.com/docs/reference/troubleshooting
[D10]: https://learn.chatgpt.com/docs/code-review?surface=app
[D11]: https://learn.chatgpt.com/docs/integrated-terminal
[D12]: https://learn.chatgpt.com/docs/environments/modes
[D13]: https://learn.chatgpt.com/docs/environments/git-worktrees
[D14]: https://learn.chatgpt.com/docs/environments/local-environment
[D15]: https://learn.chatgpt.com/docs/cloud
[D16]: https://learn.chatgpt.com/docs/permission-modes
[D17]: https://learn.chatgpt.com/docs/sandboxing
[D18]: https://learn.chatgpt.com/docs/sandboxing/auto-review
[D19]: https://learn.chatgpt.com/docs/agent-approvals-security
[D20]: https://learn.chatgpt.com/docs/automations
[D21]: https://learn.chatgpt.com/docs/skills-and-plugins
[D22]: https://learn.chatgpt.com/docs/plugins
[D23]: https://learn.chatgpt.com/docs/agent-configuration/subagents
[D24]: https://learn.chatgpt.com/docs/long-running-work
[D25]: https://learn.chatgpt.com/docs/prompting
[D26]: https://learn.chatgpt.com/docs/notifications
[D27]: https://learn.chatgpt.com/docs/pets?surface=app
[D28]: https://learn.chatgpt.com/docs/features/codex-micro
[D29]: https://learn.chatgpt.com/docs/features/composer-predictions
[D30]: https://learn.chatgpt.com/docs/browser?surface=app
[D31]: https://learn.chatgpt.com/docs/artifacts-viewer
[D32]: https://learn.chatgpt.com/docs/image-inputs
[D33]: https://learn.chatgpt.com/docs/remote-connections
[D34]: https://learn.chatgpt.com/docs/mobile
[D35]: https://learn.chatgpt.com/docs/windows/windows-app
[D36]: https://learn.chatgpt.com/docs/linux/linux-app
[D37]: https://learn.chatgpt.com/docs/app-server
[D38]: https://learn.chatgpt.com/docs/open-source
[D39]: https://learn.chatgpt.com/docs/glossary
[D40]: https://learn.chatgpt.com/docs/import
[D41]: https://learn.chatgpt.com/docs/codex/ide
[D42]: https://learn.chatgpt.com/docs/codex/cli
[D43]: https://learn.chatgpt.com/docs/agent-configuration/speed
[D44]: https://learn.chatgpt.com/docs/computer-use
[D45]: https://learn.chatgpt.com/docs/appshots
[D46]: https://learn.chatgpt.com/guides/best-practices
[W1]: https://learn.chatgpt.com/docs/whats-new
[W2]: https://learn.chatgpt.com/docs/whats-new/devday-2026
[C]: https://learn.chatgpt.com/docs/changelog
[C0202]: https://learn.chatgpt.com/docs/changelog#codex-2026-02-02
[C0227]: https://learn.chatgpt.com/docs/changelog#codex-2026-02-27-app
[C0304]: https://learn.chatgpt.com/docs/changelog#codex-2026-03-04-app
[C0311]: https://learn.chatgpt.com/docs/changelog#codex-2026-03-11-app
[C0312]: https://learn.chatgpt.com/docs/changelog#codex-2026-03-12-app
[C0316]: https://learn.chatgpt.com/docs/changelog#codex-2026-03-16-app
[C0318]: https://learn.chatgpt.com/docs/changelog#codex-2026-03-18-app
[C0319]: https://learn.chatgpt.com/docs/changelog#codex-2026-03-19-app
[C0325]: https://learn.chatgpt.com/docs/changelog#codex-2026-03-25-app
[C0401]: https://learn.chatgpt.com/docs/changelog#codex-2026-04-01-app
[C0409]: https://learn.chatgpt.com/docs/changelog#codex-2026-04-09-app
[C0410]: https://learn.chatgpt.com/docs/changelog#codex-2026-04-10-app
[C0412]: https://learn.chatgpt.com/docs/changelog#codex-2026-04-12-app
[C0416]: https://learn.chatgpt.com/docs/changelog#codex-2026-04-16-app
[C0420]: https://learn.chatgpt.com/docs/changelog#codex-2026-04-20-app
[C0505]: https://learn.chatgpt.com/docs/changelog#codex-2026-05-05-app
[C0528]: https://learn.chatgpt.com/docs/changelog#codex-2026-05-28-app
[C0604]: https://learn.chatgpt.com/docs/changelog#codex-2026-06-04-app
[C0609]: https://learn.chatgpt.com/docs/changelog#codex-2026-06-09-app
[C0611]: https://learn.chatgpt.com/docs/changelog#codex-2026-06-11-app
[C0618]: https://learn.chatgpt.com/docs/changelog#codex-2026-06-18-app
[C0709]: https://learn.chatgpt.com/docs/changelog#codex-2026-07-09-app
[C0723]: https://learn.chatgpt.com/docs/changelog#codex-2026-07-23-app
[C0730]: https://learn.chatgpt.com/docs/changelog#codex-2026-07-30-app
[C0811]: https://learn.chatgpt.com/docs/changelog#codex-2026-08-11-app
[C0820]: https://learn.chatgpt.com/docs/changelog#codex-2026-08-20-app
[C0911]: https://learn.chatgpt.com/docs/changelog#codex-2026-09-11-app
[C0922]: https://learn.chatgpt.com/docs/changelog#codex-2026-09-22-app
[C1001]: https://learn.chatgpt.com/docs/changelog#codex-2026-10-01-app
[C1007]: https://learn.chatgpt.com/docs/changelog#codex-2026-10-07-app
[C1008]: https://learn.chatgpt.com/docs/changelog#codex-2026-10-08-app
[B1]: https://openai.com/index/introducing-the-codex-app/
[B2]: https://openai.com/index/unlocking-the-codex-harness/
[B3]: https://openai.com/index/chatgpt-for-your-most-ambitious-work/
[G1]: https://github.com/openai/codex
[G2]: https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md
[G3]: https://github.com/openai/codex/blob/main/codex-rs/app-server-daemon/README.md
[G4]: https://github.com/openai/codex/blob/main/codex-rs/app-server-transport/src/transport/remote_control/protocol.rs
[G5]: https://github.com/openai/codex/blob/main/codex-rs/cli/src/doctor/desktop/windows_security.rs
[T1]: https://simonwillison.net/2026/Feb/2/introducing-the-codex-app/
[T2]: https://www.kitze.io/posts/codex-electron-app-technical-breakdown
[T3]: https://cdn.jsdelivr.net/npm/codex-web-ui@0.1.1/README.md
[T4]: https://github.com/xdLawless2/codex-desktop-linux
[T5]: https://every.to/vibe-check/codex-vibe-check
[T6]: https://getpushtoprod.substack.com/p/complete-beginners-guide-to-openais
