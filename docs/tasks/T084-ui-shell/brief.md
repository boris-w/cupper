# T084 ui-core II：token、三栏外框、页面骨架与截图测试

- 派发日期：2026-10-20
- 执行者：子 agent（前端工程师）。T052、T053、T095、T076、T009 的执行者接手
- 分支：`feat/T084-ui-shell`
- 依赖：T071（已合并）。T081、T082、T083、T085、T086 等全部界面任务依赖本任务。**本任务在剩余关键路径上（松弛为 0）**：T084 → T081 → T090 → T091 → T099

## 背景

`packages/ui-core` 还不存在；渲染进程（`apps/desktop/src/renderer/index.html`）只有一个空的 `<body>`。本任务建立 `@cupper/ui-core` 包和界面的地基：token、三栏外框、Providers 与路由、两个页面的骨架与插槽、基础组件，以及截图回归的整套设施。之后的界面任务都在这上面填内容，所以组件从第一天起只引用 token（计划 1.11）。

ui-core I（T080：状态层、`security/policies.ts`、ESLint 安全与文案规则）会和本任务并行或稍后开工，也往 `packages/ui-core` 里加文件。包的骨架由本任务建立，T080 合并时在上面追加。

## 范围

**做**（计划 T084 一行；设计 5.7.4、5.7.5、5.7.7 的主题、5.7.10、5.9.1 的截图回归）：
1. **包骨架**：`packages/ui-core`（`package.json`、`tsconfig`、Vitest、Tailwind v4），依赖按设计 5.1.3 的表（本任务只加用得到的：React 19、Tailwind v4、Radix 等）。`ui-core` 不 import `electron`、`node:*`。
2. **token**：按设计 5.7.10 直接实现布局常量、颜色（亮色与〔推算〕暗色）、字体与字号、间距、圆角（含 `corner-shape: superellipse(1.5)` 的渐进增强）、阴影与边框、动效；打成一份 CSS（5.7.7：主文档和审查文档共用）。主题跟随系统，可以固定成亮色或暗色（经 `data-theme` 一类的属性切换；设置页是 T036/后续任务）。
3. **三栏外框**（5.7.4）：顶栏 44（`hiddenInset`，交通灯留位）；侧边栏（宽度拖动 240–520、默认 288，主区至少 240；窗口窄于 768 时变成浮层，点外部或 Esc 收起）；主区；右侧侧边面板与 46 高的标签条（宽度拖动与键盘调整；主区放不下"会话区 352 + 侧边面板 320"时自动收起，窗口变宽后恢复）。宽度等存 prefs 的状态：本任务用一个小的布局 store 并经 ShellAPI 读写 prefs（没有对应接口时在汇报里提出，先存内存），T080 的状态层之后接管。
4. **Providers 与路由**：`ShellProvider`、`HostProvider`、`I18nProvider`、`ThemeProvider`（5.7.5 组件树的根）；会话页和新建会话页的路由。
5. **页面骨架与插槽**：`<NewSessionPage>`、`<SessionPage>` 的骨架，输入框位置留插槽给 T086、T083；侧边栏的头部、项目列表、底部留插槽给 T081。
6. **行类型注册表**：5.7.5 列出的消息流行类型的注册方式（类型 → 组件），未知类型显示占位；具体的行组件由后续任务填。
7. **基础组件**：工具栏按钮、菜单、tooltip、开关、警告框；样式按 ui-spec 第 4 节，只引用 token。
8. **截图回归**（计划 1.8 的"截图基线"一行，测试 #13）：
   - Playwright（Chromium）+ 浏览器版 ShellAPI（`packages/shell-api` 的 `browser-mock`）+ TS mock 宿主（`packages/host-client` 的 `mock`）；
   - 渲染因素固定：随夹具附带开源字体（只带用到的字重；中文字体请做子集，控制仓库体积；字体的许可证文本随字体一起放），固定视口、缩放、语言、时区、虚拟时钟，关闭动画，隐藏光标；
   - 基线只由专用脚本生成；元数据记下 macOS 和 Chromium（Electron）版本：Chromium 严格比对，对不上就失败；macOS 只比对大版本，小版本变化只给提示；
   - `pnpm test:ui` 与 `cargo xtask ci ui`（作业取机器级锁，`crates/xtask/src/ci/lock.rs`），并接进 `xtask ci pr`（按改动路径过滤：只在界面相关路径有改动时运行，规则写清楚）；
   - **UI-csp-zero**：在发布 CSP 下渲染，`securitypolicyviolation` 计数为 0。CSP 只有一个来源：主进程的 `MAIN_DOCUMENT_CSP`（`apps/desktop/src/main/security/app-protocol.ts`）；测试要用同一份，不要再抄一份字符串。
9. **渲染进程挂载**：`apps/desktop/src/renderer` 挂载 `@cupper/ui-core`，注入 Electron 版 ShellAPI（5.1.2 的目录说明），桌面 App 启动后能看到外框。

**不做**：
- 状态层、订阅策略、`security/policies.ts`、ESLint 安全与文案规则（T080）；
- 侧边栏的内容、横幅、toast（T081）；输入框（T086）；请求面板（T083）；新建会话页的内容（T085）；各类行组件；
- 暗色取值的核对（T106，之后由 T152 修正）；半透明侧边栏（T152）。

## 验收标准

- [ ] UI-shell：外框的亮色和暗色截图（顶栏、侧边栏、主区、侧边面板、标签条）。
- [ ] UI-shell-narrow：窗口窄于 768 时侧边栏为浮层；主区放不下时侧边面板自动收起、变宽后恢复。
- [ ] UI-csp-zero：发布 CSP 下 `securitypolicyviolation` 为 0。
- [ ] 基础组件和拖动、键盘调整宽度的 Vitest 用例；行类型注册表的未知类型占位有用例。
- [ ] `cargo xtask ci ui` 通过；元数据比对的三种情况（Chromium 不同 → 失败；macOS 小版本不同 → 只提示；相同 → 通过）有用例。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T084 段里（缺的由你补进 T084 段）。
- [ ] 汇报附复刻度审阅用的截图：固定窗口尺寸、固定夹具，在使用系统字体（SF Pro、苹方）的构建里截取，不用开源字体的回归基线（计划 1.11）；写明按 F2 查看过本机 Codex App 包的哪些资源（如果看过）。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T084-ui-shell/ci/` 通过，摘要随任务提交。

## 交接事项

`handoff-notes.md` 里第一列含 T084 的行（开工前搜一遍）。另外：
- 设计 5.7.4 的"复刻的边界"：只复刻，不复制（F2）。不使用 Codex 的代码、样式表、图标、图片、字体；图标用开源图标库或自绘。核对细节时可以只读查看本机 App 包：不启动 App，不读用户数据目录，解出的文件只放 `$TMPDIR`，用完删除。
- 文案一律走 `@cupper/i18n` 的键（M1a 只做一种语言）。
- T080 的交接里有 `versions.subscribe`、`client.counters` 等，属于 T080，本任务不用接。

## 注意

- 渲染进程不用 `innerHTML`、`dangerouslySetInnerHTML`、`insertAdjacentHTML`、`eval`、`new Function`；不创建 Trusted Types 策略（只有 T080 的 `policies.ts` 可以创建）。发布 CSP 带 `require-trusted-types-for 'script'`，任何依赖库若在渲染时写 HTML 字符串，UI-csp-zero 会暴露出来，换库或在汇报里提出。
- agent 给出的字符串不能进入 `<style>` 文本、`insertRule` 或 CSS 自定义属性；本任务只有 token，没有 agent 内容，但宽度这类用户状态写进样式时用 style 属性的具体属性，不写 CSS 自定义属性也无妨，按你的实现在 notes 里说明。
- 新依赖锁定精确版本；`pnpm i` 之后 `cargo xtask ci license` 要能过（字体也是依赖，许可证要可以开源分发，例如 OFL）。

## 规模

计划估为 L，上限约 1,500 行（字体、截图基线等夹具不计）。**本任务范围很宽，按今天各任务的经验很可能超线。开工前先在 `notes.md` 里按模块写行数估计（测试单独列），推算超线就开工前停下汇报，并给出拆分建议。** 可以考虑的拆法：
- 第一部分（本任务，留在关键路径上）：包骨架、token、外框、Providers 与路由、页面骨架与插槽、截图设施与 `xtask ci ui`、UI-shell、UI-shell-narrow、UI-csp-zero、渲染进程挂载；
- 第二部分（新编号）：行类型注册表、基础组件。

**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条），推算贴线也要停下。

## 参考资料

- 设计：`07-desktop.md` 5.7.4、5.7.5、5.7.7（主题）、5.7.8（CSP）、5.7.10；`09-testing.md` 5.9.1、5.9.10；`01-repo-build-ci.md` 5.1.2、5.1.3、5.1.6（`ui` 作业）。
- 计划：1.8（截图基线一行、机器级锁）、1.11（界面复刻度）。
- `docs/tasks/R06-codex-desktop-ui-firsthand/ui-spec.md` 第 2–4 节。
- `docs/tasks/` 下 T071、T072、T074、T076 的 `notes.md` 和 `review.md`。

## 工作方式

在原来的 T052 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T084-ui-shell main`。T009 的分支已经合并，不要再往它上面提交。该 worktree 的 `target/` 刚清理过，第一次 Rust 构建会比较慢。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 2,070 行（实现 1,600、测试 470），在写代码之前停下汇报，建议拆成三份。

**裁定：采用三分法（计划 v1.3.22）。**
- **T084 收窄为**：包骨架、token、三栏外框（含窄窗口浮层、侧边面板自动收起与恢复）、拖动与键盘调整、Providers 与路由、页面骨架与插槽、渲染进程挂载与文案。验收：新增 UNIT-ui-shell-resize；桌面 App 启动后显示外框。上限约 1,200 行，硬停止线 1,300；推算贴线也先停下。
- **新增 T118（截图回归设施）**：Playwright 截图页、固定渲染因素与字体、基线脚本与元数据比对、`xtask ci ui` 与 `ci pr` 的路径过滤、UI-shell、UI-shell-narrow、UI-csp-zero（由 T084 移来），新增 UNIT-ui-baseline-meta。
- **新增 T128（基础组件与行类型注册表）**：工具栏按钮、菜单、tooltip、开关、警告框，行类型注册表；新增 UNIT-ui-components、UNIT-ui-row-registry。（T122 是作废的编号，不复用。）
- T118、T128 都只依赖 T084，可以并行；T081、T082、T083、T086 改为也依赖两者。`tests/ids.toml` 已在 main 上改好。

**三条取舍都认可**：
1. 会话页和新建会话页用一个小的状态路由，不引入路由库。路由状态的形状写进 `notes.md`，T080 的状态层之后接管时照用。
2. 布局宽度等经 ShellAPI 的 `prefs.set`、`onChange` 和 `init` 的快照读写。
3. 编号按上面的归属登记，本任务只有 UNIT-ui-shell-resize。

**本任务的边界**：
- 外框里需要的按钮、菜单先用最朴素的元素占位，只引用 token；正式的基础组件由 T128 写，届时替换。
- 不写截图用例，也不加 Playwright 依赖。窄窗口浮层、侧边面板自动收起与恢复，用 Vitest 在布局逻辑上覆盖（例如把"窗口宽度 → 布局状态"写成纯函数）。
