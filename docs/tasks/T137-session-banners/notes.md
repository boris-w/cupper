# T137 工作笔记

## 开工前的核对

- `handoff-notes.md` 里第一列含 T137 的行：T080 的状态层接口（`useStreams().session(id)`、`useEntry`）。任务书的交接事项另有 T081（`usePrivileged`、截图页的索引流夹具）和 T134（`confirm_config` 的确认框内容由主进程从会话流取）。
- `tests/ids.toml` 的 T137 段已有 UI-session-banners、UNIT-ui-session-banners。
- 数据：会话所在项目的信任状态在索引流（`sessions[id].project_id` → `projects[...].trust_state`）；`confirmation_required`（`config_digest`、`reason`）在会话流 `configured` 里，`SessionView` 改它时 touch 的键是 `session`。
- 文案依据：5.2.6"未信任或 `stale` 时……界面显示'未信任：项目配置未加载'"（Claude 只加载用户级配置，Codex 不能使用）；5.8.4 的横幅是"命令行工具申请权限"。确认原因的说法对照 `privileged` 命名空间里确认框的文案。
- 会话页现在没有拿到 `sessionId`（`Page` 只用它做 `key`），要改成传进去。

## 规模估计（开工前）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 文案 | 两种未信任说明（`untrusted`、`stale`）、"信任此项目"、三种确认原因、"确认"；提权横幅改通用文案 | 12 | — |
| 警告框 | "未信任"（`trust_project`）、"配置待确认"（`confirm_config`）按 props 画；接索引流与会话流的外层；会话页传入 `sessionId` | 75 | — |
| 两处修正 | 提权横幅不再显示自述名字；"未信任"标签右对齐（悬停按钮改为浮在行尾） | 8 | — |
| 截图页夹具 | 把 `index` 推广成按流名给 op 的 `streams` | 8 | — |
| UI-session-banners | 两种警告框、两种主题 | — | 40 |
| UNIT-ui-session-banners | 出现与消失的条件、两个按钮的 `requestPrivileged`、提权横幅不显示自述名字 | — | 70 |
| 合计 | | 103 | 110 |

合计约 **215 行**，上限约 300。
