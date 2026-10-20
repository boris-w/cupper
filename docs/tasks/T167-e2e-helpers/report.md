# T167 汇报

- 执行者：子 agent（测试工程师）
- 分支：`feat/T167-e2e-helpers`
- 完成日期：2026-10-20

## 做了什么

T167 的辅助函数都做完了，每个都在真实 App 上自检过。过程中查出两处阻塞性的产品缺陷：修之前，真实 App 的窗口拿不到任何宿主数据。两处都经你同意在本分支上单独修复。验收各条都通过。

**辅助函数**

| 模块 | 内容 |
|---|---|
| `__cupperTest` | ui-core 的 `mount` 加可选的 `onClient`，经 `App` 传到 `HostProvider`。渲染进程入口只在 `__CUPPER_FLAVOR__ === "e2e"` 时传入，挂上 `window.__cupperTest`（`counters()`、`state()`）。 |
| 非 e2e 构建不含测试入口 | 新的 Vitest 用例 `apps/desktop/src/flavors.test.ts`，在 ts 作业里跑。按 `release`、`dev` 把 main、preload、renderer 构建到临时目录，扫不到 `__cupperTest`、`__cupperE2E`（按窗口断连的入口）、`CUPPER_E2E_DIR`。按 `e2e` 构建作对照，三个都扫得到。 |
| CLI `clock-advance <ms>` | 只在 `test-harness` 构建里有，发 `test/clock_advance`，带单元测试。 |
| CLI 包装（`cli.ts`） | `repl`，按 `repl.exit` 分条；`watch` 加上 `until` 和 `stop`；不带 key、以 `cli` 身份连接。 |
| 对话框（`dialogs.ts`） | `showMessageBoxSync`：记录调用，立即回 `cancelId`。`showOpenDialog`：记录调用，挂起到用例 `choose(序号, 路径 \| "cancel")`。T093 只需要写用例。 |
| `helpers.ts` | 见下表。 |
| `input.ts` | `focusWindow`：让窗口获得焦点，再等过守卫的 1 s。`press`、`type`：经 `webContents.sendInputEvent` 发 keyDown、char、keyUp，支持修饰键。请求面板：`requestPanel`、`panelReady`、`activatePanel`（按 Tab 直到 `data-activation=auto`，再等过守卫的打字冷却）、`pressDigit`。 |
| 夹具 | `host()`、`track()`、`processes()`（App 和宿主的进程树）。`login_env.PATH` 只放系统目录 `/usr/bin:/bin:/usr/sbin:/sbin`，临时仓库的 `is_git` 因此为 true。globalSetup 先查 `xcode-select -p`。 |

`helpers.ts` 里的函数：
- `hostClient`：读 `__cupperTest`。
- `disconnectWindow`：经主进程的 e2e 钩子断开窗口的连接，等出现新的窗口连接。
- `advanceClock`：推进手动时钟。
- `events` 和 `untilEvent`：只读打开 `host.db` 等事件，已经发生的也找得到。
- `untilSession`：用 `sessions --watch` 等会话出现在索引里。
- `killHost`：按进程表确认是本用例的宿主，记下它的适配器，发 SIGKILL，等 App 拉起新宿主。
- `elevate`：`cli` 身份在 `repl` 里 `elevate`，点侧边栏横幅的"查看"，替换后的确认框批准并勾选 approve。
- `sampleNetwork`：对本用例的进程跑一次 `nettop -L 1` 并解析。
- `texts`：按 App 的语言取界面文案。

**`@smoke` 改为从渲染进程一侧断言**：窗口的 HostClient 为 ready，并且侧边栏能看到 CLI 建的项目和会话。下面两处缺陷任何一处还在，它都会失败。

**`@focus` 分段**：
- 真实输入只在有焦点的窗口里算数，而另一个 worker 启动的 App 会抢走焦点。连跑时，请求面板那条在并行下 10 次里失败 3 次；单独跑 6 次全过。
- 现在带 `@focus` 的用例在其余用例之后、单 worker 跑。`pnpm test:e2e` 和 `ci e2e`/`ci nightly` 都分成两段：`test-e2e`、`test-e2e-focus`。
- 目前带这个标签的有四条：提权、按键、请求面板、添加项目。

## 范围外的修复（经你同意，各自单独提交，行数不计入估计）

**1. `fix(desktop)`：preload 的 HostPort 过不了 contextBridge**（新增 27 行，其中测试 15 行；删除 2 行）
- 缺陷：`bridgeHost().open()` 返回的是 `WindowHostPort` 实例，方法都在原型上。`contextBridge` 只复制对象自身的属性，所以 port 到主世界是空对象，`connectShell` 在 `port.onData` 上失败，窗口拿不到任何数据。
- 修法：`open` 返回由箭头函数组成的普通对象。单元测试断言 port 自身就带 `send`、`sendBlob`、`onData`、`ack`、`close`。
- 顺查了整个 preload，没有别的同类问题：
  - `shell` 是对象字面量加箭头函数；
  - `bridgeHost` 返回的 `{open, onConnectionState}` 是普通对象；
  - `Listeners` 不经 contextBridge 暴露，`add` 返回的是箭头函数；
  - 经 IPC 回来的都是数据。

**2. `fix(ui-core)`：`Frames` 以错误的 `this` 调用 `requestAnimationFrame`**（新增 24 行，其中测试 19 行；删除 2 行）
- 缺陷：默认的 frame 函数存的是 `requestAnimationFrame` 本身，调用时写成 `this.frame(...)`，浏览器抛 Illegal invocation。它抛在订阅重置视图时，流就停在 `waiting`，侧边栏一直为空。
- 修法：默认值改成 `(cb) => requestAnimationFrame(cb)`。新用例用一个要求 `this` 为 `globalThis` 的 rAF 替身复现；去掉修复后这条用例确实失败。
- 在 ui-core、host-client、shell-api 里顺查了同类写法，除这一处外全是直接调用，没有别的问题。查过的有：
  - 把 `setTimeout`、`clearTimeout`、`setInterval`、`queueMicrotask`、`requestAnimationFrame`、`cancelAnimationFrame`、`performance.now`、`crypto.*`、`structuredClone`、`fetch`、`Date.now` 存成属性、默认值或变量；
  - 解构 `performance`、`crypto`、`TextEncoder`/`TextDecoder` 的方法。

## 关键取舍

- **规模**：新增 807 行、删除 79 行（不含 lock 和文档）。去掉两处修复，T167 本身新增 756 行，估计约 795，上限约 850。
- **会话流的屏障读 `host.db`，不用 CLI 的 `watch`**：`watch` 只输出订阅生效之后的事件，又看不出订阅何时生效，所以不适合做屏障。索引上等会话用 `sessions --watch`，它会先输出当前状态。
- **夹具的 PATH 只给系统目录**：不用本机用户的 PATH，宿主经 T054 的发现找到 `/usr/bin/git`。`/usr/bin/git` 在没有 Xcode 或命令行工具时只是占位程序（会弹安装框），所以 globalSetup 先查 `xcode-select -p`，没有就直接报错。GitHub 的 macOS runner 自带 Xcode。
- **`activatePanel` 结束前等 1.1 s**：守卫把 Tab 也算作打字键，之后 1 s 内的数字键不算（规则 3）。
- **`local` flavor 不在构建检查里**：Vite 不接受 `local` 作 mode 名，因为与 `.env.local` 冲突。它的代码与 release 相同。

## 验证方式

- **每个辅助至少被一条自检调用过**：自检在 `helpers.spec.ts` 里，8 条，不登记编号，跑在每晚的全集里。辅助与用例的逐条对应写在 `notes.md`。请求面板用的是真实面板：假 agent 发权限请求，按 Tab 激活、按 1 之后面板收起，宿主写下同一请求的 `interaction.resolved`。
- **非 e2e 构建不含测试入口**：`flavors.test.ts` 通过，覆盖 release 和 dev，e2e 作对照。
- **连跑 10 次**：`pnpm test:e2e` 两段（9 条 workers 2，加 4 条 `@focus` 单 worker）连跑 10 次全绿，每次约 17–20 s。每次跑完都没有残留进程，`$TMPDIR/cupper-e2e/` 为空，plist 不在。加 `@focus` 分段之前连跑的 10 次里有 3 次失败，也写进了 `notes.md`。
- **`cargo xtask ci e2e`**：通过，3 项检查、9 条用例、4 条 `@focus`。
- **`cargo xtask ci pr --record docs/tasks/T167-e2e-helpers/ci/`**：在干净的 worktree 里通过，运行期间没有往工作区写任何文件。摘要 `pr-ccadc2532f4f.json` 已提交，确认不含本机路径。

  | 作业 | 结果 |
  |---|---|
  | rust | 1,877 通过 |
  | protocol | 185 通过 |
  | ts | 457 通过 |
  | license | 1 通过 |
  | testdata-scan | 3 通过 |
  | ui | 25 通过 |
  | e2e | 4 通过（`@smoke` 新断言） |

## 提交列表

```
fef093a docs(task): T167 CI summary
ccadc25 docs(task): T167 notes: sizes, self-checks, fixes, choices
2aa39dc test(e2e): run the tests that need the focus last, one at a time
0f13516 test(e2e): the helpers' self-checks; the smoke test sees the data
334beee test(e2e): real keys and the request panel
16cbfb9 test(e2e): helpers for the window, the host, the barriers and the network
87531b8 test(e2e): stand in for the synchronous message box and the open dialog
32c3583 test(e2e): the CLI's repl, watch and plain cli client
fe6a5ac fix(ui-core): call requestAnimationFrame as a plain function
53f2f9a fix(desktop): give the renderer a host port that crosses the context bridge
b3b7757 feat(cli): clock-advance in test builds
474b584 feat(desktop): window.__cupperTest in e2e builds
565e2ea docs(task): T167 size estimate
```

## 遗留问题 / 范围外发现

1. **`vite build --mode local` 本身会失败**：Vite 不允许 `local` 作 mode 名。自用构建（T161 一类）要换个 mode 名，或者 flavor 与 mode 分开映射。
2. **T091–T093 要用 `@focus`**：凡是靠按键或点击算作真实输入的用例，都要打这个标签，否则会在并行下偶发失败。例如 E2E-NFR1-2 经界面批准、E2E-SEC-real-input、面板待命类。
3. **T091 用 `__cupperTest` 时**：它目前只导出 `counters()` 和 `state()`。NFR1-1 要的视图导出由 T091 自己加，加在渲染进程入口的 e2e 分支里，`flavors.test.ts` 会照样检查它不进非 e2e 构建。
4. **机器前提**：e2e 要求本机装有 Xcode 或命令行工具，否则 globalSetup 直接报错。
5. **磁盘**：`ci e2e` 有意保留 `target/host-bundle` 等构建产物，供下次复用。
6. **e2e 抓到的东西**：两处缺陷说明，此前所有任务的单元测试和截图回归都没有在真实 Electron 里验证过数据链路。现在 `@smoke` 会在 PR 上守住这一点。
