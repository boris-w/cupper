# T126 文件搜索、提示内容映射与在外部应用打开

- 派发日期：2026-11-03
- 执行者：子 agent（Rust 工程师，宿主文件服务与编排）
- 分支：`feat/T126-files-prompt-open`
- 依赖：T099（M1a 已退出）

## 背景

输入框的 `@` 引用、图片、长文本、审查评论，在协议里都是 `PromptContent` 的内容块（03 第 171 行：`text`、`image {blob}`、`file_ref {path}`、`review_comment {…}`、`text_blob {blob}`）。宿主要把它们映射成 ACP 的内容块再交给 agent。`@` 引用的候选来自 `files/search`。UI-0 里"在终端或编辑器里打开工作区"走 `workspace/open`。协议方法在 M1a 已冻结（`files/search` scope read，`workspace/open` scope ui_local），宿主还没有实现。

## 范围

**做**（计划 T126 一行，设计 5.2.11 的"文件服务"，5.3.4）：
1. `files/search`：
   - git 仓库在快照档执行 `ls-files -z --cached --others --exclude-standard`；非 git 目录用 `ignore` crate 遍历（遵守 `.gitignore`），最多 20 万个文件；
   - 列表按工作区缓存 5 秒，回合结束时失效；
   - 用 nucleo-matcher 做模糊匹配，返回相对路径、分数和匹配区间（UTF-16 偏移）；
   - `session_id` 和 `project_id` 两种入参都支持。
2. 提示内容映射（UNIT-prompt-mapping）：
   - `image {blob}` → ACP 图片块，受能力里的 `max_image_bytes` 限制；
   - `file_ref {path}` → `resource_link`，路径相对工作区，不能越出工作区；
   - `review_comment {…}` → 宿主排成给 agent 的文本（GT-2）；
   - `text_blob {blob}` → 取回全文后作为文本块。
   - agent 不支持的块（能力里的 `prompt.image` 等）按设计处理，在汇报里写清楚。
3. `workspace/open`：按设置 `apps.terminal`、`apps.editor` 和内置的应用清单打开工作区，`finder` 用 Finder；应用不存在时返回 `unavailable/app_not_found`。打开外部应用的子进程经 `cupper-sys::spawn`（例如 `open -a`），参数不经 shell。
4. 用例：IT-files-search、UNIT-prompt-mapping、IT-workspace-open，都已登记。IT-workspace-open 在测试里不能真的打开应用，用注入点或替身确认命令和参数。
5. 依赖新 crate（nucleo-matcher、ignore）时锁精确版本，过 license 检查。

**不做**：输入框里 `@` 弹出的界面（界面任务）；审查面板（T130、T131）；"打开"菜单的界面（T153）。

## 验收标准

- [ ] 三个用例通过。
- [ ] `schema-compat` 0 breaking（只能附加）。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：600；硬停止线：750

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.2.11（文件服务与设置）、5.2.10（快照档）、5.3.4（协议方法）、03 第 171 行（`PromptContent`）、5.6.3（能力）、5.8.2（scope）。
- 计划：T126 一行，1.6。
- 卷宗：T120、T222（快照档和 Git 服务的写法）、T143（输入框会发的内容块，retry 原样重发）、T046（cupper-cli）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 测试里不要真的打开任何应用，也不要遍历测试临时目录以外的地方。
- git 子进程和外部应用一律经 `cupper-sys::spawn`；时间经 `Clock`。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
