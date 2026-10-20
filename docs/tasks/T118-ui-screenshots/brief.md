# T118 ui-core II-b：截图回归设施

- 派发日期：2026-10-20
- 执行者：子 agent（前端工程师）。T084 的执行者接手
- 分支：`feat/T118-ui-screenshots`
- 依赖：T084（合并中）。T081、T082、T083、T086 依赖本任务。**本任务在剩余关键路径上（松弛为 0）**：T084 → T118 → T081 → T090 → T091 → T099

## 背景

T084 开工前拆成三份（计划 v1.3.22）。T084 建了 `packages/ui-core`：token、三栏外框、Providers 与状态路由、页面骨架与插槽，渲染进程已挂载。本任务建截图回归的整套设施，把 UI-shell、UI-shell-narrow、UI-csp-zero 跑起来。之后的界面任务都在这上面加自己的截图用例。基础组件和行类型注册表由 T128 并行做。

## 范围

**做**（计划 T118 一行；设计 5.9.1 的截图回归、5.1.6 的 `ui` 作业、5.7.8 的主文档 CSP；计划 1.8 的"截图基线"一行）：
1. **截图页**：Playwright（Chromium）加载一个专用页面，用浏览器版 ShellAPI（`packages/shell-api` 的 `browser-mock`）和 TS mock 宿主（`packages/host-client` 的 `mock`）挂载 `@cupper/ui-core`。夹具能指定主题、语言、窗口尺寸、mock 宿主里的数据。
2. **渲染因素固定**：
   - 随夹具附带开源字体，只带用到的字重；中文字体做子集，控制仓库体积；字体的许可证文本随字体放（例如 OFL）。
   - 固定视口、缩放、语言、时区、虚拟时钟；关闭动画；隐藏光标。
3. **基线与元数据**：
   - 基线只由专用脚本生成；
   - 元数据记下 macOS 和 Chromium 版本：Chromium 版本严格比对，对不上就失败；macOS 只比对大版本，小版本变化只给提示；只在出现像素差异时才重新生成；
   - 写进 `notes.md`：基线放在哪、怎样重新生成、后续界面任务怎样加用例。
4. **作业**：
   - `pnpm test:ui`；
   - `cargo xtask ci ui`，取机器级锁（`crates/xtask/src/ci/lock.rs`）；
   - 按改动路径接进 `xtask ci pr`：只在界面相关路径有改动时运行，规则写清楚。
5. **UI-csp-zero**：
   - 在发布 CSP 下渲染，`securitypolicyviolation` 计数为 0。
   - CSP 只有一个来源：主进程的 `MAIN_DOCUMENT_CSP`（`apps/desktop/src/main/security/app-protocol.ts`）。截图页要用同一份；需要挪到两边都能引用的位置时就挪，不要再抄一份字符串。
6. **zod 的 Trusted Types 违规**（T084 的发现）：
   - 现象：渲染进程经 `@cupper/host-client` 打包进了 zod 4。zod 第一次解析对象时用 `new Function("")` 探测能否 JIT，被 `require-trusted-types-for 'script'` 拦下。zod 自己吞掉了异常，但这算一次违规。
   - 修法：在第一次解析之前调用 `z.config({ jitless: true })`。放在渲染进程和截图页都会经过的位置（例如 `@cupper/protocol` 的手写入口，或 ui-core 的 `mount`），理由写进 `notes.md`。

**不做**：
- 基础组件、行类型注册表（T128）；
- 外框的视觉调整（发现与设计不符的，在汇报里提出）；
- 暗色取值的核对（T106、T152）。

## 验收标准

- [ ] UI-shell：外框的亮色和暗色截图（顶栏、侧边栏、主区、侧边面板、标签条）。
- [ ] UI-shell-narrow：窗口窄于 768 时侧边栏为浮层；主区放不下时侧边面板自动收起、变宽后恢复。
- [ ] UI-csp-zero：发布 CSP 下 `securitypolicyviolation` 为 0（修掉 zod 那一处之后）。
- [ ] UNIT-ui-baseline-meta：元数据比对的三种情况（Chromium 不同 → 失败；macOS 小版本不同 → 只提示；相同 → 通过）。
- [ ] `cargo xtask ci ui` 通过；`xtask ci pr` 的路径过滤有用例或在 notes 里写明怎样验证。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T118 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T118-ui-screenshots/ci/` 通过，摘要随任务提交。

## 交接事项

- T084 的 `notes.md`：路由状态的形状、布局状态与 prefs 的键、token 的写法（`light-dark()`、`data-theme`、Tailwind `@theme static`）。截图里固定主题时，经 `data-theme` 切换。
- `handoff-notes.md` 里第一列含 T118 的行（如有）。

## 注意

- 新依赖锁定精确版本（含 Playwright 和它下载的浏览器版本）；字体的许可证要允许开源分发。
- 不运行真实的 Claude Code、Codex CLI；截图只用 mock 宿主。
- F2：只复刻，不复制；不使用 Codex 的字体、图标、样式表。

## 规模

计划估为 M，T084 的执行者估约 570 行（字体、截图基线等夹具不计）。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 750 行（包括贴线）时，必须先停下来汇报**；硬停止线 850。

## 工作方式

T084 合并之后，在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b feat/T118-ui-screenshots main`。T084 的分支由编排者合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
