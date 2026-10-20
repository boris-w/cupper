# T128 ui-core II-c：基础组件与行类型注册表

- 派发日期：2026-10-20
- 执行者：子 agent（前端工程师）。T084、T118 的执行者接手
- 分支：`feat/T128-ui-components`
- 依赖：T084（已合并）。T081、T082、T083、T086 依赖本任务；与 T118（截图设施）并行

## 背景

T084 开工前拆成三份（计划 v1.3.22）：
- T084 建了 `packages/ui-core`：token、三栏外框、Providers 与状态路由、页面骨架与插槽。顶栏上的按钮是只引用 token 的朴素占位元素。
- 本任务写基础组件和消息流的行类型注册表，并替换这些占位。
- 截图设施由 T118 并行做。

## 范围

**做**（计划 T128 一行；设计 5.7.5、5.7.10；`docs/tasks/R06-codex-desktop-ui-firsthand/ui-spec.md` 第 4 节）：
1. **基础组件**：工具栏按钮、菜单、tooltip、开关、警告框。
   - 用 Radix（设计 5.1.3 给 ui-core 列的依赖），样式按 ui-spec 第 4 节，只引用 token。
   - 键盘可操作；焦点样式按 token。
   - 文案走 `@cupper/i18n`。
2. **替换 T084 的占位**：顶栏的侧边栏开关、弱认证标记、"打开"、侧边面板开关、"⋯"改用工具栏按钮。"打开"和"⋯"的菜单内容属于后续任务，本任务只让按钮能挂一个菜单（可以先空着）。
3. **行类型注册表**（5.7.5 列出的消息流行类型）：
   - 类型 → 组件的注册方式；
   - 未知类型显示占位（显示"此版本不支持"之类的文案，不显示原始数据）；
   - 具体的行组件由 T082、T083 等填。

**不做**：
- 截图用例（T118 的设施合并后，后续任务各自加）；
- 侧边栏内容、横幅、toast（T081）；行组件（T082、T083）；
- 侧边面板开合的 spring 动画（留给视觉任务）。

## 验收标准

- [ ] UNIT-ui-components：每个组件的键盘操作、禁用状态、ARIA 属性；菜单的打开、关闭、选择。
- [ ] UNIT-ui-row-registry：注册与查找；未知类型的占位；重复注册的处理。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T128 段里。
- [ ] 汇报附复刻度审阅截图：固定窗口尺寸，在使用系统字体的构建里截取（计划 1.11）；写明按 F2 查看过本机 Codex App 包的哪些资源（如果看过）。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T128-ui-components/ci/` 通过，摘要随任务提交。

## 交接事项

- T084 的 `notes.md`：
  - token 的写法：`light-dark()`、`data-theme`、Tailwind `@theme static`；只有 `tokens.css` 里定义过的 token 能生成工具类；
  - 布局状态；
  - 宽度只经 `style={{width}}` 写入。

## 注意

- 渲染进程不用 `innerHTML`、`dangerouslySetInnerHTML`、`insertAdjacentHTML`、`eval`、`new Function`；不创建 Trusted Types 策略。
- agent 给出的字符串不能进入 `<style>` 文本、`insertRule` 或 CSS 自定义属性。
- Radix 会经 react-style-singleton 动态插入 `<style>`，主文档的 CSP 已为此放行 `style-src 'unsafe-inline'`（设计 5.7.8）。
- F2：只复刻，不复制。图标用开源图标库或自绘；查看本机 Codex App 包只读、不启动、解包只放 `$TMPDIR` 用完删除。
- 新依赖锁定精确版本。

## 规模

计划估为 M，T084 的执行者估约 400 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 500 行（包括贴线）时，必须先停下来汇报**；硬停止线 580。

## 工作方式

T084、T118 已合并。在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b feat/T128-ui-components main`。T118 的分支已合并，不要再往它上面提交。截图用例的加法见 `tools/ui-screenshots` 的 notes；本任务的验收不要求截图用例，审阅截图照常附上。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
