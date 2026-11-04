# T131 审查文档（独立源 iframe）

- 派发日期：2026-11-04
- 执行者：子 agent（前端工程师，桌面与渲染进程安全）
- 分支：`feat/T131-review-document`
- 依赖：T104（K9 通过）、T130（审查面板主文档部分）。都已合并。T148 等 M1b 关键链依赖本任务

## 背景

K9 已判为通过（T104，`docs/tasks/T104-review-iframe/report-q29.md`）。审查面板的 diff 视图按设计 5.7.5 放进单独源的 iframe（`app://cupper-review/`），原样使用 @pierre/diffs。

- T130 已经在主文档里定好了视图接口（`packages/ui-core/src/review/view.ts` 的 `ReviewViewProps`、`ACTIONS`、`ViewTarget`），约定在 T130 汇报"给 T131 的 diff 视图接口"一节。
- T104 的原型（`spikes/T104-review-iframe/`）验证了协议注册、CSP、Worker、端口握手和 8 点。产品代码不依赖原型，按原型的结论在产品里重写。

## 范围

**做**（计划 T131 一行；设计 5.7.5"审查文档"、5.7.8）：
1. **第二个渲染入口**：
   - `app://cupper-review/`，独立 CSP（5.7.8 的表，不启用 Trusted Types）；
   - iframe 的 sandbox 与 `allow`；
   - `navigation.ts` 只放行一次审查文档的首次加载（原型有现成写法）；
   - `reviewDocumentUrl` 经 ShellAPI 下发，浏览器 mock 下审查文档也从第二个源、按生产 CSP 加载；
   - 新入口接入许可证门禁第一层。
2. **@pierre/diffs**：
   - 锁 1.6.4；
   - `workerFactory`，worker 入口 `@pierre/diffs/worker/worker.js`；
   - `shiki-js`，只加载 diff 里出现的语言；
   - Virtualizer 必须就是滚动容器（report-q29 第 6 节第 2 条）。
3. **版本化 MessageChannel 协议**：两侧都用 zod 校验，消息照 5.7.5 的表；审查文档只认第一个端口；主文档只认创建时的那个端口。
4. **主文档一侧的视图组件**：
   - 替换 `ReviewPanel.tsx` 里的占位 `FileList`，并删掉占位；
   - 把 `ReviewViewProps` 译成 `render`、`scroll_to`，把 `hunk_action`、`draft_comment` 译回 `act`、`comment`；
   - 按自己取到的 `file_diff` 校验 iframe 发来的 `hunk_ids`；
   - 空闲时预加载 iframe、预热 Worker，首次打开后常驻（隐藏而不销毁）。
5. **审查文档内**：
   - 无状态视图；
   - hunk 按钮（暂存、取消暂存、还原，按范围）；
   - 行内评论按 Codex 的交互：悬停行号出现"+"，评论框只写草稿，经 `draft_comment` 交给主文档；
   - Esc 关闭评论框，⌘Enter 保存草稿，其余按键经 `key_forward` 转给主文档；
   - 主题、语言、缩放经端口同步。
6. **还原**：hunk 和文件的还原只经主文档的确认控件（RP-11：不自动获得焦点，同一时间只显示一个，已有一个时忽略新的）。
7. **用例**（都已登记）：
   - E2E-SEC-review-iso（pr）：跨源、`parent.document` 抛错、别的端口发来的消息被忽略、iframe 的 `revert` 不直接执行、注入金样例；
   - E2E-review-comment（nightly）：评论作为附件随下一条消息发出，用户消息显示"{n} 条评论"；
   - E2E-SEC-revert-confirm（nightly）；
   - UNIT-review-injection。
   - 测试输入的两条路径要分开用，见 report-q29 第 6 节第 3 条。
   - 剪贴板按 report-q29 第 4 节第 6 条用 e2e 核实，实际读到的值写进汇报。
8. **截图基线**：T130 的 review 8 张、T232 的文件树几张，换掉占位后都要重新生成，只用 `ui-baseline` workflow。

**不做**：Git 操作菜单与提交弹窗（T233）；回合改动卡片（T132）。

## 开工前先估算（重要）

规模是 L，前几个 L 任务的估算都偏低过。**开工前**先在 notes.md 里按模块估行数（入口与 CSP、导航、协议与 zod、主文档一侧组件、审查文档、评论交互、各个用例），用 SendMessage 报给 `main`，等回复再写代码。超过停止线的话，提一个拆分方案。

## 验收标准

- [ ] 四个用例通过；新增的 E2E 在 runner 上连跑 10 次（由 `main` 触发 e2e-repeat）。
- [ ] 审查文档里的 `securitypolicyviolation` 为 0；新入口过了许可证门禁。
- [ ] 渲染进程的安全约束都守住：ui-core 不用 `innerHTML` 一类写法；文案走 i18n；agent 给的字符串不进入 `<style>` 和 CSS 变量；还原只经主文档的确认控件。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：1,100；硬停止线：1,300（开工前的估算报给 `main` 之后再定，可能拆分）

## 参考资料

- 设计：5.7.5（审查文档、消息协议、8 点）、5.7.8（CSP、Trusted Types、RP-11）、5.7.2（入口与协议注册）。
- 卷宗：T104（`report-q29.md`，尤其第 6 节）、T130（汇报里"给 T131 的 diff 视图接口"）、T232（文件树设置 `focus`）、T074（协议与 CSP）。
- 交接事项：`docs/tasks/handoff-notes.md` 里写给 T131 的条目。

## 注意

- 本机跑任何会开窗口的东西之前，先确认它不会抢焦点、不会发真实按键、不会动剪贴板；这类步骤只在 runner 上做（T104 出过一次）。
- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
