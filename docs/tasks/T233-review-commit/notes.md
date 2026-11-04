# T233 工作笔记

- 执行者：子 agent（前端工程师，ui-core 与桌面；T130、T232 的执行者接手）

## 开工前核对到的现状

- 宿主（T123）：`git/commit {command_id, message, session_id}` → `{oid}`；`git/push {command_id, session_id, set_upstream?}` → `{branch, remote}`。失败按 reason 区分：`lock_held`、`nothing_to_commit`、`signing_failed`、`hook_failed`、`rejected`、`auth_failed`，超时或读 HEAD 失败是 `conflict/outcome_unknown`，其余是不带 reason 的 `git_error`；`data.stderr` 是脱敏后的 stderr 末尾。
- `git/status`：`branch`（detached HEAD 时没有）、`upstream`、`repo_config_changed`。没有上游时直接推送回的是不带 reason 的 `git_error`，所以默认带 `set_upstream`。
- 提交和推送不在主进程的真实输入名单里（只有 `interaction/respond`、`git/revert`），按普通命令发。
- ui-core 没有对话框组件。用原生 `<dialog>` 的 `showModal()`：顶层、遮罩、Esc 关闭、焦点留在对话框里，不加依赖。
- 头部（T232）已有"显示文件"和选项菜单；"Git 操作"菜单放在两者左边。T131 在改 `ReviewPanel.tsx` 的视图一块，这里只在 `Header` 里加一行，菜单和弹窗放在新文件 `review/GitActions.tsx`。

## 规模估计（停止线 500，硬停止线 600）

| 模块 | 内容 | 草估 |
|---|---|---|
| "Git 操作"菜单 | 提交、推送两项；detached HEAD 时推送不可用 | 25 |
| 提交弹窗 | 宽 420、标题、"提交到 {分支}"、提交信息框、三种下一步、"包含未暂存的更改"、hooks 说明、指纹提示、没有上游的说明 | 110 |
| 执行 | 暂存（取新的未暂存 diff 的令牌）→ 提交 → 推送；成功后关闭并提示，任何结果后都重新加载 | 45 |
| 失败指引 | 按 reason 给文案；stderr 折叠显示，只作文本 | 20 |
| 文案 | 约 30 个键 | 32 |
| 接线 | `Header` 一行 | 5 |
| **实现小计** | | **237** |
| UNIT-review-commit | 三种下一步、两步暂存与提交（含暂存成功提交失败）、有无上游、detached HEAD、各失败码的指引、stderr 作为文本 | 150 |
| UI-review-commit | 亮暗、带指纹提示、一张失败指引 | 50 |
| **测试小计** | | **200** |

合计约 437。每个模块做完跑一次 `cargo xtask size`，推算越过 500 就停下汇报。

## 实际规模

`cargo xtask size`：+549 −6（净 +543），超过停止线 500 共 49 行，在硬停止线 600 以内，已报编排者等决定。GitActions.tsx 263（估 200），UNIT-review-commit 208（估 150，其中约 60 行是与 review.test 相同的 provider 样板），UI-review-commit +46，文案 +29，ReviewPanel +3。
