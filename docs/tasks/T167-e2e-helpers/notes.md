# T167 工作笔记

- 执行者：子 agent（测试工程师；T090 的执行者接着做）

## 开工前读过的

- 任务书；T090 的 `notes.md`（模块 6–14）和 `review.md`；`handoff-notes.md` 第一列含 T090、T167、e2e 的行。
- 请求面板（T083）：`RequestPanel.tsx`，它的行为决定了几个辅助的写法：
  - 待命 500 ms 后设 `data-ready`；
  - 获得焦点时激活，`data-activation` 变为 `auto`；
  - 激活前除 Tab 外的按键都被吞掉。
- 输入守卫：`main/input/index.ts`、`guard.ts`。按键只在主框架有焦点时算数；窗口获得焦点后 1 s 内的按键不算。
- 侧边栏：
  - 会话行是按钮，名字是会话标题；
  - 提权横幅里的按钮文案是 `sidebar:review`，点了经 `privileged({kind: "elevation"})` 弹原生确认框。
- 添加项目：`main/shell.ts` 调 `dialog.showOpenDialog`。
- `nettop -L 1 -n -x` 的输出：每个进程一行（`名字.pid`），下面跟它的连接行（`tcp4 本地<->远端`）；`-p` 可以重复给。

## 开工前的规模估计

行数指格式化后的行，含注释。实现按 1.15 倍、测试按 1.1 倍放大。

**实现**

| # | 模块 | 内容 | 草估 |
|---|---|---|---|
| 1 | `__cupperTest` | ui-core 的 `mount` 加可选的 `onClient`，经 `App` 传到 `HostProvider`；渲染进程入口只在 e2e 构建里传入，挂上 `window.__cupperTest`（`counters()`、`state()`）；用例侧的读取 | 40 |
| 2 | CLI `clock-advance <ms>` | 参数解析和执行，`#[cfg(feature = "test-harness")]` 门控 | 25 |
| 3 | CLI 包装补齐 | `repl`（按 `repl.exit` 分条）；`watch` 和 `until(谓词)`；以 `cli` 身份连接 | 65 |
| 4 | 按窗口断连 | 经主进程的 `__cupperE2E.disconnectWindow` 断开，等窗口重新连上 | 20 |
| 5 | 事件屏障 | `clock-advance`；在会话流上等事件；在 `sessions --watch` 上等会话出现 | 40 |
| 6 | `kill -9` 宿主 | 取 pid，按进程表确认是本用例的宿主，SIGKILL，等它消失，等新宿主连上（pid 变了） | 35 |
| 7 | CLI 提权 | `cli` 身份在 `repl` 里 `elevate`；点横幅；替换后的确认框批准并勾选 approve；等授权返回 | 45 |
| 8 | 外发采样 | App 和宿主的进程树；`nettop -L 1 -n -x -p …` 采样并解析成（pid、协议、本地、远端、状态） | 50 |
| 9 | 按键 | `sendInputEvent` 发 keyDown、char、keyUp；修饰键和键名；先让主框架获得焦点，并过了 1 s 冷却 | 45 |
| 10 | 请求面板 | 等 `data-ready`；按 Tab 直到 `data-activation=auto`；按数字键 | 30 |
| 11 | 原生对话框补齐 | `showMessageBoxSync`（立即答 `cancelId`，并记录）；`showOpenDialog` 挂起到用例给出路径或取消，并记录 | 30 |
| | 实现小计 | | **约 425**，×1.15 ≈ **490** |

**测试**

| 用例 | 草估 |
|---|---|
| 非 e2e 构建不含测试入口：Vitest 用例，以 `release`、`dev` 两种 flavor 把 main、preload、renderer 构建到临时目录，扫 `__cupperTest`、`__cupperE2E`；`e2e` flavor 作对照，必须扫到 | 50 |
| `__cupperTest` 与断连：断开后重新连上，缺口、重复、重置计数都为 0 | 30 |
| 屏障与 `clock-advance`：手动时钟下推进后会话出现在索引里；在会话流上等到 `turn.completed` | 30 |
| `kill -9`：宿主被杀后 UI 拉起新宿主 | 25 |
| CLI 提权：经横幅和确认框批准，之后 `repl` 里写成功 | 30 |
| 外发采样：能取到本用例进程树的样本 | 15 |
| 按键：经 `sendInputEvent` 在输入框里打字 | 20 |
| 请求面板：假 agent 的权限请求；打开会话，等 `data-ready`，按 Tab 激活，按 1；假 agent 回显所选的选项 | 40 |
| 对话框：添加项目时的 `showOpenDialog` 拿到路径，项目加进来；`showMessageBoxSync` 的替换 | 33 |
| CLI 单元测试补 `clock-advance` 的解析 | 5 |
| 测试小计 | **约 280**，×1.1 ≈ **305** |

**合计约 795 行**，在约 850 的线以内，开工。

## 做完之后

### 实际规模

按 `git diff --numstat main...HEAD`（不含 `pnpm-lock.yaml` 和文档）：新增 807 行，删除 79 行。其中两处范围外的修复单独统计、不计入估计：preload 27 行（含测试 15 行），ui-core 24 行（含测试 19 行）。T167 本身新增 756 行，估计约 795。

| 模块 | 新增 |
|---|---|
| `__cupperTest`：`mount` 的 `onClient`、`HostProvider`、渲染进程入口 | 31 |
| 非 e2e 构建不含测试入口（Vitest） | 50 |
| CLI `clock-advance`（含单元测试） | 27 |
| CLI 包装：`repl`、`watch`/`until`/`stop`、`cli` 身份 | 121 |
| 对话框：`showMessageBoxSync`、`showOpenDialog`、`choose` | 48 |
| `helpers.ts`：`hostClient`、`disconnectWindow`、`advanceClock`、`events`/`untilEvent`、`untilSession`、`killHost`、`elevate`、`sampleNetwork`、`texts` | 192 |
| `input.ts`：`focusWindow`、`press`、`type`、请求面板三件 | 76 |
| 夹具：`host()`、`track()`、`processes()`、系统 PATH；setup 的 Xcode 检查；tsconfig 的 DOM | 25 |
| 自检 `helpers.spec.ts`；`@smoke` 改为从渲染进程断言看到数据 | 168 |
| `@focus` 分段：xtask、`test:e2e` 脚本、配置注释 | 18 |

### 每个辅助的自检

| 辅助 | 自检用例（`helpers.spec.ts`，不登记编号） |
|---|---|
| `hostClient`（`__cupperTest`）、`disconnectWindow` | `__cupperTest and dropping the window's connection`：断连后有新的窗口连接；计数全 0 |
| `advanceClock`（CLI `clock-advance`）、`untilEvent`、`events`、`untilSession`（`watch`、`until`） | `the clock, the index and the session's events`（手动时钟）：等到 `turn.completed`；推进 1 s 后索引列出会话；`project.upserted` 的 `is_git` 为 true |
| `killHost`、`track` | `kill -9 the host, and the app starts another`：新宿主 pid 不同，窗口重新 ready |
| `elevate`（`repl`、`cli` 身份、横幅、确认框） | `the CLI elevates through the banner and the dialog, then writes` `@focus`：批准后 `repl` 里 `new` 成功 |
| `sampleNetwork`、`processes()` | `a network sample has the app's flows`：主进程自己开一条回环连接，样本里有它 |
| `focusWindow`、`press`、`type` | `keys as real input type into the composer` `@focus` |
| `requestPanel`、`panelReady`、`activatePanel`、`pressDigit` | `the request panel, armed, tabbed into and answered with a digit` `@focus`：假 agent 的权限请求，按 1 后面板收起，宿主写下同一请求的 `interaction.resolved` |
| `Dialogs.choose`（`showOpenDialog`）、`showMessageBoxSync` | `the open and the synchronous dialogs` `@focus`：经侧边栏添加项目，选临时仓库后侧边栏出现它；同步消息框答 `cancelId` 并被记录 |
| `Dialogs.next`、`answer`（T090） | E2E-quit-confirm；`elevate` |

### 非 e2e 构建不含测试入口

`apps/desktop/src/flavors.test.ts`（在 ts 作业里跑）：
- 用 Vite 把 main、preload、renderer 分别按 `release`、`dev` 构建到临时目录，扫 `__cupperTest`、`__cupperE2E`（按窗口断连的入口）、`CUPPER_E2E_DIR`，一个都没有；
- 按 `e2e` 构建作对照，三个都在。
- `local` 不能当 Vite 的 mode 名（与 `.env.local` 冲突），所以没有列进去；它的代码与 release 相同。
- 范围外发现：`vite build --mode local` 本身就会失败，自用构建（T161 一类）要换个 mode 名。

### 范围外的修复（编排者同意，各自单独提交）

1. **preload 的 HostPort**（`fix(desktop)`）：
   - 缺陷：`bridgeHost().open()` 返回 `WindowHostPort` 实例，方法都在原型上，`contextBridge` 只复制对象自身的属性，port 到主世界是空对象。渲染进程的 `connectShell` 因此在 `port.onData` 上失败，真实 App 里窗口拿不到任何宿主数据。
   - 修法：`open` 返回由箭头函数组成的普通对象。
   - 测试：断言 port 自身就带 `send`、`sendBlob`、`onData`、`ack`、`close`。
   - 顺查整个 preload：`shell` 对象是字面量加箭头函数；`bridgeHost` 返回的 `{open, onConnectionState}` 是普通对象；`Listeners` 不经 `contextBridge` 暴露，`add` 返回的是箭头函数；经 IPC 回来的都是数据。没有别的同类问题。
2. **ui-core 的 `Frames`**（`fix(ui-core)`）：
   - 缺陷：默认的 frame 函数存的是 `requestAnimationFrame` 本身，以 `this.frame(...)` 调用，浏览器抛 `Illegal invocation`。它抛在订阅重置视图时，流就停在 `waiting`，侧边栏一直为空。
   - 修法：默认值改成 `(cb) => requestAnimationFrame(cb)`。
   - 测试：用一个要求 `this` 为 `globalThis` 的 rAF 替身复现。
   - 顺查 ui-core、host-client、shell-api：搜了把 `setTimeout`、`clearTimeout`、`setInterval`、`queueMicrotask`、`requestAnimationFrame`、`cancelAnimationFrame`、`performance.now`、`crypto.*`、`structuredClone`、`fetch`、`Date.now` 存成属性、默认值或解构变量的写法，以及解构 `performance`、`crypto`、`TextEncoder`/`TextDecoder` 方法的写法。除这一处外，全是直接调用。查过，没有别的。

### 其他取舍

- **事件屏障分两种**：
  - 会话流上的事件从 `host.db` 读（只读打开，`node:sqlite`），已经发生的也找得到；
  - 索引上的会话用 `sessions --watch`，它先输出当前状态。
  - CLI 的 `watch` 只输出订阅生效之后的事件，没办法确认已经生效，不适合做屏障。`Output.until` 仍然给 T091 用。
- **夹具的 PATH**：
  - `login_env.PATH` 只放 `/usr/bin:/bin:/usr/sbin:/sbin`，不用本机用户的 PATH。宿主经 T054 的发现找到 `/usr/bin/git`，临时仓库的 `is_git` 为 true。
  - `/usr/bin/git` 在没有 Xcode 或命令行工具时只是占位程序（会弹安装框），globalSetup 先查 `xcode-select -p`，没有就直接报错。GitHub 的 macOS runner 自带 Xcode。
- **`@focus`**：
  - 真实输入只在有焦点的窗口里算数，另一个 worker 启动的 App 会抢走焦点。连跑 10 次时，请求面板那条在并行下失败了 3 次；单独跑 6 次全过。
  - 现在带 `@focus` 的用例在其余用例之后、单 worker 跑（`pnpm test:e2e` 和 `ci e2e`/`nightly` 都是两段）。
  - 目前带 `@focus` 的四条：提权、按键、请求面板、添加项目。T091–T093 凡是要按键或点击算作真实输入的，都要打这个标签。
- **`activatePanel` 结束前等 1.1 s**：Tab 在守卫看来也是打字键，之后 1 s 内的数字键不算（规则 3）。
- **`@smoke`**：改成从渲染进程这一侧断言：窗口的 HostClient 为 ready，并且侧边栏里能看到 CLI 建的项目和会话。上面两处缺陷任何一处还在，它都会失败。

### 验证

- 套件 `pnpm test:e2e` 连跑 10 次全绿：第一段 9 条（workers 2），第二段 `@focus` 4 条（单 worker），每次约 17–20 s。每次跑完都没有残留进程，`$TMPDIR/cupper-e2e/` 为空，plist 不在。
- 加 `@focus` 分段之前也连跑过 10 次：3 次有请求面板那条失败，其余全绿，同样无残留。这就是分段的依据。
