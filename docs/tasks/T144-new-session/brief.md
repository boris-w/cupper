# T144 新建会话页：两种 agent、工作区与分支

- 派发日期：2026-11-16
- 执行者：子 agent（前端与 e2e；T103、T155、T262、T182 的执行者）
- 分支：`feat/T144-new-session`
- 依赖：T111、T124、T141、T152（都已合并）。T148、T170 依赖本任务

## 范围

**做**（计划 T144 一行；设计 5.7.4"新建会话页"、CP-5、AG-3、AG-4；Q44、Q45 的决定见 `docs/meetings/2026-11-16-q44-q45/`）：
1. 新建会话页的动作条：agent 选择（最左边的下拉，两种 agent 及其可用状态）、本地或新建 worktree、起始分支（从 `git/branches` 里选）；分支名显示在顶栏标题。
2. 第一条消息前显示缓存的能力：模型、推理强度、权限模式、斜杠命令（AG-4），接上 `composer/config.tsx` 的 `useOption`、`useSetOption`；当前档位为 `elevated` 时加提示。
3. **Q44**：设置页"通用"加"默认权限"一行，按 agent 各一个，选项来自 `agents/capabilities` 的缓存，存宿主设置 `sessions.default_permission.<agent>`（修改需要 `write`）；新建会话页按它预选，经 `session/create.config.permission_mode` 发出，宿主照档位规则处理（完全访问每次都要原生确认）。存下的模式已不在能力里时，按"沿用 CLI 配置"处理并在设置页提示。宿主加这个设置键。
4. **Q45**：App 启动后停在新建会话页，项目预选为上次所在会话的项目；上次的项目已不存在时不预选。
5. worktree 准备失败、LFS、子目录等的提示（见交接事项）。
6. 用例：E2E-new-session-agent-codex、E2E-new-session-before-first-message（已登记），默认权限和启动预选用单元测试或 e2e 覆盖。

**不做**：会话管理菜单和快捷键（T153）；复刻度的整体修正（T148）。

## 交接事项（逐条带入）

- 新建会话页（T085）在 `packages/ui-core/src/newsession/NewSessionPage.tsx`：agent 下拉现在只列 Claude，可用状态取自索引视图的 `AgentStatus`（不是 `agents/capabilities`，后者只有缓存的能力描述），原因码由导出的纯函数 `unavailable(status)` 推出（`cli_missing`、`not_logged_in`、`runtime_failed`，其余算 `unavailable`）；选中的 agent 和工作区经 `Composer` 的 `create` 属性传给 `useSend`（`composer/send.ts` 的 `NewSession`、`DEFAULT_NEW_SESSION`）。M1a 在 agent 不可用时仍允许发送、由宿主按语义码报错；按 AG-4"不支持的功能不出现"，是否直接禁止发送由本任务一并定（T085）
- `stream` 命名空间没有 `error_project_untrusted` 的文案，Codex 在未信任项目里的错误现在显示为"其他错误"；新建会话页和设置页会看到 Codex 的状态（没装 CLI 时为 `unavailable`）（T111（已写入 T151 的 brief））
- 新建会话页还没有权限按钮和模型选择器：两个扩展拿不到会话流时不显示；第一条消息前显示缓存的能力时，一并接上 `composer/config.tsx` 的 `useOption`、`useSetOption`（T231）
- 会话工作区是某个仓库的子目录时，`Layout::read` 找不到 `.git`，快照记为 `skipped/not_a_repo`，diff 不可用（与 5.2.11"git 仓库的子目录按非 git 目录处理"一致）；新建会话页或项目添加如果允许选仓库的子目录，要给出提示（T234）
- worktree 准备失败时会话状态是 `idle`，不进 `error`：界面凭工作区视图的 `Failed{code, reason}` 和以 `workspace_failed` 暂停的队列（第一条消息在里面）显示失败；`filter_unsupported` 的提示文案在 5.2.10；`base_ref` 不在创建时校验，从 `git/branches` 里选，避免 add 才失败；`branch_hint` 只能是单段（T124）
- `workspace.failed` 还有 `not_found`（worktree 目录被删，git 里的记录还在）：提示用户，并告诉他可以自己 `git worktree prune`（T236）
- `lfs_object_missing`：还原（审查面板、T132 的撤销）和新建 worktree 时宿主不下载本地没有的 LFS 对象，界面提示"在终端里运行 `git lfs fetch`"（T237）
- 新建会话页读 `agents/capabilities` 的查询在 `composer/extensions/slash.tsx` 的 `Cached` 组件，查询键 `["agents/capabilities", agent]`，`agents.updated` 让它失效；显示缓存的模型和权限时共用这个查询（T244）

## 验收标准

- [ ] 两个 E2E 用例在 runner 上通过（e2e-repeat 各 10 次）；默认权限、启动预选有测试。
- [ ] 设置键和协议若有改动走 `pnpm gen`，CI 的 protocol 检查通过；截图基线只由 ui-baseline 生成并逐张说明原因。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 停止线：600；硬停止线：720。开工前在 `notes.md` 里按模块估行数报给 `main`，等回复再写代码；**每完成一个模块就跑一次 `cargo xtask size`**，推算越线就停下汇报，按模块分提交。

## 参考资料

- 设计：5.7.4（新建会话页、设置页）、5.2.11（设置表）、5.8.2（档位规则）、5.6.5（配置项映射）、5.2.10（worktree）。
- 卷宗：T085、T111、T124、T141、T231、T234、T236、T237、T244。

## 注意

- e2e 在 runner 上跑（守则第 10 条）；本机只跑单元测试和单份短冒烟，PERF 不在本机跑。
- 不运行真实的 Claude Code、Codex CLI。
- 要推送、触发 e2e-repeat、ui-baseline 时告诉 `main`（e2e-repeat 写清 `spec` 和 `test`）。

## 工作方式

worktree 是 `.claude/worktrees/T037`，编排者已切到新分支 `feat/T144-new-session`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
