# T141 配置选择：set_config 与模式选择器

- 派发日期：2026-11-03
- 执行者：子 agent（全栈：宿主编排与界面）
- 分支：`feat/T141-set-config`
- 依赖：T099（M1a 已退出）

## 背景

会话的模型、推理强度、审批档位（模式）现在只能在新建会话时定。设计已经定了运行中修改的做法：宿主的 `session/set_config` 经会话 actor 转给驱动的 `set_config`，用返回的最新配置写 `session.configured`（02 第 275 行）；放宽档位按 5.8.2 的档位规则判定，并记下 `mode_set_by`；完全访问只能由桌面主进程经原生确认框设置。驱动层（`DriverCommand::SetConfig`、`ClaudeSession`/`CodexSession` 的 `set_config`）和桌面主进程的特权操作 `set_config`（`apps/desktop/src/main/privileged/actions.ts`）都已经有了，宿主的协议方法和输入框底栏的选择器还没有。

## 范围

**做**（计划 T141 一行，第 3 节"T141（L）"的注）：
1. **宿主 `session/set_config`**：
   - 会话 actor 转给驱动，用返回的配置写 `session.configured`；回合进行中也允许改，由 agent 决定何时生效；
   - 档位规则（5.8.2 的表）：放宽按 principal 判定，降档只需要 write；记下 `sessions.mode_set_by`，经原生确认进入 `full_access` 时记下 `full_access_confirmed_at`；
   - 非档位的配置项按 5.8.3 矩阵的那一行；
   - 协议方法照 ADR-04 声明 `SCOPE`、`IDEMPOTENCY`，契约已冻结，只能附加，`schema-compat` 0 breaking。
2. **界面**（设计 5.7.4 的输入框底栏插槽）：
   - 权限按钮：图标加短名，下拉选择；
   - "{模型} {强度} ⌄"合一的选择器；
   - 完全访问走 `ShellAPI.requestPrivileged`，开启后输入框上方显示"完全访问权限已开启"；
   - `elevated` 档位给提示，不需要原生确认；
   - 文案走 i18n；截图基线如有变化，只用 `ui-baseline` workflow 生成（5.1.6，T143 的教训）。
3. 用例：
   - **IT-set-config-tier**（已登记）：档位规则逐格，加 `mode_set_by`；
   - **E2E-full-access-native**（已登记，nightly）：完全访问经原生确认框，开启后有提示。

**不做**：新建会话页的 agent 选择（T144）；设置页（T147、T151）；T143 留给 T141 的"change_model 恢复指引入口"可以顺带接上，接不上就列进汇报。

## 开工前先估算（重要）

规模是 L，T120、T121 的估算都偏低过。**开工前**先在 notes.md 里按模块（宿主、界面、用例）估行数，用 SendMessage 报给 `main`，等回复再写代码。超过 1,000 的话，提一个按"宿主 / 界面"拆开的方案。

## 验收标准

- [ ] 两个用例通过；E2E-full-access-native 在 runner 上能跑，附运行链接。
- [ ] `schema-compat` 0 breaking。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：1000；硬停止线：1200（开工前的估算报给 `main` 之后再定，可能拆分）

## 参考资料

- 设计：02 第 275–286 行（`session/set_config`、档位的设定者、回合中越权）、5.6.5（配置项）、06 第 133 行、08 的 5.8.2（档位规则表）与 5.8.3（权限矩阵）、07 的 5.7.4（输入框底栏）、5.7.8（原生确认框、`requestPrivileged`）。
- 计划：T141 一行，第 3 节的注，1.5（截图基线），1.6。
- 卷宗：T036（特权操作）、T055、T095（配置项与选项对照表）、T143（恢复指引的入口、截图基线的做法）、T226。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）；e2e 连跑放到 runner 上。
- 渲染进程的安全约束照旧（不用 `innerHTML` 一类写法；文案走 i18n）。
- 每条事件流只有一个写者；时间经 `Clock`。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
