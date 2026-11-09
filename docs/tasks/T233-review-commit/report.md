# T233 汇报

- 执行者：子 agent（前端工程师，ui-core 与桌面）
- 分支：`feat/T233-review-commit`（已合并，main 3f63361）
- 完成日期：2026-11-04
- PR：https://github.com/boris-w/cupper/pull/72（head b93d2b5）
- CI：https://github.com/boris-w/cupper/actions/runs/41787172749 ，8 个检查全部通过（rust、ui、e2e、compat-framework，均为 macos-15；protocol、ts、license、testdata-scan，均为 ubuntu-24.04）
- ui-baseline：https://github.com/boris-w/cupper/actions/runs/41786433567（在 2035276 上跑）

## 做了什么

1. **"Git 操作"菜单**（新文件 `review/GitActions.tsx`）：放在审查面板头部"显示文件"的左边，`ReviewPanel.tsx` 只在 `Header` 里加了这一处，没有碰 T131 改的视图那一块。
   - 菜单有"提交…"和"推送"两项，打开的是同一个弹窗，只是默认选中的下一步不同。
   - detached HEAD 时，"推送"不可用。
2. **提交弹窗**：用原生 `<dialog>` 的 `showModal()`，没有新增依赖。
   - 宽 420，标题"提交或推送"，写明"提交到 {分支}"。detached HEAD 时写"当前是分离的 HEAD，只能提交，不能推送"。
   - 提交信息框高 80；下一步三选一：提交、提交并推送、推送。只推送时，信息框和"包含未暂存的更改"都不可用。
   - "包含未暂存的更改"：先现取一次"未暂存"的 diff 拿到新令牌，`git/stage {all}`，再 `git/commit`。暂存成功而提交失败时，提示末尾加"改动已在已暂存里。"。
   - 按钮旁注明"将运行仓库的 hooks"。推送也会运行 pre-push，所以三种下一步都显示这句。
   - `git/status` 有 `repo_config_changed` 时，弹窗里有指纹提示，列出路径。
   - 关闭时的焦点：键盘关闭（Esc，或用键盘提交成功）把焦点还给"Git 操作"按钮，指针关闭则不还。这与 T232 修的菜单规则一致。
   - 弹窗里没有 Radix 的下拉，用的是原生的单选框和复选框，所以不存在 portal 被遮罩盖住的问题。
3. **推送**：`git/status` 没有 `upstream` 时默认带 `set_upstream`，弹窗里会说明"这个分支还没有上游，推送时会设置上游"。
4. **失败的指引**：按 reason 给文案，都走 i18n。
   - `signing_failed`：改用 pinentry-mac。
   - `auth_failed`：检查 ssh agent 或凭据助手，并说明只在 shell 配置里 export 的 `SSH_AUTH_SOCK`，从图形界面启动时看不到。
   - `hook_failed`、`nothing_to_commit`、`rejected`（先拉取再推送）、`lock_held`、`outcome_unknown`（刷新后由用户决定）各有对应文案。
   - 其他情况显示"操作没有完成（{code}）"。
   - `data.stderr` 放在折叠的"git 的输出"里，用 `<pre>` 以文本节点渲染。
   - 任何结果之后，都让 `["session", id]` 下的查询失效，重新加载。
5. **用例**：
   - UNIT-review-commit，`review/commit.test.tsx`，5 个用例：三种下一步各发出哪些命令；两步的暂存与提交（含暂存成功、提交失败）；有无上游；detached HEAD；提交和推送的各个失败码；stderr 作为文本渲染；Esc 关闭后焦点回到按钮。jsdom 没有模态对话框，测试里补了 `showModal`、`close`。我也试过去掉"还焦点"那一行，对应的断言会失败。
   - 共用的测试辅助：按你的要求，两份单测相同的 provider 样板抽成了 `review/test-harness.tsx`，`review.test.tsx` 也改用它，用例一条没砍。
   - UI-review-commit：亮暗两种主题的弹窗，以及一张带指纹提示和失败指引的。
6. **复刻度审阅截图**：系统字体，3 张，在 `docs/tasks/T233-review-commit/screenshots/`。

## 关键取舍

- **原生 `<dialog>`**：自带顶层显示、遮罩、Esc 关闭，焦点也不会离开对话框，不需要再加 Radix Dialog。
- **暂存的令牌**：不读缓存，现取一次。宿主只接受当前未暂存 diff 的令牌，缓存的令牌可能已经过期。
- **"将运行仓库的 hooks"在推送时也显示**：`git push` 会运行 pre-push。

## 验证方式

- **本机**：ui-core vitest 113 个、i18n vitest 10 个全过；tsc、eslint、prettier 都通过；整套截图用新基线跑，57 个全过。
- **基线清单**（run 41786433567，产物共 56 个文件，逐个与分支上的做字节比较）：

  | 基线 | 变化 | 说明 |
  |---|---|---|
  | review-commit-light、review-commit-dark、review-commit-failed | 新增 3 张 | 逐张看过 |
  | review-empty、review-last-turn、review-revert-light/dark、review-scope-menu、review-staged、review-unstaged-light/dark（T130 的 8 张） | 变化 | 像素比较：只在 x 956–1078、y 120–144（设备像素）这一框里，即新加的"Git 操作"按钮 |
  | review-tree-light/dark、review-tree-filter、review-tree-menu（T232 的 4 张） | 变化 | 同上，同一框 |
  | 其余 41 个（含 meta.json、shell、UI-new-session） | 逐字节相同 | — |

  "显示文件"（从 x 1138 起）和"⋯"都没有移动。基线提交为 db93bdea。
- **安全约束**：没用 `innerHTML` 一类写法；文案都走 i18n；stderr 只作为文本渲染；ui-core 不 import `electron`、`node:*`。
- **规模**：`cargo xtask size` 是 +587 −60（净 +527）。超过停止线 500，经你同意，在硬停止线 600 以内。

  | 部分 | 行数 |
  |---|---|
  | GitActions.tsx | 263（估 200） |
  | UNIT-review-commit | 175 |
  | 共用辅助 test-harness.tsx | +52 |
  | review.test.tsx 改用共用辅助 | +19 −54 |
  | UI-review-commit | +46 −5 |
  | 文案 | +29 |
  | ReviewPanel | +3 |

  明细在 notes 里。

## 提交列表

```
b93d2b5 docs(task): T233 fidelity screenshots with system fonts
db93bde test(ui-screenshots): UI-review-commit baselines, and the review ones with Git 操作
2035276 docs(task): T233 notes: the size after the shared harness
692bfd9 test(ui-core): one harness for the review panel's tests
0e6823b docs(task): T233 notes: estimate and size
f337edc test(ui-screenshots): UI-review-commit
75863ab test(ui-core): UNIT-review-commit
f1ff5a4 feat(ui-core): the review panel's Git actions and commit dialog
```

## 遗留问题 / 范围外发现

- **T131 合并后要重新生成基线**：它替换占位视图之后，这次的 8 张 review、4 张 review-tree 和 3 张 review-commit 都可能再变。按你的安排，后合并的一方在 merge main 之后重新生成一次。
- **单选框和复选框的颜色**：用的是浏览器默认的强调色（系统蓝），没有接设计 token。如果要统一，可以加 `accent-color`，这不在本任务范围内，没有改。
