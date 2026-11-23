# T267 会话悬停卡片、"打开"的替身与会话管理的端到端用例

- 派发日期：2026-11-23
- 执行者：子 agent（前端与 e2e；T265 的执行者）
- 分支：`feat/T267-hover-card`
- 依赖：T265（已合并）。由 T265 拆出（拆分时的估算见 `docs/tasks/T265-session-manage/notes.md` 的 M4–M6）。T148 依赖本任务

## 范围

**做**（计划 T267 一行；设计 5.7.4 的侧边栏、5.9.9）：
1. **会话行的悬停卡片**（照 Codex，T106 的核对与截图）：
   - 替换会话行上写 agent 名的 Tooltip（`sidebar/parts.tsx` 的 `SessionRow`）；
   - Codex 的卡片宽约 20rem：第一行标题与相对时间（右侧可带未读点），下面是项目行（图标加名字），状态行（如系统错误）在下；agent 名并入卡片（AG-3）；
   - 相对时间用 `SessionSummary.last_activity_at`，i18n 加 `Intl.RelativeTimeFormat` 的格式化；
   - 计划行里写的"行尾固定和归档按钮"，先对照 T106 的材料和 T265 现在的会话行，看还缺什么，在估算里说明。
2. **会话标题经 `escapeInvisible`**：顶栏（`shell/SessionTitle.tsx`）、侧边栏会话行、悬停卡片三处一起改，防标题冒充（T266 的交接）。
3. **"打开"的替身**：e2e 构建（`test-harness`）里，`workspace/open` 一律执行 `<数据目录>/opener`，发布构建和自用构建仍是 `/usr/bin/open`；形态补进设计 5.9.9（可以直接改在分支上）。
4. **E2E-archive-cleanup**（已登记）：
   - 把 `notification.spec.ts` 里的 `asHarness` 挪进 helpers 共用；
   - 建两个 worktree 会话（经 harness 调 `session/create`，或者用 T144 的新建会话页）；
   - 干净的从行菜单归档，并删掉 worktree；有改动的列出原因后保留归档。
5. **E2E-workspace-open**（已登记）：夹具写替身脚本，把参数追加到日志；三个目标各点一次，核对替身收到的参数。
6. E2E 里顺带覆盖 ⌘⇧A（归档）。

**不做**：顶栏"打开"的应用选择设置项；项目行"在 Finder 中显示"（没有宿主方法）。

## 交接事项（逐条带入）

- 悬停卡片要替换会话行上写 agent 名的 Tooltip；卡片尺寸与内容如上；E2E 里覆盖 ⌘⇧A；`opener` 替身的形态补进 5.9.9（T265）。
- 会话标题在顶栏和侧边栏都原样显示，没有像项目名那样经 `escapeInvisible`；悬停卡片也显示标题，三处一起改（T266）。

## 验收标准

- [ ] 单元测试覆盖悬停卡片（标题、相对时间、项目、状态行）和标题转义；E2E-archive-cleanup、E2E-workspace-open 通过，并在 runner 上各连跑 10 次（由 `main` 触发 e2e-repeat，同时给 `spec` 和 `test`）。
- [ ] 截图基线只由 ui-baseline 生成并逐张说明原因。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 拆分时估 245（悬停卡片 90、替身 25、E2E 130），加上标题转义和 ⌘⇧A。停止线：330；硬停止线：400。
- 开工前按模块估一遍报给 `main`，每完成一个模块跑一次 `cargo xtask size`。

## 注意

- e2e 在 runner 上跑（守则第 10 条）；本机只跑单元测试和单份短冒烟。
- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T150`，编排者已切到新分支 `feat/T267-hover-card`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
