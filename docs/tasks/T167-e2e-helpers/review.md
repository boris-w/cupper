# T167 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：T167 本身新增 756 行（估约 795，上限约 850）；加上两处范围外修复共新增 807、删除 79（不含 lock 和文档）。两处修复经编排者同意单独提交，行数单列。
- **CI**：执行者的摘要 `pr-ccadc2532f4f.json` 通过（e2e 4 条，含新的 `@smoke` 断言）。合并 main（含 T163、T085）没有冲突，`ci pr --record` 通过：`pr-a195f06e33c8.json`（rust 1893、protocol 185、ts 465、license、testdata-scan、ui 35、e2e 4）。之后又合入了 T168（改 ui-core 的请求面板、用户气泡和 host-client，与本分支的文件不重叠），在合并结果上补跑 `ci ts` 和 `ci e2e`（全集）通过。
- **验收**：
  - 每个辅助都在真实 App 上自检过（`helpers.spec.ts` 8 条，每晚的全集）；
  - `flavors.test.ts` 证明 release、dev 构建里扫不到 `__cupperTest`、`__cupperE2E`、`CUPPER_E2E_DIR`，e2e 构建作对照；
  - 两段连跑 10 次全绿，无残留进程；`ci e2e` 通过。

## 范围外的两处修复（阻塞性产品缺陷）

T090 之前，没有任何测试在真实 Electron 里走过窗口的数据链路：契约测试在 Node 里跑，截图回归用 mock，T090 的 `@smoke` 只从宿主一侧看到窗口连上。T167 第一次在真实 App 里读数据，就撞上两处：

1. **preload 的 HostPort 过不了 `contextBridge`**（T076 引入）：`open()` 返回类实例，方法在原型上，`contextBridge` 只复制自身属性，port 到主世界是空对象，窗口拿不到任何数据。改为返回由箭头函数组成的普通对象，单元测试断言 port 自身带齐五个方法。顺查整个 preload，没有别的同类问题。
2. **`Frames` 以错误的 `this` 调用 `requestAnimationFrame`**（T080 引入）：默认值存了函数本身、以 `this.frame(...)` 调用，浏览器抛 Illegal invocation，流停在 `waiting`，侧边栏一直为空。改为 `(cb) => requestAnimationFrame(cb)`，新用例用要求 `this` 为 `globalThis` 的替身复现。顺查 ui-core、host-client、shell-api，没有别的同类写法。

**`@smoke` 改为从渲染进程一侧断言**：窗口的 HostClient 为 ready，并且侧边栏看得到 CLI 建的项目和会话。两处缺陷任何一处还在，它都会在 PR 上失败。这是本任务最重要的产出之一。

## 逐项意见

1. **`__cupperTest`**：只读的 `counters()`、`state()`，经 `mount` 的可选 `onClient` 挂上，渲染进程入口按 `__CUPPER_FLAVOR__ === "e2e"` 决定，其他构建经 `define` 去掉，`flavors.test.ts` 守住。
2. **会话流屏障读 `host.db`**：理由成立。CLI 的 `watch` 只输出订阅生效之后的事件，又看不出何时生效；只读打开数据库，已经发生的事件也找得到。
3. **`@focus` 分段**：真实输入只在有焦点的窗口里算数，并行的另一个 App 会抢焦点（请求面板那条并行下 10 次失败 3 次）。带 `@focus` 的用例在其余用例之后单 worker 跑，`ci e2e`、`ci nightly` 和 `pnpm test:e2e` 都分两段。做法正确，代价是总时长略增。
4. **夹具 PATH 只给系统目录**：不读本机用户的 PATH；globalSetup 先查 `xcode-select -p`，避免 `/usr/bin/git` 占位程序弹安装框。
5. **`showOpenDialog` 的替换**挂起到用例 `choose`，T093 只写用例。

## 记下的点

- **T091–T093**：凡是靠按键或点击算真实输入的用例都要打 `@focus`；NFR1-1 要的视图导出由 T091 加在渲染进程入口的 e2e 分支里，`flavors.test.ts` 会照样检查。
- **自用构建（T160、T161）**：`vite build --mode local` 会失败（Vite 不接受 `local` 作 mode 名），要换名或把 flavor 与 mode 分开映射。
- **机器前提**：e2e 要求本机装有 Xcode 或命令行工具。
- **教训**：真实 App 的数据链路只有 e2e 能验证。以后动 preload、`contextBridge`、渲染进程入口或 host-client 连接的任务，CI 里的 `@smoke` 就是守门的用例。
