# T137 ui-core III-b：会话页警告框

- 派发日期：2026-10-20
- 执行者：子 agent（前端工程师）。T084、T118、T128、T081 的执行者接手
- 分支：`feat/T137-session-banners`
- 依赖：T080、T081（已合并或合并中）。T093（交互与项目 e2e）依赖本任务

## 背景

T081 开工前拆出了会话页上方的两个警告框（计划 v1.3.29）。它们的数据在会话流里，位置在会话页，和侧边栏关系最弱。

T081 的评审另外发现两处要修，一并放进本任务（见范围第 3、4 项）。

## 范围

**做**（计划 T137 一行；设计 5.7.4、5.7.5 的 `<SessionBanners>`、5.2.6 的项目信任与配置确认）：
1. **"未信任"警告框**：会话所在的项目不是 `trusted`（含 `stale`）时，显示在会话页上方。文案说明"项目配置未加载"一类的后果（对照 5.2.6：Claude 只加载用户级配置）。提供"信任此项目"，走 `requestPrivileged({kind: 'trust_project', project_id})`。
2. **"配置待确认"警告框**：
   - 数据：会话流 `session.configured` 的 `confirmation_required`（原因、实际档位等），从 `useStreams().session(id)` 读；
   - 提供"确认"，走 `requestPrivileged({kind: 'confirm_config', session_id})`；
   - 确认后警告框随流的变化消失，不在本地先隐藏。
3. **修正 T081 的提权横幅**：
   - 现在横幅显示的是申请方自述的 `client_name`（例如"cupper-cli 申请更高的权限"），这个名字未经核实，可以被伪造成误导性的名称。
   - 改为设计 5.8.4 时序图里的通用文案"命令行工具申请权限"。核实过的信息只在原生确认框里出现。
   - 如果确实要显示名字，必须同时标明"未经核实"，并在汇报里说明理由。
4. **修正 T081 的"未信任"标签对齐**：项目行里的"未信任"停在行中间，要和会话行尾的"等待批准""需要用户输入"一样右对齐。
5. 警告框用 T128 的 `Alert`；特权操作的反馈用 T081 的 `usePrivileged`；文案走 i18n。

**不做**：消息流（T082）；输入框（T086）；会话行与项目的菜单（T153）。

## 验收标准

- [ ] UI-session-banners：两种警告框；两种主题。截图页的会话流夹具照 T081 加索引流夹具的做法。
- [ ] UNIT-ui-session-banners：
  - 警告框出现与消失的条件（信任状态、`confirmation_required`）；
  - 两个按钮调用对应的 `requestPrivileged`；
  - 提权横幅不显示自述的名字。
- [ ] UI-sidebar、UI-banners 的基线因为第 3、4 项会变：重新生成、逐张看过，在汇报里说明哪几张变了。
- [ ] UI-csp-zero 照常为 0；ESLint 规则（T136）通过。
- [ ] 汇报附复刻度审阅截图（系统字体、1280 × 820）。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T137 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T137-session-banners/ci/` 通过，摘要随任务提交。

## 交接事项

- T081：`usePrivileged` 已有 `trust_project`、`confirm_config` 的反馈；截图页的索引流夹具。
- T080：`useStreams().session(id)`、`useEntry`。
- T134：`confirm_config` 的原生确认框内容由主进程从会话流取，界面只发 `requestPrivileged`。

## 注意

- agent 和申请方给出的字符串只作为文本节点渲染，不进样式；自述的内容不能被呈现得像核实过的信息。
- F2：只复刻，不复制。

## 规模

S，上限约 300 行（含两处修正）。**开工前先在 `notes.md` 里按模块写行数估计**；推算超线就先停下汇报。

## 工作方式

T081 合并之后，在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b feat/T137-session-banners main`。T081 的分支由编排者合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
