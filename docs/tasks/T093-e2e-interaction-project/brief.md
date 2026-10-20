# T093 交互与项目 e2e

- 派发日期：2026-10-20
- 执行者：子 agent（前端工程师）。T084、T118、T128、T081、T082、T085 的执行者接手（熟悉外框、侧边栏、消息流和新建会话页的 DOM）
- 分支：`test/T093-e2e-interaction-project`
- 依赖：T083、T165、T085、T090、T167、T137（都已合并）。T099 依赖本任务

## 背景

e2e 设施（T090）和辅助函数（T167）都已就位；T167 还修掉了两处让真实 App 拿不到数据的缺陷，App 现在真正能跑。本任务在真实 App 上验证请求面板、提问面板、持久选项的原生确认、添加项目与信任、新建 Claude 会话。**只新增用例文件**。

## 范围

**做**（计划 T093 一行；设计 5.9.4、5.12.4，以及 5.7.4、5.7.8 中各条交互的规定）：
1. **E2E-approve-enter-esc**：请求面板的键盘与点击。覆盖 Enter、Esc、数字键、方向键，"跳过""关闭"的映射；点击走同一套映射。
2. **E2E-question-panel**：提问面板（单选、多选、文本），回答后宿主收到正确的答案。
3. **E2E-persistent-option-native**：持久选项（`allow_always` 类）走原生确认框；确认框由 T090、T167 的替换函数应答，取消和确认两个分支都要有。`ids.toml` 里标的是 `nightly`。
4. **E2E-project-add-trust**：
   - 经侧边栏或新建会话页的"添加项目"，原生选文件夹的对话框由 T167 的 `showOpenDialog` 替换函数应答；
   - 信任项目时走原生确认框，确认框里显示项目路径（T135）；
   - 未信任的标记随之消失。
5. **E2E-new-session-agent-claude**：在新建会话页选项目、"Claude"、"本地"，发出第一条消息，`session/create` 带上所选的 agent 和工作区，进入会话页。
6. **E2E-reject-with-message**（T165）：
   - 拒绝成功之后才发跟进消息，跟进带 `reply_to`；
   - CLI 先批准时，文字留在输入框，提示"已在别处处理"，接在原草稿之后；
   - M1a 一律排队。
7. **缺陷就地修**：按计划 1.2 的上限，单处修复不超过 100 行、不改接口，单独提交；超出的停下汇报。

**不做**：NFR-1 的五个用例（T091）；安全 e2e（T092）。

## 验收标准

- [ ] 六个编号全部通过。靠按键或点击算真实输入的用例打 `@focus`（T167 的交接），否则并行下会被别的 App 抢焦点。
- [ ] `ids.toml` 里标 `pr` 的用例：单个能在 `ci pr` 里较快跑完的打 `@smoke`；太慢的在汇报里提议改成 `nightly`，并给出实测时长。
- [ ] 连跑 10 次全绿（按 T167 的两段跑法）；跑完没有残留进程。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T093 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T093-e2e-interaction-project/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T093、e2e 的行；开工前逐行读一遍）

- T167：辅助函数（`helpers.ts`、`input.ts`、`cli.ts`、`dialogs.ts` 的 `showOpenDialog` 替换），`@focus` 分段，夹具的 PATH。
- T083、T165、T168：请求面板的 DOM 标记、自由输入行、截断内容。
- T086：输入框的 DOM 标记。
- T085：新建会话页（项目选择器、动作条）。
- T081、T135、T137：侧边栏的"添加项目"、信任确认框里的项目路径、会话页的未信任警告框。
- T077、T134：真实输入的规则、特权操作与原生确认框。

## 注意

- 不运行真实的 Claude Code、Codex CLI；agent 一律用假 agent。
- 不读本机的 Codex App 包；不读、不改用户的 `~/.claude`、`~/.codex`。

## 规模

M。**开工前先在 `notes.md` 里按用例写行数估计；推算超过约 850 行（包括贴线）就先停下汇报**；硬停止线 950。每完成一个用例统计一次（守则第 8 条）。

## 工作方式

在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b test/T093-e2e-interaction-project main`。`feat/T085-ui-new-session` 已经合并，不要再往上提交。e2e 第一次构建要跑 `runtime-pack` 和 `build:e2e`，会慢一些。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
