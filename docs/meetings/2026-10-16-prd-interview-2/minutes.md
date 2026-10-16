# 需求访谈 #2

- 日期：2026-10-16
- 类型：需求访谈
- 参与者：用户、编排者
- 关联：阶段 ① PRD；接续 `2026-10-16-prd-interview-1`

## 议程

1. 用户回答关键的待澄清问题：定位、平台、截图
2. 解释 Claude 订阅与合规的关系
3. 处理其余待澄清问题

## 讨论要点

### 1. 用户的回答

> 1 开源和自用，直接用本地的claude cli没关系吧，跟订阅有什么关系
> 2 macos 和 ios
> 3 （提供了一张 Codex 界面截图）

### 2. 本地 Claude CLI 与订阅的关系

编排者的解释：

- **调用链**：cupper → ACP 适配器（claude-agent-acp）→ Claude Agent SDK → Claude Code 程序。适配器可以通过 `CLAUDE_CODE_EXECUTABLE` 指向用户本机安装的 `claude`。
- **账号**：Claude Code 运行时使用用户本机已有的登录凭据。如果用户用 Pro/Max 订阅登录过，那么 cupper 里的会话消耗的就是这个订阅的额度。这就是它和订阅的关系。cupper 本身不接触凭据。
- **政策原文**（R02 来源 S11，Claude Agent SDK 文档）："Unless previously approved, Anthropic does not allow third party developers to offer claude.ai login or rate limits for their products, including agents built on the Claude Agent SDK."
- **风险判断**（编排者的理解，不构成法律意见）：
  - 自用：用户在自己的电脑上，用自己的登录凭据运行官方 Claude Code，风险很低。
  - 开源分发：灰色地带在于 cupper 是否在"提供 claude.ai 登录"。规避办法如下：
    - cupper 不内置任何 Claude 或 ChatGPT 的登录流程，要求用户先自行安装并登录官方 CLI；
    - README 和宣传中不使用"用你的 Claude 订阅"之类的说法；
    - 产品名不包含 "Claude Code"；
    - 同时支持 API key 方式。
  - 如果以后要做大范围的公开分发，建议先向 Anthropic 确认。

结论：用户的方案（直接使用本地 CLI）可行，也就是 PRD 的账号方案。

### 3. 截图

- 截图内容是 ChatGPT 桌面 App 的 Codex 视图（macOS，中文，亮色）。原图含有个人和工作信息，项目又计划开源，所以原图只保存在本地被 git 忽略的 `.private/references/` 中，仓库里只提交观察笔记 `codex-screenshot-notes.md`。
- 截图补充了 R01 缺少的信息：最左侧的图标栏、权限模式位于输入框内部、右侧任务面板是浮动卡片、活动日志的样式、需要中文界面。

### 4. 定位带来的连锁调整

"开源 + 自用"意味着没有一个由我们运营的官方服务。因此问题 11（自托管与推送网关）的默认方案调整为：

- 中继由用户自己部署（开源，单个服务）。
- iOS App 由用户用自己的 Apple 开发者账号构建，推送使用用户自己的 APNs 凭据，由中继直接发出。
- 不再需要"发布者运营的推送网关"。推送内容仍然要加密。

### 5. 其余待澄清问题

用户这次没有逐条回答问题 2、3、5–11。为了不阻塞进度，编排者先按建议默认值写进 PRD，并在 PRD 中逐条标为"默认假设，待用户确认"，在 PRD 评审时统一确认。

## 决策

| # | 决策 | 理由 |
|---|---|---|
| D1 | 定位：开源 + 自用优先 | 用户回答 |
| D2 | 平台：桌面端 macOS，移动端 iOS | 用户回答 |
| D3 | 账号：复用本机已登录的官方 Claude Code 和 Codex CLI；cupper 不内置任何登录流程，产品名和宣传规避商标 | 满足用户诉求，同时把政策风险降到最低 |
| D4 | 中继自托管，推送使用用户自己的 APNs 凭据；不设官方服务 | 由 D1 推出 |
| D5 | 用户截图原图不进仓库，只提交脱敏后的观察笔记 | 项目要开源，截图里有个人和工作信息 |
| D6 | 其余问题先采用建议默认值，在 PRD 评审时统一确认 | 不阻塞进度 |
| D7 | 界面支持简体中文和英文 | 用户使用中文界面；开源项目需要英文 |

## 待办

| 事项 | 负责 | 关联 |
|---|---|---|
| 起草 `docs/01-prd.md` v0.1 | 编排者 | 阶段 ① |
| 召开产品-架构对齐会 | 编排者 + 架构师子 agent | 阶段 ① |
| 评审 PRD，确认各项默认假设 | 用户 | 阶段 ① 闸门 |

## 物料

- `codex-screenshot-notes.md`：截图观察笔记
- `.private/references/codex-ui-2026-10-16-main-light.png`：原图，只在本地，不提交
