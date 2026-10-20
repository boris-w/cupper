# T081 ui-core III：侧边栏、横幅与 toast

- 派发日期：2026-10-20
- 执行者：子 agent（前端工程师）。T084、T118、T128 的执行者接手
- 分支：`feat/T081-ui-sidebar`
- 依赖：T084、T118、T128（已合并或合并中）；T080（状态层，并行进行中，见"工作方式"）。T090 依赖本任务；**本任务在剩余关键路径上**：T080 → T081 → T090 → T091 → T099

## 背景

外框、token、插槽（T084）、截图设施（T118）、基础组件与行类型注册表（T128）都已就位。T080（流 store、订阅策略、zustand、查询）正由另一个执行者在做。

本任务按 Codex 的样式实现侧边栏、各类横幅与警告框、toast，接到 T084 留的 `sidebarHeader`、`sidebarList`、`sidebarFooter` 三个插槽上。

## 范围

**做**（计划 T081 一行；设计 5.7.4 的侧边栏、窄窗口、toast，5.7.5，5.7.10，5.12.2 的界面部分；ui-spec 4.1）：
1. **侧边栏**：
   - 头部：产品名、宿主状态点。替换 T084 的默认头部。
   - "新会话"一行。
   - 项目分组："未信任"标记、悬停时的"+"和菜单、展开显示。
   - 会话行：高 30。显示状态的转圈、未读蓝点、"等待批准"和"需要用户输入"两种标签（按 `pending_kind`）、错误图标。
   - 宽度拖动与窄窗口浮层已由 T084 做好，本任务只需确认内容在两种模式下都正常。
2. **横幅与警告框**：
   - 提权横幅：点"查看"调 `ShellAPI.requestPrivileged({kind: 'elevation', …})`（T134 已接上）；
   - 添加项目：调 `requestPrivileged({kind: 'add_project'})`；
   - 会话页上方的"未信任""配置待确认"警告框，"配置待确认"的确认走 `requestPrivileged({kind: 'confirm_config'})`；
   - 重连状态。
   - 弱认证标记已在顶栏（T128），本任务不重复做。
3. **Toaster**：顶部居中，最多 3 条，位置按 `--layout-toast-top`、`--layout-composer-adjacent-max-w`。
4. **数据**：都来自索引流，经 T080 的 store 和 hook 取：
   - 会话摘要（`status`、`pending_kind`、未读、`error_code`）；
   - 项目的信任状态；
   - `clients`；
   - 宿主状态；
   - 提权申请（`elevation.requested`、`elevation.resolved`）。
5. 文案走 `@cupper/i18n`；组件只引用 token，基础组件用 T128 的。

**不做**：
- 新建会话页（T085）；
- 会话行和"⋯"菜单里的动作（T153）；
- 设置页（T036）。

## 验收标准

- [ ] UI-sidebar：会话行的各种状态，以及 `pending_kind` 的两种标签；两种主题；窄窗口浮层。
- [ ] UI-banners：提权横幅、"未信任"与"配置待确认"警告框、重连状态、toast 的位置（最多 3 条）。
- [ ] 截图用例照 `tools/ui-screenshots` 的 notes 写，数据经 mock 宿主的索引流夹具给出。
- [ ] 组件的交互（点"查看"、添加项目、确认配置会调用对应的 `requestPrivileged`）有 Vitest 用例。
- [ ] UI-csp-zero 照常为 0。
- [ ] 汇报附复刻度审阅截图：系统字体、固定窗口尺寸（计划 1.11）；写明按 F2 查看过本机 Codex App 包的哪些资源（如果看过）。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T081 段里（另需的单元测试编号补进该段）。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T081-ui-sidebar/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T081 的行；开工前逐行读一遍）

- T084：插槽的名字；token 写法；用户状态只经 `style={{...}}` 写入。
- T118：截图用例的加法。
- T128：基础组件（工具栏按钮、菜单、tooltip、开关、警告框）。
- T134：`requestPrivileged` 的各种 kind 和返回值（`confirmed`、`cancelled`、`busy`、`cooldown`、`user_activation_required`、`failed`）。界面要给每种返回值合适的反馈，例如 `cooldown` 显示还要等多久。

## 注意

- F2：只复刻，不复制；图标用开源图标库或自绘；查看本机 Codex App 包只读、不启动、解包只放 `$TMPDIR` 用完删除。
- agent 给出的字符串（会话标题等）只作为文本节点渲染，不进样式。
- T136（ESLint 安全与文案规则）在并行。规则合并后，本任务的代码要能通过，所以现在就按 `CLAUDE.md` 的代码约束写：不用 `innerHTML` 一类的 API，文案不写字面量。

## 规模

计划估为 M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项；推算超过约 900 行（包括贴线）就开工前停下汇报，并给出拆分建议。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

## 工作方式

- T128 合并之后，在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b feat/T081-ui-sidebar main`。T128 的分支由编排者合并，不要再往它上面提交。
- **T080 还在进行中**。先做不依赖它的部分：
  - 组件按 props 写；
  - 截图用例用 mock 宿主的夹具；
  - 横幅和 toast 的交互。
- T080 的 store 和 hook 形状定下后，可以在 T054 worktree 里只读查看它分支上的 `notes.md`。不要合并它的分支。
- T080 合并进 main 后，由编排者把 main 合进你的分支并通知你，你再把组件接到 store 上。在那之前不要执行 merge 或 rebase。
- 不依赖 T080 的部分做完了而 T080 还没合并时，停下来汇报进度，等通知。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
