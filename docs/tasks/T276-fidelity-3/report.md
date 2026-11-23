# T276 汇报：复刻度修正 III（菜单、弹窗、全局）

T148 清单里 I 组的 I1、I3、I4、I5、I8 和 J 组的 J1、J2 都做完了，PR #137 在 e6edb43e 上 8 个检查全部通过：https://github.com/boris-w/cupper/pull/137 ，运行记录 https://github.com/boris-w/cupper/actions/runs/45167823717 。

## 做了什么

- **I1、I3 圆角**：菜单改为 20（Codex 的 `--menu-radius`），菜单条目仍是 15；弹窗改为 15。设计 5.7.10 的圆角表同步更正。
- **I4 提交弹窗**：`review/GitActions` 改用共用的 `components/Dialog`。Dialog 新增两个可选项：标题下的一行（description）和按钮前的说明（note）。提交弹窗原来自己写的 dialog、showModal 和焦点处理都删掉了，布局不变。
- **I5 焦点归还**：
  - Dialog 在渲染时记下打开前的焦点，赶在弹窗里 autoFocus 的输入框抢走焦点之前。
  - 关闭时，如果焦点落在 body 上或还留在弹窗里，就交还给原来的元素。
  - 原来的元素已经不在页面上（打开它的是菜单项，菜单已收起）：按键关闭的，交给菜单的触发按钮；鼠标关闭的不还，和 Menu 现有的规矩一致。
  - 没有增加 Tab 停点。
- **I8**：顶栏的分支名经过 escapeInvisible。
- **J1 细滚动条**：所有滚动区域统一用细滚动条，轨道透明，滑块用 `--color-scrollbar-thumb`，悬停时换成 border-strong。替换了原来只给消息流的那条规则；审查文档加载的是同一份样式，也一起生效。
- **J2**：根元素设 `accent-color: var(--color-accent)`，单选框、复选框跟着变色。

## 关键取舍

- **焦点来处的读取时机**：原打算在 layout effect 里读，但 autoFocus 的输入框会先拿到焦点，读到的已经是弹窗里的元素，所以改在渲染时读。
- **鼠标关闭不还焦点**：为了避免点击后触发按钮弹出 tooltip、显示焦点环，你已同意。
- **不加 fragment**：换成共用 Dialog 后整块少了一层缩进，我没有为了让 diff 好看去包一层无用的 fragment；你同意了，硬线提到 170。

## 验证方式

- **单元测试**：
  - I5：从"⋯"打开的弹窗按键关闭后焦点回到"⋯"，鼠标关闭后不回；
  - 原有用例"提交弹窗按 Esc 把焦点还给 Git 操作"照旧通过；
  - I8：分支名里的零宽字符显示为转义形式。
- **本机**：ui-core 202 项全过（编排者合并 main 后是 204 项）；全部 TS 项目 tsc 通过；eslint、prettier 通过。
- **基线**：用 57a0d2c8（已含 T148、T272）上 ui-baseline 45166131648（all=true）的产物，84 张里 9 张有变化，都已提交，其余 75 张逐字节相同。
  - new-session 的 agent、项目选择器（亮暗各 2 张）、review-scope-menu、review-tree-menu：只在菜单的圆角处变化（20）。
  - review-commit-light、review-commit-dark、review-commit-failed：弹窗圆角变为 15，单选框和复选框变为强调色。
  - 这些截图里都没有出现滚动条，所以没有基线因滚动条而变。
- **规模**：+158 / −145（净 +13），硬线 170。去掉空白（`diff -w`）是 +102 / −89；GitActions 里约 53 行只是缩进变化。

## 提交列表

- 5a562f6a feat(ui-core): menus 20 and dialogs 15; the commit dialog on Dialog
- 8b9e658a fix(ui-core): the title bar's branch name goes through escapeInvisible
- 153bfc26 feat(ui-core): thin scrollbars everywhere; radios and checkboxes in the accent
- 57a0d2c8 Merge branch 'main'（编排者合并 T148、T272，没有冲突）
- e6edb43e test(ui): baselines with menus at 20, dialogs at 15, radios in the accent

## 遗留问题 / 范围外发现

- 共用 Dialog 的"取消"文案在 `session` 命名空间里，现在提交弹窗也用它，适合挪到更通用的命名空间，可以在 T154 做英文时一并处理。
- `review/RevertConfirm` 是嵌在页面里的确认卡片，不是模态弹窗，圆角仍是 20，这次没有改。
- 右键菜单的触发点是会话行本身，不是按钮，所以从右键菜单打开的弹窗按键关闭后没有触发按钮可以交还焦点，焦点会落回 body。要改的话可以交还给会话行的按钮，另立任务。
