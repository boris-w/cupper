# T070 汇报

- 执行者：子 agent（TypeScript 工程师，桌面端）
- 分支：`feat/T070-shell-api-i18n`
- 完成日期：2026-10-19

## 做了什么

**结果：** 验收标准逐条满足。在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T070-shell-api-i18n/ci/` 通过：
- rust 16 项、ts 26 项、license 1 项；
- 摘要的 `results` 里有 `UNIT-shell-mock` 和 `UNIT-i18n-escape`，结果都是 pass；
- 摘要文件是 `docs/tasks/T070-shell-api-i18n/ci/pr-6d620b452c1a.json`。

**规模：** 不含 lock 文件约 1,120 行。其中：
- 产品代码约 420 行；
- 测试约 340 行（TS 约 305 行，xtask 约 35 行）；
- xtask 的产品代码约 140 行；
- 配置约 80 行；
- `notes.md` 约 60 行。

lock 文件新增 613 行。

**交付内容：**

1. **`@cupper/shell-api`**（`packages/shell-api`）
   - 接口：`init` 与 `ShellInfo`、`onCommand`、`onNavigate`、`openExternal`、`pickDirectory`、`system`、`window`、`prefs`、`updates`。没有 `host.*` 和 `requestPrivileged`，这两项归 T071。
   - 相关类型：`ShellCommand`、`KnownCommandId`、`NavigateTarget`、`KnownNavigateTarget`、`UpdateState`、`Unsubscribe`。
   - 辅助函数：
     - `commandHandler`：只执行已知命令 ID 的处理函数，其余的忽略；
     - `isKnownTarget`：开放枚举的类型守卫；
     - `isExternalUrl`：只认 http 和 https。T074 的 preload 和主进程可以共用这条规则。
   - 浏览器 mock 在子路径 `@cupper/shell-api/browser-mock`，用 `createBrowserShell({ info })` 创建，返回 `{ shell, controls }`。
     - 它完全由测试驱动，通过 `controls` 扮演菜单、系统和主进程。
     - 它不用 DOM，在 Node 里也能跑。
     - 值按 `contextBridge` 的方式复制。
2. **`@cupper/i18n`**（`packages/i18n`）
   - 基于 i18next，以语义码为键，每个功能文件是一个命名空间：`src/locales/zh-CN/command.json`（键是命令 ID）、`update.json`。
   - `createI18n(locale)` 返回独立的实例，返回时已经可以用。其他语言回退到 zh-CN。插值不做 HTML 转义。
   - `escapeInvisible` 把以下字符换成可见的 `\u{XXXX}`：
     - 控制字符（保留 Tab 和换行）；
     - 格式字符，包括双向控制符、零宽字符和标签字符；
     - 行分隔符和段分隔符；
     - 孤立的代理项；
     - Unicode 的 `Default_Ignorable_Code_Point`。
3. **xtask（T001 留下的两点）**
   - **测试跑两遍**：每个包的 `vitest.config.ts` 只收 `src/**/*.test.ts`。实测 Vitest 5 默认仍然会扫 `dist/`：不加配置时是 24 个用例，加了以后是 12 个。
   - **逐用例结果**：
     - ts 作业改为逐个包运行 `test` 脚本，并在末尾补上 JSON reporter 的参数；
     - 新增 `parse_vitest`，跳过和 todo 的用例不计；文件在用例之外失败时，记 1 个失败；
     - 一个包失败后，其余的包照常跑完；
     - `ids::owner` 增加对 JS 标题的匹配：标题以编号原文开头即可，仍然精确匹配、区分大小写。

## 关键取舍

- **依赖版本**：vitest 5.0.3、vite 8.3.3、i18next 26.4.2，只在各包里声明。
  - Vitest 5 把 vite 改成了 peer 依赖，所以要自己声明 vite。
  - vite 取 8.3.3（13 天），没有取更满两周的 8.3.2：10-06 这天它的 5 个版本线同时发版，像是安全修复。
- **包导出 TS 源码**：`exports` 直接指向 `src/*.ts`，由 Vite 和 Vitest 读源码；类型检查走 project references，引用方用 `dist/` 里的 `.d.ts`。相对 import 不写扩展名，跟随 T001 的 bundler 模式。这是第一个 TS 包定下的惯例，请编排者留意。
- **dist 的问题选了"Vitest 只收 src"**，没有选"测试不进 tsc 的产出"，因为后者会让 tsc 不再检查测试文件的类型。
- **`UpdateState` 由我定义**：设计只写了类型名。定为 `idle | checking | downloading | ready | installing | error`：
  - `ready` 就是"重启以更新"，`installing` 是 5.10.5 说的安装空窗；
  - `error` 不带原文，因为更新器的错误信息可能带路径。
- **`prefs.onChange` 的回调签名**：设计没写，定为 `(key, value)`。
- **开放枚举的收窄问题**：设计里 `NavigateTarget` 的 `{ kind: string & {} }` 会让 TS 不再把 `kind` 当判别字段，`t.kind === "session"` 之后访问不到 `session_id`（TS 6.0.3 实测）。
  - 所以拆出 `KnownNavigateTarget` 并提供守卫 `isKnownTarget`。
  - 接口的形状和设计一致。
- **mock 的 `onCommand` 不过滤任何 ID**：命令 ID 是开放枚举，未知 ID 照样送达；忽略未知 ID 的是 `commandHandler`。测试断言了两点：发送未知 ID 不抛错；`toString`、`__proto__` 这类 ID 碰不到任何处理函数。
- **转义范围用 Unicode 属性来写**，不逐个列码点：逐个列会漏，还会触发 ESLint 的 `no-misleading-character-class`。
  - 代价：带变体选择符或 ZWJ 的 emoji 也会显示出转义。这在需要批准的内容里可以接受。
  - 因为这个代价，没有把它挂到所有插值上，只供请求面板和原生确认框显式调用，和设计的范围一致。
  - 空格类（NBSP、全角空格）不转义。
- **M1a 的语言选 zh-CN**：依据是 PRD 访谈的 D7（用户使用中文界面）。

## 验证方式

- [x] **UNIT-shell-mock**（12 个用例）
  - 用 `Members<ShellAPI>` 映射类型列出全部成员，漏写或多写都编译不过；运行时逐一比对 mock 的形状，缺少或多出成员都会失败。
  - 每个成员都有行为测试。
  - `onCommand` 遇到未知 ID 时不报错，处理函数也不会被调用。
- [x] **UNIT-i18n-escape**（6 个用例）
  - 双向控制符（含 U+202E）、零宽字符、C0/C1 控制字符、行和段分隔符、标签字符、变体选择符、孤立代理项都被转义；
  - 增补平面的字符按一个码点转义；
  - Tab 和换行原样保留；
  - 中英文、全角标点、全角空格、希伯来文和阿拉伯文、emoji 原样保留；
  - 重复转义不再变化。
  - 变异检查：去掉 `Default_Ignorable_Code_Point` 后，测试失败。
- [x] **`cargo xtask ci pr --record` 在干净的 worktree 里通过**，ts 作业实际跑到了 tsc 和两个包的 Vitest。
- [x] **每个提交单独通过**：在临时克隆里冷启动，对四个代码提交逐个跑了 `cargo xtask ci pr`，全部通过。
- [x] **失败路径**：在临时克隆里构造了一个断言失败和一个加载失败的测试文件。
  - 退出码为 1；两个包都跑完了；
  - 加载失败按文件名记为失败；
  - 摘要里 `UNIT-shell-mock` 记为 fail。
- [x] **测试的名称以编号开头**：用 `describe("UNIT-…")`。i18n 加载器的 4 个用例和 xtask 解析器的用例没有编号，和 T001 的做法一致。
- [x] **其他**：
  - 全部改动文件扫描过不可见字符，结果为 0；
  - 入库内容里没有本机路径和用户名；
  - 没有依赖需要运行安装脚本，`allowBuilds` 保持为空。

## 提交列表

```
3db4fe3 docs(task): record the T070 pr CI summary
6d620b4 docs(task): add T070 working notes
cd6d2e1 fix(xtask): keep a space between a long step name and its result
6f9ac0b feat(i18n): add @cupper/i18n with the invisible-character escape
db830e5 feat(shell-api): add the protocol-independent ShellAPI and its browser mock
a185312 feat(xtask): record every Vitest case in the ts job
```

## 遗留问题 / 范围外发现

1. **根配置没有改**。改动只在 `packages/shell-api`、`packages/i18n`、`crates/xtask/src/ci/`（`ctx.rs`、`ids.rs`、`jobs/ts.rs`）和 `pnpm-lock.yaml`。
2. **设计 07-desktop.md 第 373 行的示例显示不出来**："显示成可见的转义，如 `‮`"，反引号里实际是一个真正的 U+202E 字符。建议改成 `\u{202E}`，这也是本任务采用的格式。
3. **ShellAPI 里还没有 `pathForFile`（M1b）和 `saveFile`（P1）**：任务书的范围没有列这两项。按"冻结后只加成员"，可以由对应的任务再加；如果 M1a 冻结时就要包含，请编排者决定。
4. **`lightningcss` 的许可证是 MPL-2.0，不在 ADR-11 白名单里**。它是 vite 的依赖，只在开发和测试时使用，不进 App。T006 的许可证门禁要按生产依赖闭包计算，才不会误报。
5. **ts 作业的新约定**：包的 `test` 脚本必须跑 Vitest，作业会在末尾补 reporter 参数。以后有不用 Vitest 的包，要另起脚本名（例如 `test:e2e`）。
6. **i18n 现有的文案**是 `command`（菜单命令名，取自 5.7.7 表格的"动作"一列）和 `update`。措辞由 T074、T154 按 Codex 的中文译文再核对。
7. **工具层面的提醒**：我这边的文件写入工具会把 `\uXXXX` 解码成真实字符，测试源码里一度混进了真实的 U+202E。已经改用脚本生成并扫描确认。之后写这类测试的子 agent 也要注意这一点。
