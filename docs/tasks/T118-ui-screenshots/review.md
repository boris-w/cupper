# T118 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：651 行（实现约 472、测试约 179；不含 notes、lock、字体、基线），在约 750 的停止线以内；开工前估 610。
- **CI**：执行者的摘要 `pr-477fa3e16e0d.json` 通过，`ui` 作业第一次有实现（7 项）。合并 main（含 T077）之后在本 worktree 重跑 `cargo xtask ci pr --record`，`pr-6af8374210f8.json` 通过（rust 1779、protocol 185、ts 379、license、testdata-scan、ui 7）。合并没有冲突。

## 逐项意见

1. **截图包放在 `tools/ui-screenshots`**：用例需要 Node 和 Playwright 的类型，截图页需要 DOM 库，放进 `ui-core` 或 `apps/desktop` 都会污染它们的类型环境。认可。
2. **CSP 只有一个来源**：`MAIN_DOCUMENT_CSP` 挪进 `@cupper/shell-api` 的 `csp.ts`，主进程和截图用例都引用它。Trusted Types 策略名本来就是外壳与界面之间的约定，放在 shell-api 合理。
3. **zod 的修复**：zod 在构建 schema 时（模块求值阶段）就探测 JIT，所以 `jitless` 必须在生成的 schema 求值之前设好。`@cupper/protocol` 的入口第一个导入 `jitless.ts`，渲染进程、截图页、主进程、Node 测试都经过这里。UI-csp-zero 修复前报违规、修复后为空，证据充分。
4. **基线与元数据**：
   - 只有 `pnpm ui:baseline` 能写基线，平时 `updateSnapshots: "none"`，缺基线就失败；
   - Chromium 严格比对；macOS 大版本不同失败，小版本只提示。
   - 符合计划 1.8。我看过 `shell-light.png`：字体是夹具里的 Inter 和 Noto Sans SC，mock 宿主下状态点是绿色，弱认证标记显示正常。
5. **字体取 GB2312 一级汉字的子集（约 1.1 MB）**：后续加中文文案基本不用重做子集。`fonts.test.ts` 会在文案出现子集外的字时报错。体积可以接受，取舍合理。字体是 OFL，许可证随附；来源、哈希和命令记在 notes 里，可以复现。
6. **`ci pr` 的路径过滤**：只看从 main 分叉以来改过的路径；在 main 上或没有 main 时照常运行（保守）。规则在 `ui.rs` 的 `UI_PATHS`，有单元测试。

## 记下的差异

- **截图用 Playwright 自带的 Chromium（156），App 里的 Electron 44 是 Chromium 152**：
  - 设计 5.9.1 定的就是"Playwright（Chromium）+ 浏览器版 ShellAPI"，所以元数据记 156 是对的；
  - 截图回归防的是界面自身的回退，不是引擎差异；
  - 复刻度审阅用的截图另在 e2e 构建（Electron）里截取（计划 1.11），两者不冲突。
  - 以后如果要求截图引擎与 App 一致，再改用 `_electron` 驱动（涉及 T090）。
- **路径过滤的端到端效果没有实际跑过**，只由单元测试覆盖。下一个只改文档的任务合并时，我会看一眼 `skipped steps` 里有没有 `ui / test-ui`。

## 交接

- **T081、T082、T083、T086 等界面任务**：照 `tools/ui-screenshots` 的 notes"后续界面任务怎样加用例"一节加截图用例。
  - spec 标题以编号开头；用 `open(page, fixture)` 打开；
  - 用 `pnpm ui:baseline` 生成基线，看图后连同 `meta.json` 一起提交；
  - 新文案出现子集外的字时 `fonts.test.ts` 会报错，按 notes 重做子集。
