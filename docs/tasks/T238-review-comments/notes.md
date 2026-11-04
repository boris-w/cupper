# T238 notes

## 开工前的估算（2026-11-04）

叠在 T131 的分支上做。T131 已经有了端口协议、`ReviewFrame` 对 `hunk_action`、`draft_comment` 的校验，以及还原经 `askRevert` 进确认控件（来源为 `view`）。本任务要补的是审查文档里的交互和用例。

| 模块 | 实现 | 测试 |
|---|---|---|
| A. 协议与主文档：`files` 加回 `actions`、`drafts`，`ReviewFrame` 下发 | 10 | — |
| B. 文件头的按钮（按范围） | 20 | — |
| C. hunk 按钮：每个 hunk 末行挂一条 annotation，从 pierre 的 hunk 序号对回宿主的 `hunk_id` | 45 | — |
| D. 行内评论：pierre 自带的行号"+"（`enableGutterUtility` 和 `onGutterUtilityClick`，可拖选多行）、评论框（Esc 关闭、⌘Enter 保存）、引用的行文、已有草稿显示在行上 | 90 | — |
| E. 评论框里的按键不转发；文案 | 15 | — |
| F. e2e 钩子：复制一段文本（核实剪贴板用） | 5 | — |
| G. 用例：E2E-review-comment；E2E-SEC-revert-confirm；剪贴板核实（`@focus`，只在 runner 上跑）；截图工具里一个评论交互用例 | — | 200 |
| 合计 | ~185 | ~200 |

合计约 385，按以往偏差上浮后约 460，在停止线 500 之内。

## 实现记录

