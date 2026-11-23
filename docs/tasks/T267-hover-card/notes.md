# T267 笔记

## 估算与实际（`cargo xtask size`，新增行）

| 模块 | 估算 | 实际 |
|---|---|---|
| M1 悬停卡片 + M2 标题转义 | 150 + 20 | 158（一个提交：卡片里的标题本身就要转义） |
| M3 `opener` 替身 | 15 | 15（累计 173） |
| M4 两个 E2E | 130 | 98（累计 271） |

## 取舍

- 不加单独的归档按钮：设计 5.7.4 定的是悬停时行尾只出菜单按钮；固定是 P1，宿主也没有方法（编排者 2026-11-23 同意）。卡片没有可聚焦元素，不改 Tab 顺序（T270）。
- 行首错误图标上的 Tooltip 去掉，原因并进卡片的状态行：两个 Tooltip 嵌在一起会同时弹出。
- worktree 会话经 `security.ts` 已有的 `asWindow`（以窗口身份调 `session/create`）建，不再把 `notification.spec.ts` 的 `asHarness` 挪进 helpers：窗口本来就能建会话，少一套连接。
- ⌘⇧A 经菜单项的加速键（`input.ts` 的 `shortcut`），命令发给获得焦点的窗口，所以 E2E-archive-cleanup 标 `@focus`；E2E-workspace-open 不需要焦点。
- 相对时间用 `Intl.RelativeTimeFormat`（`numeric: "auto"`），中文是"5分钟前""昨天"，没有 Codex 那样的空格。
