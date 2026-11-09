# T151 设置页 I：agent 状态

- 派发日期：2026-11-09
- 执行者：子 agent（全栈：宿主索引流与 ui-core 设置页）
- 分支：`feat/T151-settings-agents`
- 依赖：T111、T125、T240、T241、T193（都已合并）。T147、T148 依赖本任务

## 范围

**做**（计划 T151 一行；设计 5.7.4 设置页、5.3.4；PRD AG-2、AG-5）：

1. **宿主**（附加式协议，契约已冻结）：
   - 索引流加 `settings.updated {settings}` 事件，索引视图加可选的 `settings`。附加式，后写覆盖先写，规则与 `host.updated` 相同。新加的 `Option` 字段要带 `skip_serializing_if = "Option::is_none"`。
   - `AgentsUpdated.git` 接上 T094 的 git、git-lfs 版本检测（v1.3.56 由 T036 移来）。
2. **设置页框架**，按设计 5.7.4 的 Codex 样式：
   - 占满主区，左侧导航和"返回应用"；
   - 内容区最大宽度 768；
   - 设置行与开关的规格。
   - T147 会在这个框架里加"通用""通知""外观""存储"等分页，框架要便于它直接加。
3. **agent 状态**：
   - CLI 路径与版本、已验证范围、登录状态、配置目录校验、运行时、资源占用；
   - 重启，调 `agents/restart`。
   - 错误条目恢复指引里的 `upgrade_cli`、`install_cli`、`restart_agent`，入口落在这一页（交接事项 T141、T151、T153 一行）。
   - 没装 Codex CLI 时显示为 `unavailable`，文案要对（交接事项 T111 一行）。
4. **开工前定下的两处**（2026-11-09）：
   - "返回应用"回到进设置页之前的那一页（会话页或新会话页），由路由记住来处；
   - 错误条目里 `upgrade_cli`、`install_cli`、`restart_agent` 三个按钮，跳到 agent 状态页里对应的那个 agent，为此改一点 T143 的 `rows/plan-error`。
5. **用例**：UI-settings-agents（已登记）。截图基线只由 `ui-baseline` 生成。宿主部分有对应的单元或集成用例。

**不做**：
- 连接与设备、已归档会话：开工前估约 640 行，拆到 T248（计划 v1.3.89）；
- 设置页的"通用、通知、外观、存储、命令行工具"（T147）；
- 会话管理界面和"打开"按钮（T153）；
- 默认编辑器的修正（交接事项 T126 一行，归 T153）。

## 验收标准

- [ ] UI-settings-agents 通过；宿主部分的用例通过。
- [ ] `schema-compat` 0 breaking；`settings.updated` 和 `IndexView.settings` 按附加通过（交接事项 T016 一行）。
- [ ] 渲染进程的安全约束都守住：文案走 i18n，agent 给出的字符串只作为文本显示。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：480；硬停止线：640（2026-11-09 开工前估约 640，按功能拆出 T248 后约 435；宿主部分做完 +228，推算整体约 620，不再拆：界面是宿主新事件的读者，硬停止线由 560 放到 640）。

## 参考资料

- 设计：5.7.4（设置页）、5.3.4（`settings/*`、`agents/*`、`session/list_archived`、`session/restore`）、5.3.7、5.2.12（supervisor、资源采样）。
- 交接事项：`docs/tasks/handoff-notes.md` 里接收任务含 T151 的各行。
- 卷宗：T094（git 版本检测）、T111（Codex）、T193、T240、T241。

## 注意

- 本机只单份跑截图用例，不抢焦点（守则第 10 条）。
- 不运行真实的 Claude Code、Codex CLI。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
