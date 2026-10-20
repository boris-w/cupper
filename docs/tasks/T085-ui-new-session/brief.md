# T085 ui-core VI：新建会话页（M1a）

- 派发日期：2026-10-20
- 执行者：子 agent（前端工程师）。T082 的执行者接着做
- 分支：`feat/T085-ui-new-session`
- 依赖：T081、T086（已合并）。T016（契约冻结）、T093（交互与项目 e2e）依赖本任务

## 背景

新建会话页现在只有一个空的外框：`app/pages.tsx` 的 `NewSessionPage` 把 T086 的输入框放在视口高度 42% 处。`session/create` 的参数在 `composer/send.ts` 里写死成 `agent: "claude"`、`workspace: {kind: "local"}`，项目取路由里的 `projectId`，没有就取第一个项目。本任务按设计把这一页做完整（计划 T085 一行，桌面 #2）。

## 范围

**做**（设计 5.7.4 的"新建会话页"；5.7.10 的 `--text-heading-app`、`--color-composer-bar`）：
1. **大标题与项目选择器**：
   - 有项目时显示"你想在 {项目} 中构建什么？"，项目名本身就是选择器；没有项目时显示"我们要构建什么？"；
   - 选择器列出索引视图里的项目，未信任的照侧边栏标出。选中后改路由的 `projectId`，项目名经 `escapeInvisible`；
   - 列表末尾放"添加项目"，复用侧边栏的 `privileged({ kind: "add_project" })`；
   - 没有项目时输入框不能发送，这是 T086 现有的 `ready` 规则。
2. **输入框下方的动作条**（`--color-composer-bar` 底）：
   - **agent 选择**（AG-3）："Claude ⌄"放在最左边。M1a 只列 Claude，可用状态取 `agents/capabilities` 的查询结果，不可用时按语义码显示原因。Codex 在 M1b 加入，这次不列；
   - **工作区**：只显示"本地"。新建 worktree 和起始分支是 M1b 的事，这次不做。
3. **接线**：选中的 agent 和工作区经 `useSend` 传进 `session/create`，替换写死的值。会话创建后不能更换 agent。
4. 文案一律走 `@cupper/i18n`。

**不做**：
- 模型、推理强度、权限模式的选择（T141、T144）；
- 斜杠命令；
- Codex 的首页建议卡片、云端和远程运行位置、环境选择（设计写明是 P1 或不做）。

## 验收标准

- [ ] UI-new-session：有项目、没有项目、项目选择器展开、agent 下拉展开四种状态，亮暗两种主题；每个用例 CSP 违规为 0。
- [ ] 组件测试：选项目改路由；`session/create` 带上所选的 agent 和工作区；没有项目时不能发送；"添加项目"走特权路径。写在已有的 ui-core 测试里，或者在 `tests/ids.toml` 的 T085 段里补一个编号，并在汇报里说明。
- [ ] 审阅截图放在 `docs/tasks/T085-ui-new-session/screenshots/`，用系统字体，1280 × 820。
- [ ] UI-csp-zero 为 0；ESLint 规则（T136）通过。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T085 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T085-ui-new-session/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T085、T086、T081 的行）

- T086：输入框（`composer/Composer.tsx`、`composer/send.ts` 的 `useSend`），DOM 标记 `data-composer`、`data-send-state`；长文本经 blob（T164）。
- T081：侧边栏的项目列表、未信任标记、"添加项目"的特权调用（`sidebar/parts.tsx`）。
- T080：查询层（`state/query.ts` 已处理 `agents/capabilities` 的失效）。
- T136：样式与文案的静态规则。

## 注意

- 项目名、路径都是外部字符串：经 `escapeInvisible` 显示，不进样式。
- 不读本机的 Codex App 包。

## 规模

S。估约 300 行（页面与选择器约 120、动作条约 60、接线约 20、测试与截图用例约 100）。**开工前先在 `notes.md` 里按模块写行数估计；推算超过约 380 行（包括贴线）就先停下汇报**；硬停止线 420。

## 工作方式

在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b feat/T085-ui-new-session main`。`feat/T082-ui-stream` 已经合并，不要再往上提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
