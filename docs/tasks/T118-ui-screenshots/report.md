# T118 汇报（ui-core II-b：截图回归设施）

- 执行者：子 agent（前端工程师）
- 分支：`feat/T118-ui-screenshots`
- 完成日期：2026-10-20

截图回归设施已经能跑：UI-shell、UI-shell-narrow、UI-csp-zero、UNIT-ui-baseline-meta 全部通过，`cargo xtask ci pr --record` 在干净的 worktree 里通过，摘要已提交。zod 那处违规修好了，UI-csp-zero 的计数为 0。规模 651 行，在约 750 行的停止线以内。

## 做了什么

- **截图包**：新建工作区包 `tools/ui-screenshots`（`@cupper/ui-screenshots`）。截图页用 browser-mock 的 ShellAPI，配上 `createMockShellHost(new MockHost(methods))`，挂载 `@cupper/ui-core`。
- **加载方式**：Playwright 1.64.1 用自带的 Chromium（headless shell 156.0.8078.4）。截图页由 global setup 用 Vite 构建，再用 `page.route` 从 `https://cupper.test/` 提供，每个响应都带 `MAIN_DOCUMENT_CSP`。
- **夹具**：可以设 `info`（主题、语言、prefs）和 `methods`（mock 宿主的应答，默认 `protocol/fixtures/methods.json`）。页面上暴露两个对象供用例驱动：`window.cupperShell`（菜单命令、系统主题等）和 `window.cupperHost`（mock 宿主）。
- **固定的渲染因素**：
  - 字体：Inter 拉丁子集（可变字重），加 Noto Sans SC 子集（GB2312 符号区和一级汉字，约 1.1 MB），两者都是 OFL，许可证随附；
  - 视口 1280×820，缩放 2，`zh-CN`，`Asia/Shanghai`，`clock.setFixedTime`；
  - reduced motion，截图时关闭动画、隐藏光标。
- **基线与元数据**：
  - 基线在 `tools/ui-screenshots/baselines/`，`meta.json` 记录 Chromium 和 macOS 版本；
  - 只能由 `pnpm ui:baseline` 生成：它只改写有差异或缺失的图，并写入当前版本；
  - 平时运行设了 `updateSnapshots: "none"`，缺基线直接失败；
  - global setup 先比对元数据：Chromium 不同就失败；macOS 大版本不同就失败，只有小版本不同时打印提示。
- **用例**（4 个编号）：

  | 编号 | 内容 |
  |---|---|
  | UI-shell | 亮色、暗色，侧边面板打开 |
  | UI-shell-narrow | 700 宽时侧边栏变浮层，Esc 关闭；959 宽时侧边面板自动收起，960 宽时恢复 |
  | UI-csp-zero | 加载后操作开关和主题，违规列表为空 |
  | UNIT-ui-baseline-meta | 元数据比对：相同通过；Chromium 不同失败；macOS 小版本只提示；macOS 大版本失败 |

  另有一个不带编号的检查：字体子集是否覆盖全部文案。
- **作业**：
  - `pnpm test:ui` 和 `pnpm ui:baseline` 两个根脚本；
  - `cargo xtask ci ui` 现在有实现，持有机器级锁，依次运行 install、`playwright install chromium`、`playwright test`，并逐条解析 Playwright 的 JSON 报告，摘要里能看到 4 个编号；
  - 在 `ci pr` 里按路径过滤：只看从 main 分叉以来改过的路径。没有 main 分支、或 HEAD 没有领先 main 时照常运行。规则写在 `ui.rs` 的 `UI_PATHS` 和 notes 里。
- **CSP 只有一个来源**：`MAIN_DOCUMENT_CSP` 挪到了 `@cupper/shell-api` 的 `csp.ts`，主进程和截图用例都引用它。
- **zod 修复**：`@cupper/protocol` 新增 `src/jitless.ts`，入口文件在导入生成的 schema 之前先导入它。

## 关键取舍

- **包放在 `tools/` 下，不放进 `ui-core` 或 `apps/desktop`**：用例需要 Node 和 Playwright 的类型，放进 `ui-core` 会让它的源码看到 Node 全局对象；截图页需要 DOM 库，桌面端工程没有。
- **CSP 挪进 shell-api**：从测试包直接引用桌面端源文件，会让两个 TS 工程互相引用。CSP 里的 Trusted Types 策略名本来就是外壳和界面之间的约定，放在 shell-api 也合理。
- **zod 的修复位置**：zod 是在**构建** schema 时（模块求值阶段）就做探测，不是在第一次解析时。所以 `jitless` 必须在生成的 schema 求值之前设好，放在 ui-core 的 `mount` 里已经太晚。protocol 包只导出入口 `.`，所以渲染进程、截图页、主进程、Node 测试都必须经过这里。我最初把设置写在入口文件末尾，UI-csp-zero 仍然报违规，于是改成第一个导入，在提交前修正了那次提交。
- **字体子集取一级汉字，而不是只取文案里用到的字**：后者只有几十 KB，但每加一条中文文案都要拿 17 MB 的原字体重新取子集；前者约 1.1 MB，后续任务基本不用再动字体。`fonts.test.ts` 会在文案出现子集外的字时报错。
- **用 `page.route` 加载截图页，不起静态服务器**：不占端口，也不用额外的进程。
- **元数据记录 Playwright 的 Chromium（156）**：截图实际是用它渲染的；Electron 44 是 Chromium 152，两者不同。

## 验证方式

- `pnpm test:ui` 连跑多次，5 条全部通过。另外手动验证了两种失败情况：删掉一张基线后运行会失败；把 `meta.json` 里的 Chromium 改掉后，global setup 会报错退出。
- UI-csp-zero：修复之前报 `require-trusted-types-for trusted-types-sink … Function|() {}`，修复之后为空。
- 基线由 `pnpm ui:baseline` 生成，我逐张看过：亮色、暗色、窄窗口浮层、侧边面板收起。字体是夹具里的 Inter 和 Noto Sans SC，状态点为绿色（mock 宿主），弱认证标记也显示出来了（来自 `methods.json` 的应答）。
- xtask 新增单元测试：Playwright 报告解析、`touches_ui` 的路径规则、`changed_since_main`（在 main 上、在任务分支上、没有 main 三种情况）。`cargo xtask ci ui` 通过（7 项）。
- 在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T118-ui-screenshots/ci/`，全部通过，摘要 `pr-477fa3e16e0d.json` 已提交：

  | 作业 | 结果 |
  |---|---|
  | rust | 1,779 通过 |
  | protocol | 185 通过 |
  | ts | 366 通过 |
  | license | 通过 |
  | testdata-scan | 通过 |
  | ui | 7 通过 |

  e2e 作业还没实现。license 这次一遍就过了。
- 路径过滤的手动验证方法写在 notes 里：在只改了文档的分支上跑 `ci pr`，结果的 skipped steps 里会出现 `ui / test-ui`。我没有实际做这一步，过滤逻辑只由上面的单元测试覆盖。
- 用 `git merge-tree` 试合并最新的 main（已含 T077），没有冲突。
- **规模**：651 行（实现约 472，测试约 179；不含 notes、lock、字体、基线）。开工前估计 610，在约 750 的停止线以内。

## 提交列表

```
4afcea8 docs(T118): record the ci pr summary
477fa3e docs(T118): record where the baselines live and how to add a case
61e1c62 feat(xtask): run the screenshot regression as the ui job
8541fce test(ui): add the screenshot regression suite
2deba51 fix(protocol): build the zod schemas without the eval probe
2947591 refactor(shell-api): keep the main document's CSP in shell-api
dfd1890 docs(T118): record the checks and the size estimate
```

## 遗留问题 / 范围外发现

- **下载**：两次联网下载，都是本任务必需的公开物料，请求里没有个人信息：
  - Playwright 的浏览器下载到用户缓存目录，不进仓库；
  - Noto Sans SC 原字体从 google/fonts 下载，子集工具 fonttools 装在临时 venv 里，用完不留。来源、哈希和命令都记在 notes 里，可以复现。
- **截图与真实 App 的 Chromium 版本不同**（156 对 152）：如果以后要求"截图引擎与 App 完全一致"，需要换成 Playwright `_electron` 驱动，这会涉及 T090 的 e2e 设施。
- **后续界面任务怎样加用例**：notes 的"后续界面任务怎样加用例"一节写了步骤：spec 标题以编号开头、用 `open(page, fixture)` 打开页面、用 `pnpm ui:baseline` 生成基线、看图后连同 `meta.json` 一起提交。可以带进 T081、T082、T083、T086 的任务书。
- **和 T128 的冲突**：本任务没有改 `packages/ui-core/package.json`。两边都会改的是根目录 `pnpm-lock.yaml`。