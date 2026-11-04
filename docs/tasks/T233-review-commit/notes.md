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

编排者接受超过停止线（硬停止线 600 不变），并要求把两份单测相同的 provider 样板抽成共用的 `review/test-harness.tsx`，`review.test.tsx` 也改用它，用例一条不砍。抽完后 `cargo xtask size`：+587 −60（净 +527），离硬停止线 600 还有 13 行；加数里含 `review.test.tsx` 改用共用辅助时重写的 19 行。

| 部分 | 估 | 实际 |
|---|---|---|
| GitActions.tsx（菜单、弹窗、执行、失败指引） | 200 | 263 |
| 文案 | 32 | +29 −1 |
| ReviewPanel 接线 | 5 | +3 |
| UNIT-review-commit | 150 | 175 |
| 共用的测试辅助 `test-harness.tsx`，`review.test.tsx` 改用它 | — | +52；+19 −54 |
| UI-review-commit | 50 | +46 −5 |

GitActions 超出估计，主要是弹窗的状态比估计的多（下一步、提交信息、包含未暂存、忙碌、失败、关闭方式）和说明注释。
