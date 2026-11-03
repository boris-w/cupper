# T143 消息流增强 I：代码高亮、计划与恢复指引

- 派发日期：2026-11-03
- 执行者：子 agent（前端工程师，ui-core 消息流）
- 分支：`feat/T143-stream-highlight-plan`
- 依赖：T099（M1a 已退出）。T146（消息流增强 II）排在本任务之后；T103（SP-6 场景 1–4、7、8）依赖本任务

## 背景

M1a 的消息流（T082 行列表、T166 消息行与 Markdown）已经能显示 agent 的回答、计划条目和错误条目，但代码块没有高亮，计划只是原样列出，错误条目没有恢复指引的按钮。设计 5.7.4 的"计划""错误和提示"、5.7.10 的代码高亮配色、MS-1、MS-4、MS-5、F3 已经定了样子。

## 范围

**做**（计划 T143 一行，5.7.4 消息流，5.7.10）：
1. **代码高亮**（MS-1）：
   - Shiki 懒加载，代码块完成后再高亮（5.7.6 长会话一行），流式期间不反复高亮；
   - 双主题，只取 5.7.10 代码高亮表里的七个语法类别；
   - 接进现有的 Markdown 渲染（`packages/ui-core/src/rows/markdown.tsx`，streamdown）。
2. **计划**（MS-4）：
   - 计划条目：编号步骤，每步一个 16 的状态图标（未开始、进行中、完成）；
   - Markdown 计划显示为"计划"摘要条，可以展开；
   - **计划药丸**（F3）：回合运行中有 `plan` 条目时，输入框上方浮一枚药丸"第 n / m 步"，n 是进行中的那一步，点击后滚动到计划条目；放在 `<ComposerOverlays/>`，用 `--shadow-pill`。
3. **恢复指引**（MS-5）：错误条目用警告框样式，`ErrorItem.guidance`（`retry`、`run_login_in_terminal`、`upgrade_cli`、`install_cli`、`quota_exhausted`、`change_model`、`restart_agent`、`none`）映射成框里的按钮；未知取值按开放枚举兜底。按钮做什么，按 5.7.4 和已有的界面入口来定；还没有入口的（例如设置页），先说明，不在本任务造新入口。
4. 用例：UI-code-highlight、E2E-error-guidance、UI-plan（含计划药丸）、UI-error，都已登记在 `tests/ids.toml`。需要别的新编号先告诉 `main`。

**不做**：活动组折叠、晚到输出等（T146）；diff 的高亮（审查文档的 Worker，T130 起）；设置页（T147、T151）；改宿主下发的语义码。

## 验收标准

- [ ] 四个用例通过；E2E-error-guidance 在 nightly 路径上能跑（`〔晚〕`），附运行链接。
- [ ] 渲染进程的安全约束都守住（见"注意"）。
- [ ] 截图基线如有变化，按计划 1.5 的规则处理，在汇报里列出。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：900；硬停止线：1100

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.7.4（消息流：计划、错误和提示；主题）、5.7.6（长会话：Shiki 懒加载）、5.7.8（安全加固：CSP、Trusted Types）、5.7.10（代码高亮表、`--shadow-pill`）、12-milestones 的 MS-1、MS-4、MS-5、F3。
- 计划：T143 一行，第 3 节"T143、T146"的注，1.5（截图基线），1.6（编号）。
- 卷宗：T082（行列表）、T166（消息行与 Markdown、UNIT-md-injection）、T090–T093（请求面板，`plan_confirm`）。

## 注意

- **渲染进程不用 `innerHTML`、`dangerouslySetInnerHTML`、`insertAdjacentHTML`、`eval`、`new Function`**。Shiki 的 HTML 输出（`codeToHtml`）不能直接插进页面；用 token 接口渲染成 React 元素之类的做法。
- CSP 不放宽。Shiki 默认的正则引擎是 WASM；如果需要放宽 CSP（例如 `wasm-unsafe-eval`）才能用，先停下来报 `main`，说明可选方案（例如 Shiki 的 JavaScript 正则引擎）。
- agent 给出的字符串不能进入 `<style>` 文本、`insertRule` 或 CSS 自定义属性；颜色只来自 5.7.10 的固定表。
- 新增依赖要过 license 检查（Apache-2.0 兼容），锁精确版本。
- `ui-core` 不 import `electron`、`node:*`；文案一律走 `@cupper/i18n`。
- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）；不跑性能或高负载测试。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
