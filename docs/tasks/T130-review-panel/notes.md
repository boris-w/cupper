# T130 工作笔记

- 执行者：子 agent（前端工程师，ui-core 与桌面；T160、T225、T173 的执行者接手）

## 开工前核对到的现状

- 侧边面板（T084）已有标签条和空的 `tabpanel`；主进程菜单已注册 `open_review_tab`（⌃⇧G）、`toggle_file_tree`（⌘⇧E），ShellAPI 的命令联合里也有这两个（M1a 冻结）。
- 输入框已有附件条，`review_comment` 每条一个标签；设计要的是合成"{n} 条评论"、展开列出文件、行号、正文，逐条删除，要改。
- `Menu`、`ToolbarButton`、`Tooltip`、`Switch`、toaster 已有；TanStack Query 已接好，`turn.completed` 让会话的查询失效。
- 协议（冻结）：`git/diff {session_id, scope}` → `{caveats, diff_token, files, snapshot?, stat}`；`git/stage`、`git/unstage`、`git/revert` 共用 `GitApplyParams {command_id, diff_token, session_id, target, input?}`；`git/commit {message}`、`git/push {set_upstream?}`；`git/status` 给分支、ahead/behind。
- T123 会给 `git/status` 加 `repo_config_changed?: [paths]`、给 `ErrorReason` 加 `signing_failed`；合并前在 mock 里按这个形状做。
- mock 宿主（`@cupper/host-client/mock`）目前没有 git 方法，截图回归和单测要补。

## 规模估计（停止线 1,100，硬停止线 1,300）

行数指 Prettier 展开后的行，含注释和空行。文案（`packages/i18n` 的 JSON）计入。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| 数据与状态 | 审查面板的界面状态（范围、文件树开合与筛选、换行、diff 视图、展开集合、选中文件）、每个会话的评论草稿；`git/diff` 按范围查询并持有 `diff_token`，`git/status` 取分支和 `repo_config_changed` | 110 |
| 头部、提示与空状态 | "查阅来源"下拉、增删统计、文件树开关、选项菜单（换行、diff 视图、展开收起全部、刷新）；`caveats`、`repo_config_changed` 的醒目提示、快照排除的说明；空状态 | 150 |
| 文件树 | 由路径建树、目录吸顶、中间截断、Git 状态图标、"筛选文件…"、⌘⇧E、右键菜单（复制路径、在外部应用中打开）；文件行上的操作按钮 | 200 |
| 操作执行与整组药丸 | 暂存、取消暂存、还原的执行（`command_id`、`diff_token`、target；`stale_diff` 时刷新并提示；结果提示）；底部药丸按范围变化 | 150 |
| 还原确认（RP-11） | 500 ms 待命、按触发来源决定焦点、同一时间只显示一个且忽略新的、只认就绪后的按键和点击；确认后才发 `git/revert` | 110 |
| Git 操作菜单与提交弹窗 | 菜单、弹窗（分支、提交信息、"包含未暂存的更改"即先 `git/stage` 再 `git/commit`、三选一、hooks 与 `repo_config_changed` 提示）；错误：`hook_failed`、`nothing_to_commit`、`signing_failed`（pinentry 指引）、`auth_failed`、`rejected`；无上游时推送带 `set_upstream` | 230 |
| 评论草稿与附件条 | 草稿进附件条，合成"{n} 条评论"并可展开、逐条删除；给 T131 的添加入口 | 70 |
| diff 视图接口 | 给 T131 的组件接口（下发 `diff_token`、文件、草稿、展开状态；收 hunk 操作、评论草稿、`need_file`），T131 之前用占位 | 50 |
| 接线、图标、文案 | 侧边面板里挂上审查面板、两个菜单命令；Git 状态与操作图标；中英文案约 70 个键 | 200 |
| **实现小计** | | **1,270** |

**测试**

| 项 | 内容 | 草估 |
|---|---|---|
| UNIT-review-actions | 三种范围下的整组操作、`stale_diff`、还原确认的焦点与待命、一次只一个、"包含未暂存"先暂存后提交、`signing_failed` 指引、草稿进附件条 | 220 |
| UI-review | 文件树开合、整组操作在三种范围下、空状态、提交弹窗 | 110 |
| mock 宿主的 git 方法与夹具 | `git/diff`、`file_diff`、`status`、`stage`、`unstage`、`revert`、`commit`、`push` | 70 |
| 复刻度审阅截图 | e2e 构建、系统字体，不进基线 | 40 |
| **测试小计** | | **440** |

**合计约 1,710**，按前几个 L 任务的偏差（多 10–30%）可能到 1,900–2,200。远超 1,100，需要拆分。

## 拆分方案（报编排者）

| 任务 | 内容 | 估计 |
|---|---|---|
| T130（关键链，T131、T132 等它） | 数据与状态、头部与提示与空状态、操作执行与整组药丸、还原确认（RP-11）、评论草稿与附件条、diff 视图接口、接线与相应文案；UNIT-review-actions 的这部分、UI-review 的"整组操作三种范围、空状态" | 约 1,080（可能到 1,250） |
| T130b 文件树 | 文件树全部、文件行上的操作按钮；UI-review 的"文件树开合" | 约 350 |
| T130c Git 操作与提交弹窗 | 菜单与弹窗、`repo_config_changed` 的确认文案、`signing_failed` 指引；依赖 T123（正好等它合并） | 约 450 |

编号建议：UI-review 拆成子编号（`-panel`、`-tree`、`-commit`）；UNIT-review-actions 留给 T130，T130c 另起 UNIT-review-commit。

## 拆分后的范围（2026-11-04 编排者同意）

T130 做数据与状态、头部、提示与空状态、操作执行与整组药丸、还原确认、评论草稿进附件条、diff 视图接口；文件树（含 hunk 和文件的图标按钮、文件行操作、右键菜单、⌘⇧E）归 T232，"Git 操作"菜单与提交弹窗（含 `signing_failed` 指引、提交确认里的 `repo_config_changed`）归 T233。

## 给 T131 的 diff 视图接口（定死）

代码在 `packages/ui-core/src/review/view.ts`。审查面板持有 diff 和它的 `diff_token`、评论草稿、折叠的文件、滚动目标，往下传；视图是无状态的，往上只交 hunk 操作和评论草稿，由面板检查后执行。这与 5.7.5"审查文档"一节的 iframe 消息协议对得上：T131 实现的是主文档一侧的组件，它建 iframe 和 `MessageChannel`，把这些 props 译成 `render`、`scroll_to`，把 `hunk_action`、`draft_comment` 译回 `act`、`comment`。

```ts
export type HunkAction = "stage" | "unstage" | "revert";
/** What a hunk action names: hunks of one file, or whole files, of `diff`. */
export type ViewTarget = { hunk_ids: HunkId[] } | { paths: WirePath[] };
export const ACTIONS: Readonly<Record<ReviewScope, readonly HunkAction[]>>;
// last_turn: revert；unstaged: stage、revert；staged: unstage

export interface ReviewViewProps {
  sessionId: string;
  scope: ReviewScope;                 // "last_turn" | "unstaged" | "staged"
  diff: GitDiffResult;                // 文件列表与 diff_token；hunk 由视图按需取
  wrap: boolean;
  view: DiffView;                     // "auto" | "split" | "unified"
  collapsed: ReadonlySet<string>;     // 折叠的路径；"收起全部"放进全部路径
  focus?: { path: string; line?: number; seq: number }; // 新的 seq 表示再跳一次
  drafts: readonly ReviewComment[];   // 输入框里待发送的本会话评论
  setCollapsed(path: string, collapsed: boolean): void;
  act(action: HunkAction, target: ViewTarget, label?: string): void;
  comment(draft: ReviewComment): void;
}
export type ReviewViewComponent = ComponentType<ReviewViewProps>;
```

约定：

- **接入**：T131 把 `ReviewPanel.tsx` 里的占位 `FileList` 换成自己的组件（删掉占位）。`Slots.reviewView` 留着，供测试和截图换上替身，与其他槽位相同。
- **只在有改动时渲染**：加载中、出错、不是仓库、空状态都由面板处理；视图拿到的 `diff.files` 不为空。范围切换或宿主返回新的 `diff_token` 时，`diff` 换成新对象。
- **取 hunk**：视图自己用 `git/file_diff {session_id, diff_token, path}` 取（`need_file` 时），查询键放在 `["session", sessionId, "git/file_diff", diff_token, path]` 下，这样 `turn.completed` 和每次操作后的失效会一并覆盖。
- **校验**：`act` 之前，视图（主文档一侧）按它取到的 `git/file_diff` 校验 iframe 发来的 `hunk_ids` 属于当前 `diff_token`；面板再按 `ACTIONS[scope]` 丢掉这个范围不提供的操作。宿主仍会对不匹配的令牌回 `conflict/stale_diff`，面板提示"变更已经更新，请再操作一次"并重新加载。
- **还原**：`act("revert", …)` 不会直接执行，而是在主文档弹出确认控件（RP-11）：不抢焦点、同一时间一个、已有一个时新的被忽略、500 ms 待命、只认就绪后的真实点击或按键。`label` 写文件路径，用作确认标题"是否还原 {path}？"；`hunk_ids` 目标的标题按 5.7.5 是"还原这个 hunk？"。
- **暂存与取消暂存**：直接执行，结果提示与失败提示由面板负责。
- **评论**：`comment` 把草稿追加到本会话输入框的附件（`{type: "review_comment", ...draft}`），附件条合成"{n} 条评论"，展开后逐条列出 `{path}:{line}`、正文，可以逐条删除；视图从 `drafts` 读回，画在对应的行上。发给 agent 只经输入框的发送。
- **滚动**：`focus` 由文件树（T232）和消息流的跳转设置，T130 里还没有设置它的地方；视图在 `seq` 变化时滚到 `path`（和 `line`）。设置方式是 `store.set({ focus: { path, line, seq: (focus?.seq ?? 0) + 1 } })`。
- **`wrap`、`view`**：来自头部选项菜单，视图照着渲染；`auto` 按面板宽度在分栏与统一之间选，阈值由 T131 定。

## 实现中的取舍

- 整组"暂存全部"在"最近一轮"里：宿主只接受未暂存的令牌，所以先取"未暂存"的 diff，取它与本轮文件的交集，按路径暂存；交集为空时提示"没有可以暂存的更改"。
- 每次操作之后，不论成败，都让 `["session", id]` 下的查询失效（GT-1：以仓库为准），`stale_diff`、`outcome_unknown` 因此不需要单独刷新。
- 菜单的勾选项：`MenuItem` 加了 `checked`，范围与视图用 `menuitemradio`，换行用 `menuitemcheckbox`，不勾选的项留出勾的位置对齐。
- 还原确认里，点击只在按下发生在就绪之后才算数（就绪前按下、就绪后松开不算）；键盘的点击（`detail === 0`）在就绪后算数。

## 发现的范围外问题

- 菜单关闭后焦点回到触发按钮，`ToolbarButton` 的 tooltip 随之打开、焦点环出现，用鼠标选完也是这样（所有用 `ToolbarButton` 作触发器的菜单都一样）。截图用例在拍之前点一下面板空白处。是否改成鼠标选择后不显示 tooltip，留给编排者决定。

## 实际规模

`cargo xtask size`：+1,145 −29（净 +1,116），停止线 1,250 以内。分布：ui-core +977（实现约 730，单测约 250），i18n +54，截图用例 +114。比开工前的估计（约 1,080）多 6%。
