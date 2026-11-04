# T233 审查面板 III：Git 操作与提交弹窗

- 派发日期：2026-11-04
- 执行者：子 agent（前端工程师，ui-core 与桌面）
- 分支：`feat/T233-review-commit`
- 依赖：T130（审查面板主体）、T123（`git/commit`、`git/push`），都已合并；T232（文件树）也已合并

## 背景

T130 开工前估算后拆成三份（计划 v1.3.87），"Git 操作"菜单与提交弹窗拆到本任务。宿主一侧已经齐了：
- `git/commit`、`git/push`（用户档）都已就绪（T123）；
- `git/status` 带 `repo_config_changed`；
- 失败时给出 `hook_failed`、`nothing_to_commit`、`signing_failed`、`auth_failed`、`rejected`、`lock_held` 等 reason，`data.stderr` 里有脱敏后的 stderr 末尾。

T131（审查文档）正在并行开发，它会替换 `ReviewPanel.tsx` 里的占位视图。本任务改的是头部和弹窗，两边尽量不碰同一处；谁后合并，谁 merge main 解决冲突。

## 范围

**做**（计划 T233 一行；设计 5.7.5"提交"、5.2.10"提交与推送"、Q23）：
1. **"Git 操作"菜单**：放在审查面板头部，M1 的项是"提交""推送"；Codex 的"创建分支""切换分支""创建 PR"是 P1，不做。
2. **提交弹窗**（5.7.5）：
   - 宽 420，标题"提交或推送"，写明"提交到 {分支}"；
   - 提交信息框高 80；
   - 下一步三选一：提交、提交并推送、推送；
   - 复选框"包含未暂存的更改"：勾选时先对"未暂存"范围执行 `git/stage`（目标为全部），再 `git/commit`；暂存成功而提交失败时，改动留在已暂存里；
   - 按钮旁注明"将运行仓库的 hooks"；
   - `git/status` 有 `repo_config_changed` 时，确认文案带上指纹提示，列出路径。
3. **推送**：没有上游时默认带 `set_upstream`；detached HEAD 时不提供推送。
4. **失败的指引**：按语义码给出，文案走 i18n：
   - `signing_failed`：改用图形版 pinentry（pinentry-mac）；
   - `auth_failed`：检查 ssh agent 或凭据（只在 shell 配置里 export 的 `SSH_AUTH_SOCK` 从 GUI 启动时看不到）；
   - `hook_failed`：hook 拒绝；
   - `nothing_to_commit`、`rejected`（先拉取再推送）、`lock_held`、`outcome_unknown`（刷新后由用户决定）。
   - `data.stderr` 可以折叠显示，只作为文本节点渲染。
5. **用例**（都已登记）：
   - UNIT-review-commit：组件测试，覆盖三种下一步、"包含未暂存的更改"的两步、有无上游、各个失败码的指引；
   - UI-review-commit：弹窗的截图（亮暗、带指纹提示）。基线只用 `ui-baseline` workflow 生成，逐张看过再提交。
6. 附复刻度审阅截图（e2e 构建、系统字体，不进基线）。

**不做**：生成提交信息、创建 PR（GT-6，P1）；宿主一侧的改动。

## 验收标准

- [ ] 两个用例通过；现有基线中只有确实受影响的那几张改变，汇报里逐张说明。
- [ ] 渲染进程的安全约束都守住（不用 `innerHTML` 一类写法；文案走 i18n；stderr 只作为文本渲染）。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：500；硬停止线：600

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.7.5（提交）、5.2.10（提交与推送，T123 落实的细节）、5.3.4（`git/commit`、`git/push`、`git/status`）、13 的 Q23。
- 卷宗：T123（汇报里的失败分类和 Q23 的说明）、T130（汇报、审查面板的结构）、T232（头部的"显示文件"按钮）。
- 交接事项：`docs/tasks/handoff-notes.md` 里写给 T233 的条目。

## 注意

- 本机跑任何会开窗口的东西之前，先确认它不会抢焦点、不会发真实按键、不会动剪贴板。
- 测试只用 mock 宿主或临时目录的仓库，不推送到任何真实的远端。
- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
