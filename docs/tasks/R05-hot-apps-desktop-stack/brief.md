# R05 热门 App 的桌面端技术栈（R04 补充）

- 派发日期：2026-10-16
- 执行者：子 agent（技术调研）
- 分支：`research/R05-hot-apps-desktop-stack`
- 依赖：R04（已合并）

## 背景

R04 调研了 40 多个编码 agent 客户端、ACP 客户端和参照用开发者工具的桌面框架（`docs/tasks/R04-desktop-framework-survey/report.md`）。用户补充要求：

> 包括manus。反正比较火热的app，他们的桌面端都是用什么做的

本任务补上 R04 没有覆盖的热门 App，作为编排者给出 ADR-03 结论的补充依据。

## 范围

- 做：查清以下 App 的 **macOS 桌面端**技术栈。R04 已经覆盖的不用重复，可以直接引用 R04 的结论。
  1. **AI 助手与 agent 类**：Manus、Perplexity（含 Comet 浏览器）、Gemini（如有桌面端）、ChatGPT Atlas、Genspark、Grok（如有）、Microsoft Copilot、Poe、Granola。
  2. **国内热门 AI 类**：豆包（含豆包浏览器或 PC 版）、Kimi、腾讯元宝、通义（千问）、DeepSeek（如有官方桌面端）、Trae、Qoder、CodeBuddy 等。
  3. **大众热门 App**：Figma、Discord、Slack、Spotify、Obsidian、Notion（主 App）、Telegram、WeChat（Mac 4.x）、飞书/Lark、钉钉、Zoom、Microsoft Teams（R04 已有迁移记录，补充现状即可）。

  每个 App 要写清：
  - 框架（Electron、CEF、Tauri、原生、Qt、自研等）和渲染方式；
  - 版本或时间点；
  - 安装包大小；
  - 有没有迁移过框架；
  - 证据和是否已核实。

  证据的优先级：本机已安装 App 的 bundle 结构（只读）> 官方说明或开源仓库 > 可信的拆解文章 > 第三方扫描。
- 不做：不重复 R04 的框架全景分析，不给 ADR-03 下结论。

## 允许的操作和约束

- 可以只读查看本机 `/Applications` 下已安装 App 的 bundle：
  - 目录结构、`Info.plist`、`Contents/Frameworks` 列表；
  - asar 的头部目录和 `package.json` 的依赖名；
  - 用 `strings` 或 `grep -a` 搜索二进制里的版本串。

  **不要运行这些 App，不要读取任何用户数据目录**（例如 `~/Library/Application Support`、`~/Library/Containers`），不要解包或修改 bundle。
- 不要下载或运行第三方安装包。安装包大小用 HEAD 请求或发布页面上的信息获取。
- 可以用本机已登录的 `gh` 做只读查询，不能做任何写操作。
- 遵守 `CLAUDE.md` 子 agent 守则第 7 条：外发请求里不能出现任何个人信息，User-Agent 用 `cupper-research` 这样的占位写法。
- 提交的内容里不能出现本机用户名或绝对路径（用 `/Applications/<App>.app` 这种写法）。**不要在仓库里列出用户本机装了哪些 App**，只写每个被调研 App 的结论，以及证据来自"本机 bundle"。

## 验收标准

- [ ] 一张表，每个 App 一行：类别 / App / 框架 / 渲染方式 / 版本或时间 / 包体 / 迁移史 / 证据 / 是否核实。
- [ ] 按框架分组的统计，并与 R04 的样本合并后给出整体分布。
- [ ] 3 到 5 条补充规律。重点关注两点：AI 新产品和国内大厂产品的选型倾向；与 R04 的规律是否一致。
- [ ] sources.md 中每条来源都写明日期和获取方式。

## 汇报要求

按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。sources.md 由执行者在任务分支上提交。
