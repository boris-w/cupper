# T130 汇报

- 执行者：子 agent（前端工程师，ui-core 与桌面）
- 分支：`feat/T130-review-panel`
- 完成日期：2026-11-04
- PR：https://github.com/boris-w/cupper/pull/62（head a780500）
- CI：https://github.com/boris-w/cupper/actions/runs/41767473304 ，8 个检查全部通过（rust、ui、e2e、compat-framework，均为 macos-15；protocol、ts、license、testdata-scan，均为 ubuntu-24.04）
- ui-baseline：run 41765234394（在 e541007 上跑）、run 41766405259（在 29e1269 上跑）

## 做了什么

范围是拆分后的 T130。文件树、hunk 与文件的图标按钮、右键菜单、⌘⇧E 归 T232；"Git 操作"菜单与提交弹窗归 T233。

- **数据与状态**（`review/state.ts`）：
  - 每个会话一个 zustand store，持有范围（默认"未暂存"）、换行、diff 视图（auto/split/unified）、折叠的路径、滚动目标。
  - `git/diff` 按范围查询，`git/status` 取 `repo_config_changed`（直接用 T123 生成的 `GitStatusResult`）。
  - 查询键都放在 `["session", id, …]` 下，`turn.completed` 和每次操作之后都会让它们失效。
- **头部**：
  - "查阅来源"下拉，三个选项：最近一轮、未暂存、已暂存。
  - `+N −N` 统计；没有改动时不显示。
  - 选项菜单：换行；分栏、统一、自动；展开全部、收起全部；刷新。
- **提示与空状态**：
  - `repo_config_changed` 的醒目提示，列出路径。
  - `caveats` 的提示：`mixed_changes`、`late_tool_effects` 有专门的文案，其他码用通用文案并带上码。
  - 空状态：三种范围各一个；另有没打开会话、不是 Git 仓库、加载失败（可重试）。
- **操作执行与整组药丸**（`review/actions.ts`）：
  - 每个操作都带它所在 diff 的 `diff_token`；不论成败，操作后都重新加载。
  - 失败提示按语义码给出：`stale_diff`、`outcome_unknown`、`user_activation_required`，其他情况带码。
  - 药丸：未暂存和最近一轮是"还原全部""暂存全部"，已暂存只有"取消暂存全部"。
  - 在"最近一轮"里点"暂存全部"：先取未暂存的 diff，与本轮文件取交集，再按路径暂存。
- **还原确认**（`review/RevertConfirm.tsx`，RP-11）：
  - 500 ms 待命期；只认就绪之后按下的点击，以及键盘点击。
  - 由"还原全部"触发时焦点放在"确认"；由视图触发时不抢焦点。
  - 同一时间只显示一个，已有一个时新的请求被忽略；不提供"不再询问"。
  - 确认之后才发 `git/revert`。主进程按真实输入附上 InputEvidence。
- **评论草稿与附件条**：视图交上来的草稿进入输入框的附件，合成"{n} 条评论"，展开后每条列出 `{path}:{line}` 和正文，可以逐条删除。
- **diff 视图接口**：在 `review/view.ts` 定死，见下一节。T131 之前先用占位的 `FileList`（文件名加增删数）。
- **接线**：
  - 侧边面板的"变更"tabpanel 挂上审查面板。
  - 命令 `open_review_tab` 打开侧边面板。
  - 文案在 `@cupper/i18n` 新增的 `review` 命名空间里，约 45 个键。
- **菜单**：`MenuItem` 加了 `checked`、`radio`，并与 T231 统一（见后面一节）。

## 给 T131 的 diff 视图接口

代码在 `packages/ui-core/src/review/view.ts`，全文也写进了 `docs/tasks/T130-review-panel/notes.md`。

```ts
export type HunkAction = "stage" | "unstage" | "revert";
export type ViewTarget = { hunk_ids: HunkId[] } | { paths: WirePath[] };
export const ACTIONS: Readonly<Record<ReviewScope, readonly HunkAction[]>>;
// last_turn: revert；unstaged: stage、revert；staged: unstage

export interface ReviewViewProps {
  sessionId: string;
  scope: ReviewScope;                 // "last_turn" | "unstaged" | "staged"
  diff: GitDiffResult;                // 文件列表与 diff_token；hunk 由视图按需取
  wrap: boolean;
  view: DiffView;                     // "auto" | "split" | "unified"
  collapsed: ReadonlySet<string>;
  focus?: { path: string; line?: number; seq: number }; // seq 变了就再跳一次
  drafts: readonly ReviewComment[];
  setCollapsed(path: string, collapsed: boolean): void;
  act(action: HunkAction, target: ViewTarget, label?: string): void;
  comment(draft: ReviewComment): void;
}
export type ReviewViewComponent = ComponentType<ReviewViewProps>;
```

约定：

- **接入**：T131 把 `ReviewPanel.tsx` 里的占位 `FileList` 换成自己的组件，并删掉占位。`Slots.reviewView` 留给测试和截图换替身。
- **主文档一侧**：这个组件是 5.7.5 里 iframe 的主文档一侧：建 iframe 和 MessageChannel，把 props 译成 `render`、`scroll_to`，再把 `hunk_action`、`draft_comment` 译回 `act`、`comment`。
- **面板负责的状态**：加载中、错误、不是仓库、空状态都由面板处理，视图拿到的 `diff.files` 一定不为空。
- **取 hunk**：视图在 `need_file` 时自己调 `git/file_diff`，查询键用 `["session", id, "git/file_diff", diff_token, path]`，这样失效会一并覆盖到。
- **校验**：
  - 视图先按自己取到的 `file_diff` 校验 iframe 发来的 `hunk_ids` 属于当前 `diff_token`。
  - 面板再按 `ACTIONS[scope]` 丢掉这个范围不提供的操作。
  - 宿主对不匹配的令牌回 `stale_diff`，面板提示后重新加载。
- **还原**：`act("revert", …)` 一律经确认控件，不抢焦点，一次只一个。`label` 写路径，用作标题"是否还原 {path}？"；hunk 目标的标题是"还原这个 hunk？"（5.7.5 的原文）。
- **暂存与取消暂存**：直接执行，结果提示由面板负责。
- **评论**：`comment` 把草稿追加到附件，视图从 `drafts` 读回来，画在对应的行上。
- **`focus`**：由 T232 和消息流的跳转设置，写法是 `store.set({ focus: { path, line, seq: (focus?.seq ?? 0) + 1 } })`。T130 里还没有设置它的地方。
- **`auto` 视图**：按 ui-spec 4.6，同时有增删时用分栏，面板窄或只有增或删时用统一视图；"窄"的阈值由 T131 定。

## Menu.tsx 统一时改了 T231 的哪些地方

T231（PR #60）合并后，我 merge 了 main（ecb6313），再在 04f0f3b 统一：

- `CheckIcon`、`MenuLabel` 各只留一个。`icons.tsx` 与 main 完全相同；`MenuLabel` 用 main 的写法。
- `MenuItem` 的 class 与 main 相同，没有加 gap。`checked` 时给 role `menuitemradio`（加 `radio`）或 `menuitemcheckbox`，并带 `aria-checked`。勾放在条目末尾自己的槽里：`ms-auto w-8 justify-end`，不勾时留空，宽度不跳。
- **T231 的 `composer/config.tsx`**：`Choices` 去掉手工的勾和 `CheckIcon` 的 import，改成 `<MenuItem radio checked={…}>`。
- **T231 的 `composer/composer.test.tsx`**（UNIT-composer-config）：`pick` 改按 `menuitemradio` 查找，并加了一条断言：打开权限菜单时，勾选项是当前模式 Default。
- **T231 的 `tools/e2e/src/config.spec.ts`**：有一处按 `menuitem` 找"Bypass permissions"，改成 `menuitemradio`。
- **勾放哪边**：ui-spec 没有记录勾的位置。ChatGPT 系的菜单（Codex 的权限、模型下拉）勾在右侧，T231 也是这样，所以全应用统一放在右侧。
- **验证**：本机跑 UI-new-session 的 8 张基线都通过，两次 ui-baseline 也没有改动它们。

## 关键取舍

- **在"最近一轮"里暂存全部**：宿主只接受未暂存的令牌，所以取"未暂存"的 diff 与本轮文件的交集，按路径暂存；交集为空时提示"没有可以暂存的更改"。同一文件里用户自己的改动会一起暂存，头部已经有 `mixed_changes` 提示。
- **失效**：每次操作后，不论成败，都让 `["session", id]` 下的查询失效（GT-1，以仓库为准）。`stale_diff`、`outcome_unknown` 因此不用单独刷新。
- **还原确认里的点击**：按下发生在就绪之前、松开在就绪之后，这一下不算；键盘点击（`detail === 0`）在就绪后才算数。
- **范围的操作限制**：`ACTIONS` 由面板来执行。审查文档的 iframe 不可信，所以主文档要再检查一遍。
- **store 的 `reveal` 方法**：起初加过，但 T130 里没人用，已删掉。`focus` 作为接口留着，设置方式写在接口约定里。

## 验证方式

- **UNIT-review-actions**（`packages/ui-core/src/review/review.test.tsx`，5 个用例，全部通过）：
  - 药丸随范围变化，每个操作用对应范围的令牌。
  - "最近一轮"的"暂存全部"用未暂存的令牌、按路径暂存。
  - 范围不提供的操作被丢掉。
  - "还原全部"：焦点在"确认"，500 ms 待命，就绪前按下不算，取消后不发请求，确认后发 `git/revert {all}`。
  - 视图触发的还原：不抢焦点，一次只显示一个。
  - hunk 暂存直接发出；评论进入附件。
  - `stale_diff` 时提示并重新加载；其他拒绝提示带码。
  - `repo_config_changed` 提示、空状态、不是仓库。
- **UI-review-panel**（`tools/ui-screenshots/src/review.spec.ts`，8 个用例）：未暂存（亮、暗）、还原确认（亮、暗）、最近一轮（提示与 caveats）、已暂存、范围菜单、空状态，并检查没有 CSP 违规。CI 的 ui 检查通过。按你的说明，这两个用例不用连跑。
- **本机**：ui-core vitest 108 个、i18n vitest 10 个全过；tsc、eslint、prettier 都通过；整套截图在本机也通过。
- **安全约束**：
  - 没用 `innerHTML` 一类写法，ui-core 不 import `electron`、`node:*`。
  - 文案全部走 i18n，宿主只给语义码。
  - 确认控件要求就绪后的真实输入。
  - agent 给的字符串不进入样式。
- **复刻度审阅截图**：系统字体、1280×820，6 张，在 `docs/tasks/T130-review-panel/screenshots/`。

### 基线清单（10 张）

| 基线 | 来源 |
|---|---|
| review-unstaged-light、review-unstaged-dark、review-revert-light、review-revert-dark、review-last-turn、review-staged、review-scope-menu、review-empty（新增 8 张） | run 41765234394（e541007），提交 14d0052 |
| shell-light、shell-dark（变更 2 张） | run 41766405259（29e1269），提交 21fc99a |

- **shell 两张为什么取自第二次**：第一次产物里，没打开会话时的空状态贴在面板顶上（tabpanel 不是 flex 容器）。a1f996d 修好后重新生成。
- **第二次产物核对**：8 张 review 与 14d0052 逐字节相同，其余基线和 meta.json 也没有变化。

### 规模

`cargo xtask size`：+1,140 −35（净 +1,105），在停止线 1,250 以内。

| 部分 | 行数 |
|---|---|
| ui-core | +969（实现约 720，单测约 250） |
| i18n | +54 |
| 截图用例 | +114 |
| e2e | +1 |

开工前估计约 1,080，实际多 2%。

## 提交列表

```
a780500 docs(task): T130 notes: staging a turn's paths takes the user's edits in them too
be5f842 refactor(ui-core): read repo_config_changed from the generated GitStatusResult
bfd2304 Merge branch 'main' into feat/T130-review-panel
21fc99a test(ui-screenshots): UI-shell baselines with the review panel's no-session state
29e1269 docs(task): T130 fidelity screenshots with system fonts
14d0052 test(ui-screenshots): UI-review-panel baselines from the ui-baseline workflow
a1f996d fix(ui-core): center the review panel's no-session state
e541007 docs(task): T130 notes: the menus made one with T231's, and the size after the merge
04f0f3b refactor(ui-core): one way to tick a menu choice, at the item's end
ecb6313 Merge branch 'main' into feat/T130-review-panel
e859830 docs(task): T130 notes: the diff view interface for T131, choices and size
a2a1a11 test(ui-screenshots): UI-review-panel in each scope, the confirmation and the empty state
8f2aa83 feat(ui-core): group the review comments in the attachment bar
11ed006 feat(ui-core): the review panel's header, notices, bulk actions and revert confirmation
5d6567c feat(ui-core): menu items that show a check, and a menu label
006fbec Merge branch 'main' into feat/T130-review-panel
37f3792 docs(task): estimate T130 by module and propose a split before starting
```

## 遗留问题 / 范围外发现

- **菜单关闭后焦点回到触发按钮**：`ToolbarButton` 的 tooltip 随之打开、焦点环出现，用鼠标选完也是这样。所有用 `ToolbarButton` 作触发器的菜单都有这个问题，属于共用组件，不在 T130 范围内，没有改。截图用例在拍之前点一下面板空白处来绕开。是否改成鼠标选择后不显示 tooltip，请你决定。
- **T131 接手时**：把占位 `FileList` 删掉，换成审查文档组件。
- **T232 接手时**：用 `store.set({ focus })` 实现点击文件跳转；文件行和 hunk 的操作按 `ACTIONS[scope]` 显示。
