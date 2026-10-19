# T070 工作笔记

## 依赖版本（2026-10-19 选定）

原则同 T001：选发布满约两周的版本。三个包都只在 `packages/*` 里声明，根清单没有改。

| 包 | 版本 | 发布日期 | 理由 |
|---|---|---|---|
| vitest | 5.0.3 | 09-30 | 5.x 是当前主线（5.0.0 发布于 09-03）。5.0.4、5.0.5 不满两周 |
| vite | 8.3.3 | 10-06 | Vitest 5 把 vite 改成了 peer 依赖，要自己声明。10-06 这天 8.3.3、8.2.4、8.1.6、7.3.7、6.4.4 同时发布，像是安全修复，所以取 8.3.3（13 天），没有取 8.3.2（18 天）。T074 用 Vite 构建时可以沿用 8.x |
| i18next | 26.4.2 | 09-03 | 26.4.3 起都不满两周。没有运行时依赖 |

- 没有任何依赖需要运行安装脚本，`allowBuilds` 保持为空。
- 新增依赖的许可证都在 ADR-11 白名单内，只有 `lightningcss`（MPL-2.0）例外：它是 vite 的依赖，只在开发和测试时使用，不进 App。

## 关键取舍

- **包导出 TS 源码**：`exports` 直接指向 `src/*.ts`。使用方（ui-core、桌面端的主进程和 preload）都经 Vite 打包，Vitest 也直接读源码。类型检查走 project references，引用方会改用被引用包 `dist/` 里的 `.d.ts`。`dist/` 里的 JS 目前没有人用。
- **相对 import 不写扩展名**：跟随 T001 的 `moduleResolution: bundler`。
- **T001 遗留 1（测试跑两遍）**：每个包的 `vitest.config.ts` 只收 `src/**/*.test.ts`。实测 Vitest 5 的默认值仍然会扫 `dist/`：不加配置时是 2 个文件、24 个用例，加了以后是 1 个文件、12 个用例。没有选"测试不进 tsc 的产出"，因为那样 tsc 就不检查测试文件的类型了。
- **T001 遗留 2（逐用例结果）**：
  - ts 作业改为按包逐个跑 `pnpm --dir <包> run test --reporter=default --reporter=json --outputFile.json=<日志目录>/ts-test-<包>.json`。约定是：包的 `test` 脚本跑 Vitest，作业在末尾补上 reporter 参数，pnpm 会把这些参数传给脚本。
  - 一个包失败后，其余的包照常跑完，最后一起报错。
  - 用例命名为 `<相对路径>::<describe>…::<标题>`。
  - 跳过和 todo 的用例不计。如果一个文件在用例之外失败（加载不了、hook 抛错），而它的用例都通过了，就按文件名记 1 个失败。
  - 报告缺失或无法解析时，记 1 个失败。
  - 编号匹配：`ids::owner` 原来只认 snake_case（Rust 测试名），现在也认"段首就是编号原文，后面紧跟一个不能延续编号的字符"（JS 标题，如 `UNIT-shell-mock …`）。仍然是精确匹配，区分大小写。
- **用例编号写在 describe 上**：`describe("UNIT-shell-mock", …)`。完整名称以编号开头，各条用例的标题只写行为。
- **`UpdateState` 由本任务定义**：设计只写了类型名。定义成 `idle | checking | downloading | ready | installing | error`，其中 `ready` 就是"重启以更新"，`installing` 是 5.10.5 说的安装空窗。`error` 不带原文，因为 electron-updater 的错误信息可能带路径。
- **`prefs.onChange` 的回调签名**：设计没写，定为 `(key, value)`。
- **开放枚举的收窄问题**：设计里的 `NavigateTarget` 有一项 `{ kind: string & {} }`，它会让 TS 不再把 `kind` 当判别字段，`t.kind === "session"` 之后访问不到 `session_id`（实测 TS 6.0.3）。所以：
  - 拆出 `KnownNavigateTarget`，提供守卫 `isKnownTarget`；
  - 对命令提供 `commandHandler`，只执行已知 ID 的处理函数，其余的忽略。
  - 两者都用 `Object.hasOwn` 判断，`toString`、`__proto__` 这类 ID 不会碰到原型上的东西。
- **浏览器 mock 完全由测试驱动**：通过 `controls` 扮演菜单、系统和主进程。它不读 `matchMedia`、不监听 focus，也不碰 DOM，所以在 Node 里也能跑，截图测试也能做到确定性。值按 `contextBridge` 的方式复制（`structuredClone`）；`prefs.set` 遇到不能复制的值时返回 reject。`reviewDocumentUrl` 默认是 `about:blank`。
- **`isExternalUrl`** 放在 shell-api 里导出，T074 的 preload 和主进程可以共用同一条规则。
- **i18n**：
  - M1a 的语言是 zh-CN（PRD 访谈 D7：用户使用中文界面），其他语言回退到 zh-CN。
  - 两个功能文件：`command`（键是 ShellCommand 的 ID，文案取自 5.7.7 快捷键表的"动作"一列）和 `update`。
  - `createI18n` 用 `createInstance`，不用全局实例；资源随包打进来，`initAsync: false`，返回时已经可以用。
  - 插值关掉 HTML 转义（`escapeValue: false`）：React 和原生确认框都按纯文本显示，转义了反而会显示出 `&lt;`。
  - 考虑过把 `escapeInvisible` 挂成所有插值的转义函数，没有这样做：设计只要求请求面板和原生确认框转义，挂到全部插值上会把普通文案里的 emoji（ZWJ 序列、变体选择符）也转义掉。
- **不可见字符的范围**：
  - Cc（保留 Tab 和换行）、Cf、Cs、Zl、Zp；
  - Unicode 的 `Default_Ignorable_Code_Point`，即"渲染时不显示"的字符清单，包括 Hangul 填充符、CGJ、变体选择符、标签字符等。
  - 用属性来写，而不是逐个列码点。逐个列的时候，ESLint 的 `no-misleading-character-class` 会把相邻的组合字符报成错误，而且清单会漏。
  - 代价：带 VS16 或 ZWJ 的 emoji 也会显示出转义。在需要批准的内容里，宁可这样。
  - 空格类（NBSP、U+3000 全角空格）不转义，中文里经常出现。
  - 转义格式是 `\u{XXXX}`（大写十六进制），和 JS、Rust 的写法一致。
- **CR 也转义**：CRLF 会显示成 `\u{D}` 加换行。命令里出现 CR 本来就值得注意。

## 试错

- 我这边的文件写入工具会把 `\uXXXX`（4 位的形式）解码成真实字符，于是测试源码里一度出现了真实的 U+202E 和 U+200B。后来改用脚本生成这两个文件，入库前对全部改动文件扫描了一遍不可见字符，结果为 0。源码里的这类字符一律写成转义。
- `string & {}` 不会触发 typescript-eslint 的 `no-empty-object-type`。预先写的 disable 注释成了"未使用的指令"告警，已经删掉。
