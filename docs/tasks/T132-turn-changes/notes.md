# T132 工作笔记

- 执行者：子 agent（前端工程师，ui-core 与桌面；T130、T232、T233、T146 的执行者接手）

## 开工前核对到的现状

- 回合的快照在视图的 `turn.snapshots` 里（每个阶段一条 `snapshot.recorded`）；结束快照 `phase: end` 带 `stat`、`status`（`ok`、`partial`、`failed`、`skipped`）、`reason`、`excluded`。
- `git/diff {session_id, scope: {turn: id}}` 给这个回合的文件和 `diff_token`；宿主的 `git/revert` 接受回合范围的令牌。
- 审查面板的 store 现在由 `ReviewPanel` 自己建，消息流够不着；侧边面板的开合在 `LayoutStore`，也只有 `Shell` 拿得到。审查面板的范围只有"最近一轮""未暂存""已暂存"三种，没有某一个回合。
- 还原确认控件 `RevertConfirm` 的位置写死为审查面板底部；`useActions` 失败时弹 toast，`stale_diff` 的文案是审查面板的"变更已经更新，请再操作一次"，还没有 `lfs_object_missing` 的文案。
- E2E：假 agent 有 `write_file`；确认控件要真实输入，e2e 用 `sendInputEvent` 按键，测试前 `focusWindow` 会抢焦点，所以 E2E 只在 runner 上跑，本机不跑。

## 做法

- 审查面板的 store 提到 `Shell`，和一个 `openReview({turn, path})` 一起经 context 给消息流：设范围为这个回合、设 `focus`、打开侧边面板。
- 审查面板加第四种范围 `turn`（菜单里不列，由卡片打开），标题写"第 {n} 轮"，操作同"最近一轮"（还原；暂存按路径在未暂存上做）。切到别的会话时，范围从别的会话的回合退回"未暂存"。
- 卡片是新的行类型 `turn_changes`，排在回合最后，加进 `ALWAYS`。
- 撤销用 `useActions` 和 `RevertConfirm`（加一个排在卡片里的位置），焦点在"确认"（由主文档按钮触发，RP-11）；失败的文案由卡片给：`stale_diff` 为"之后已有新的改动，无法自动撤销"并带"打开审查面板"，`lfs_object_missing` 为"在终端里运行 `git lfs fetch`"。

## 规模估计（停止线 450，硬停止线 550）

| 模块 | 内容 | 草估 |
|---|---|---|
| 审查面板 store 上移与打开入口 | `Shell` 建 store，`openReview` 的 context | 30 |
| 回合范围 | 范围类型、`git/diff` 的参数、标题、`ACTIONS`、整组操作、空状态、换会话时退回 | 35 |
| 拍平与行的形状 | `turn_changes` 行排在回合最后、进 `ALWAYS` | 28 |
| 卡片 | 标题行、文件行（点了打开审查面板）、快照不完整的说明、撤销与失败提示 | 90 |
| 确认控件与失败文案 | 卡片里的位置；`lfs_object_missing` | 10 |
| 文案 | 约 12 个键 | 12 |
| **实现小计** | | **205** |
| 单测 | UNIT-stream-rows 补卡片的位置和折叠时也在 | 25 |
| UI-turn-changes | 卡片亮暗、折叠时也在、快照不完整、点文件打开审查面板 | 70 |
| E2E-turn-changes-undo | 假 agent 改文件，撤销走确认控件，文件还原 | 35 |
| **测试小计** | | **130** |

合计约 335。

## 编排者的裁定（2026-11-04）

- store 上移到 `Shell`、加 `openReview`；第四种范围 `turn` 不进菜单、头部写"第 {n} 轮"、只能还原。会和 T131、T238 冲突，T131 先合并，我在合并前 merge main 解决；`ReviewViewProps.scope` 多了 `"turn"`。
- 卡片的出现条件：`ok`、`partial` 且有改动；`failed`、`skipped`（不是 `not_a_repo`）也显示，写"这一轮的改动没能记录"，不提供撤销。
- E2E-turn-changes-undo 会抢焦点、发真实按键，只在 runner 上跑，由编排者用 e2e-repeat 连跑 10 次。

## 实际规模

`cargo xtask size`：+443 −35（净 +408），停止线 450 以内。实现约 240（卡片 141、拍平与行的形状 +30、回合范围与 store 上移 +55 −20、确认控件与失败的入口 +12、文案 +13），单测 +30，UI-turn-changes +118，E2E +41。
