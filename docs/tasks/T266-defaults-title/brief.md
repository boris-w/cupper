# T266 默认权限、启动时预选项目与顶栏标题

- 派发日期：2026-11-16
- 执行者：子 agent（前端与 e2e；T144 的执行者）
- 分支：`feat/T266-defaults-title`
- 依赖：T144（已合并）。由 T144 拆出（开工前估算见 `docs/tasks/T144-new-session/notes.md`）。T148 依赖本任务

## 范围

**做**（计划 T266 一行；Q44、Q45 的决定见 `docs/meetings/2026-11-16-q44-q45/`；设计 5.7.4、5.2.11）：
1. **Q44 默认权限**：
   - 宿主设置键 `sessions.default_permission.<agent>`（`Option<String>`，修改需要 `write`），跑 `pnpm gen`；
   - 设置页"通用"按 agent 各一行"默认权限"，选项来自 `agents/capabilities` 的缓存，含"沿用 CLI 配置"；存下的模式已不在能力里时按沿用处理并提示；
   - 新建会话页按它预选：接在 `useNewOptions(agent)` 的 `permission_mode` 上（取值顺序"用户的选择 → 默认权限 → agent 自己的值"，见交接事项），完全访问照样走 `create_session` 原生确认。
2. **Q45 启动时预选项目**：进入会话时记下它的项目（UI 偏好）；App 启动后停在新建会话页，预选上次所在会话的项目；那个项目已不存在时退回列表里的第一个。
3. **顶栏**：会话页显示会话标题、agent 标识，worktree 会话加分支名（5.7.4）。
4. 单元测试覆盖默认权限的解析与失效提示、启动预选（含项目已删除）、顶栏三项；需要时补 e2e。

**不做**：能力缓存的事件（T268）；会话管理菜单（T265、T267）。

## 交接事项（逐条带入）

- 默认权限接在 `useNewOptions(agent)` 的 `permission_mode` 解析上，取值顺序"用户的选择 → 默认权限（在缓存的能力里时）→ agent 自己的值"，同一个值进 `chosen`，完全访问照样走 `create_session` 原生确认（T144）。

## 验收标准

- [ ] 单元测试覆盖上面第 4 条；协议改动经 `pnpm gen`，CI 的 protocol 检查通过。
- [ ] 截图基线只由 ui-baseline 生成并逐张说明原因（阈值内的改动用 `all=true`）。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 停止线：350；硬停止线：430（拆分时估约 290）。开工前在 `notes.md` 里按模块估行数报给 `main`，每完成一个模块跑一次 `cargo xtask size`，按模块分提交。

## 注意

- e2e 在 runner 上跑（守则第 10 条）；本机只跑单元测试和单份短冒烟。
- 要推送、触发 e2e-repeat、ui-baseline 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T037`，编排者已切到新分支 `feat/T266-defaults-title`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
