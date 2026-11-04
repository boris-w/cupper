# T231 配置选择 II：权限按钮与模型选择器

- 派发日期：2026-11-04
- 执行者：子 agent（前端工程师，ui-core 与桌面）
- 分支：`feat/T231-config-selectors`
- 依赖：T141（宿主的 `session/set_config`，已合并）

## 背景

T141 执行中按"宿主 / 界面"拆开（计划 v1.3.86）：宿主的 `session/set_config` 已合并（PR #57），本任务做界面那一半。宿主已就绪：普通档和 `elevated` 由窗口直接发 `session/set_config`；完全访问走 `requestPrivileged({kind: "set_config", …})`，主进程经 `policy/describe` 和原生确认框后在 control 连接上发出。T141 开工前的估算里，界面约 370 行（hook 40、权限按钮 95、选择器 60、完全访问提示 20、图标与文案 25、组件测试 130），E2E 约 50。

## 范围

**做**（计划 T231 一行，设计 5.7.4 的输入框底栏）：
1. 输入框底栏插槽里的权限按钮：图标加短名，下拉选择。
2. "{模型} {强度} ⌄"合一的选择器。
3. 完全访问走 `requestPrivileged`，开启后输入框上方显示"完全访问权限已开启"；`elevated` 档位给提示，不需要原生确认。
4. 读能力和发命令的 hook；选项只取 agent 能力里有的。
5. T143 留下的 `change_model` 恢复指引入口：错误框里的"换模型"按钮打开模型菜单（约 20 行）。
6. 用例：组件测试覆盖两个选择器、完全访问的流程、`elevated` 提示；**E2E-full-access-native**（已登记，nightly）：完全访问经原生确认框，开启后有提示。
7. 附复刻度审阅截图：e2e 构建、系统字体，截输入框底栏两个选择器展开时的样子，放进卷宗，不进基线。需要在 runner 上截的话告诉 `main`。

**不做**：新建会话页的选择器（T144）；设置页（T147、T151）。

## 验收标准

- [ ] 组件测试和 E2E-full-access-native 通过，E2E 在 runner 上的运行附链接。
- [ ] 渲染进程的安全约束都守住；文案走 i18n。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：550；硬停止线：650

## 参考资料

- 设计：07 的 5.7.4（输入框底栏）、5.7.8（原生确认框、`requestPrivileged`）、08 的 5.8.2（档位规则）。
- 卷宗：T141（report、review；宿主接口）、T143（恢复指引的入口、截图基线的做法）、T036（特权操作）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）；e2e 连跑放到 runner 上。
- `ui-core` 不 import `electron`、`node:*`；渲染进程不用 `innerHTML` 一类写法。
- 截图基线如有变化，只用 `ui-baseline` workflow 生成。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
