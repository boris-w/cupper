# T118 工作笔记

## 开工前的核对

- `docs/tasks/handoff-notes.md` 里第一列含 T118 的行：没有。T084 的 notes 和评审里与本任务有关的：主题经 `data-theme` 固定；zod 的 Trusted Types 违规在本任务修。
- `tests/ids.toml` 的 T118 段已有 UI-shell、UI-shell-narrow、UI-csp-zero、UNIT-ui-baseline-meta。
- xtask：`ui` 作业已经登记为持锁作业（`locked("ui", None)`），`pr` 组合里也有它；还没有按路径过滤的机制，也没有 Playwright 报告的解析。
- `MAIN_DOCUMENT_CSP` 在 `apps/desktop/src/main/security/app-protocol.ts`。截图设施若放在独立的包里，直接引用桌面端的源文件会让 TS 工程互相引用，所以把它挪到 `@cupper/shell-api`（两边都已依赖）。
- `@cupper/protocol` 的 `src/index.ts` 是手写的（生成器只写 `gen/`），可以在那里设置 zod。

## 规模估计（开工前）

按格式化后的写法估。字体、字符表、截图基线这类夹具不计。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| CSP 挪到 shell-api | `csp.ts`、导出、桌面端改为引用 | 25 | — |
| zod jitless | `@cupper/protocol` 入口设置，附理由 | 5 | — |
| 包骨架 | `tools/ui-screenshots` 的 `package.json`、`tsconfig`、Vitest、Playwright、截图页的 Vite 配置 | 85 | — |
| 截图页 | `index.html`、`main.ts`（browser-mock + mock 宿主 + 夹具）、字体的 `@font-face` | 70 | — |
| 运行设施 | 构建截图页、带 CSP 提供页面、违规计数、基线元数据的读取比对与写入 | 100 | — |
| 截图用例 | UI-shell（亮、暗）、UI-shell-narrow（浮层、自动收起与恢复）、UI-csp-zero | — | 90 |
| 单元测试 | UNIT-ui-baseline-meta、字体子集覆盖文案的检查 | — | 55 |
| xtask | `ui` 作业、Playwright JSON 报告解析、`ci pr` 的路径过滤 | 110 | 60 |
| 其他 | 根 `package.json` 的脚本等 | 10 | — |
| 合计 | | 405 | 205 |

合计约 **610 行**。实现常比估计多 15% 左右，推算约 700，离 750 不远：每个模块完成后统计，推算到 750 就停下汇报。

## 实现记录

### 放在哪里

- 截图设施是一个独立的工作区包 `tools/ui-screenshots`（`@cupper/ui-screenshots`），和 `tools/` 下其他测试工具并列。没放进 `ui-core`：截图用例要用 Node 和 Playwright 的类型，放进去会让 `ui-core` 的源码也能看到 Node 的全局对象。没放进 `apps/desktop`：截图页要 DOM 类型，桌面端工程没有 DOM 库。
- `ts` 作业照常对它跑 `tsc --build` 和 Vitest（UNIT-ui-baseline-meta、字体覆盖检查）；Playwright 用例是 `*.spec.ts`，只由 `ui` 作业跑。
- `MAIN_DOCUMENT_CSP` 挪到了 `@cupper/shell-api` 的 `csp.ts`，主进程和截图用例引用同一份。CSP 里的 Trusted Types 策略名 `cupper-worker` 由 `ui-core`（T080）创建，本来就是外壳和界面之间的约定，放在 shell-api 也说得通。

### zod 的修法和位置

- 现象：zod 4 在**构建**对象 schema 时（模块求值阶段，不是第一次解析时）就执行 `new Function("")` 探测，结果缓存在 schema 上。所以必须在生成的 schema 模块求值**之前**设置 `jitless`。
- 位置：`@cupper/protocol` 新增 `src/jitless.ts`（只调用 `config({ jitless: true })`），入口 `src/index.ts` 第一行 `import "./jitless"`，排在 `export * from "./gen/..."` 之前。ES 模块按导入顺序求值，所以每个 schema 构建时都已关掉探测。
- 理由：所有使用 schema 的地方（渲染进程经 host-client、截图页、主进程、Node 测试）都只能经这个入口拿到 schema（包的 `exports` 只有 `.`），放在这里一处就覆盖全部，不依赖调用方的初始化顺序。放在 `ui-core` 的 `mount` 里不行：`mount` 运行时 schema 早已构建完。主进程和测试里也关掉 JIT，代价是解析稍慢，消息规模下可以忽略。
- 验证：修之前 UI-csp-zero 报 `require-trusted-types-for trusted-types-sink ... Function|() {}`；修之后为空。

### 字体

| 文件 | 来源 | 处理 |
|---|---|---|
| `fonts/inter-latin-wght.woff2` | npm `@fontsource-variable/inter@5.3.0` 的 `files/inter-latin-wght-normal.woff2`（sha256 `3100e775…4c62`） | 原样使用（拉丁字符子集，可变字重） |
| `fonts/noto-sans-sc-subset.woff2` | google/fonts 仓库 `ofl/notosanssc/NotoSansSC[wght].ttf`（sha256 `a3041811…af0da`） | fonttools 4.60.1 的 `pyftsubset`：`--text-file=noto-sans-sc-chars.txt --flavor=woff2 --no-hinting --layout-features='kern,liga,calt,ccmp,locl,mark,mkmk'`，结果 sha256 `6e16fc28…5c2d`，约 1.1 MB |
| `fonts/noto-sans-sc-chars.txt` | GB2312 的符号区（1–9 区）和一级汉字（16–55 区），加可打印 ASCII 和 U+22EF，共 4,533 个字符 | 用 Python 的 `gb2312` 编解码逐区生成 |
| `fonts/*-OFL.txt` | 两个字体各自的 OFL 1.1 许可证 | 原样 |

- 选一级汉字而不是"界面现有文案用到的字"：后者只有几十 KB，但每加一条中文文案都要拿 17 MB 的原字体重新取子集。一级汉字覆盖常用字，后续任务加文案时基本不用动字体。
- `src/fonts.test.ts` 检查 `packages/i18n` 各语言文案里的每个非 ASCII 字符都在字符表里。失败时，要么换个常用字，要么按上表重新取子集（把缺的字加进字符表）。
- 截图页用 `fonts.css` 覆盖 `--font-sans`：Inter 在前，拉丁字符取它；其余字符回落到 Noto Sans SC 子集。真实 App 不受影响。

### 渲染因素

视口 1280×820，`deviceScaleFactor` 2，`zh-CN`，`Asia/Shanghai`，`page.clock.setFixedTime(2026-01-01 09:00 +08:00)`，`reducedMotion: reduce`，截图时 `animations: "disabled"`、`caret: "hide"`。主题经 browser-mock 的 `ShellInfo.theme`（`ThemeProvider` 写 `data-theme`）。Playwright 1.64.1 自带 Chromium（headless shell）156.0.8078.4；Electron 44 是 Chromium 152，两者不同，元数据记的是截图实际用的 Playwright Chromium。

### 截图页与夹具

- `src/page/main.ts`：`createBrowserShell({ info, host: createMockShellHost(new MockHost(methods)) })`，然后 `mount(shell)`。`window.cupperShell` 是 browser-mock 的 controls（菜单命令、系统主题等），`window.cupperHost` 是 mock 宿主。
- `src/serve.ts` 的 `open(page, fixture)`：用 `page.route` 从 `https://cupper.test/` 提供构建好的页面，每个响应都带 `MAIN_DOCUMENT_CSP`；页面加载前经 `addInitScript` 放入夹具，并开始记录 `securitypolicyviolation`（含来源文件、行号和样本，方便定位）。`fixture.info` 覆盖 `ShellInfo`（主题、语言、prefs），`fixture.methods` 是 mock 宿主的应答，默认 `protocol/fixtures/methods.json`。等到宿主握手完成、字体加载完再返回。
- 截图页由 Playwright 的 global setup 用 Vite 构建到 `dist/page/`（被 gitignore）。

### 基线

- 位置：`tools/ui-screenshots/baselines/<名字>.png`，元数据 `baselines/meta.json`（`chromium`、`macos`）。
- 重新生成：`pnpm ui:baseline`（即 `CUPPER_UI_BASELINE=1 playwright test --update-snapshots=changed`）。它只改写像素有差异的图和缺失的图，并把当前的 Chromium、macOS 版本写进元数据。生成后逐张看图再提交。
- 平时 `pnpm test:ui`：配置里 `updateSnapshots: "none"`，缺基线直接失败，不会顺手写一张。global setup 先比对元数据：Chromium 不同 → 失败；macOS 大版本不同 → 失败；只有小版本不同 → 打印提示，照常比对像素，像素有差异时再跑 `pnpm ui:baseline`。
- `xtask ci ui` 运行时去掉 `CUPPER_UI_BASELINE`，CI 永远不写基线。

### 后续界面任务怎样加用例

1. 在 `tools/ui-screenshots/src/` 加 `<名字>.spec.ts`（或在已有文件里加），测试标题以用例编号开头，例如 `test("UI-sidebar dark", ...)`。
2. 用 `open(page, { info: { theme, prefs }, methods })` 打开页面。需要宿主数据时，在 `methods` 里换掉对应方法的应答；需要菜单命令或系统事件时，`page.evaluate(() => window.cupperShell.command({ id: "..." }))`。
3. `await expect(page).toHaveScreenshot("<名字>.png")`，名字不要和已有基线重复。
4. 跑 `pnpm ui:baseline` 生成基线，看图，连同 `meta.json` 的变化一起提交。
5. 新增中文文案时，`fonts.test.ts` 会检查字体子集是否覆盖。

### `xtask ci ui` 与 `ci pr` 的路径过滤

- `ui` 作业：`pnpm install --frozen-lockfile`、`playwright install chromium`（已装好时不下载）、`playwright test`，用 JSON reporter 逐条记录用例。作业取机器级锁。
- 过滤规则（`crates/xtask/src/ci/jobs/ui.rs` 的 `UI_PATHS`）：只在 `ci pr` 里生效。比较的是从 `main` 的分叉点到 HEAD 的提交里改过的路径；命中下列任一项就运行，否则记为跳过的步骤（摘要里 `skipped: ["test-ui"]`）：
  - 目录：`packages/ui-core/`、`packages/shell-api/`、`packages/host-client/`、`packages/protocol/`、`packages/i18n/`、`protocol/fixtures/`、`tools/ui-screenshots/`；
  - 文件：根目录的 `package.json`、`pnpm-lock.yaml`、`tsconfig.base.json`。
- 无从比较时照常运行：没有 `main` 分支，或者 HEAD 没有领先 `main` 的提交（例如在 main 上跑合并后的 CI）。`ci ui` 和 `ci nightly` 总是运行。
- 验证：`touches_ui` 和 `changed_since_main` 各有单元测试；手动验证时，在只改了文档的分支上跑 `cargo xtask ci pr`，结果的"skipped steps"里会有 `ui / test-ui`。
