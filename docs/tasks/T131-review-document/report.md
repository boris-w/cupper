# T131 汇报

- 执行者：子 agent（前端工程师，桌面与渲染进程安全）
- 分支：`feat/T131-review-document`
- 完成日期：2026-11-09
- PR：https://github.com/boris-w/cupper/pull/75
- PR 上的 CI（c3267511，8 个检查全过）：https://github.com/boris-w/cupper/actions/runs/42588398906

## 做了什么

范围按 v1.3.87 的拆分：审查文档 I，即入口、协议与只读视图。hunk 按钮、行内评论和剪贴板核实拆到 T238。

1. **第二个渲染入口**
   - `app://cupper-review/` 是第二个主机，由 `appProtocolHandler` 服务 `dist/review`，用独立的 CSP（`reviewDocumentCsp`），不启用 Trusted Types，`frame-ancestors app://cupper`。
   - 主文档的 CSP 加了 `frame-src app://cupper-review`。两份 CSP 写成函数（`mainDocumentCsp(review)`、`reviewDocumentCsp(main)`），截图工具用它们换成测试源。
   - `navigation.ts`：`will-frame-navigate` 只放行每个 frame 的第一次导航，并且地址必须是 `REVIEW_DOCUMENT_URL`；主框架和之后的导航一律阻止。有单元测试。
   - `reviewDocumentUrl` 经 ShellAPI 下发：`app://cupper-review/review.html`。
   - vite 新增 `review` 环境（`consumer: "client"`），Worker 单独打包（`worker.format: "es"`），两者都写元数据。许可证门禁的第一层多登记了 `review` 和 `review worker` 两项。
   - 4 份 electron-builder 配置都加了 `dist/review/**`。这是本机 e2e 发现的：不加的话审查文档返回 404。
   - 浏览器 mock 和截图工具：审查文档从第二个源 `https://cupper-review.test` 加载，两个文档各按生产 CSP；违规收集覆盖所有 frame。

2. **@pierre/diffs 1.6.4**（`packages/ui-core/src/review/document/`，经 `@cupper/ui-core/review-document` 导出）
   - 用 pierre 的 `CodeView`。它自己的 div 就是滚动容器，满足 report-q29 第 6 节第 2 条；只渲染屏幕附近的行。
   - `WorkerPoolContextProvider` 随文档挂载，不等第一个 diff，`workerFactory` 以 module Worker 从审查源启动 `@pierre/diffs/worker/worker.js`。所以预加载的同时也预热了 Worker。
   - 高亮固定 `shiki-js`。`langs: []`，语言在第一次高亮某种语言的文件时才解析，因此只加载 diff 里出现的语言。
   - 文件头由我们自己渲染：折叠按钮、状态图标、路径、增删数，以及二进制和文件模式的说明。文件头渲染时（文件进入渲染窗口）才发 `need_file`，这就是懒加载。
   - hunk 分隔用 `simple`：pierre 默认的分隔会显示英文的 "N unmodified lines"，不能走 i18n。
   - 字体、字号取 token（`--diffs-font-family` 等）。
   - `auto` 视图的阈值：iframe 宽度不小于 800 时分栏，小于 800 时统一视图。

3. **版本化的 MessageChannel 协议**（`review/protocol.ts`，两侧共用一份 zod schema）
   - 下行：`init`、`theme_changed`、`files`、`render`、`scroll_to`、`clear`。
   - 上行：`ready`、`need_file`、`set_collapsed`、`hunk_action`、`draft_comment`、`key_forward`。
   - 审查文档只认第一个端口：`event.source` 是父窗口、`event.origin` 等于 `location.ancestorOrigins[0]`、带着一个端口、内容是合法的 `init`。收下后就移除 `window` 上的监听。
   - 主文档只听自己建的那个端口。每条消息都经 zod 解析，不合格的丢掉。
   - `patchOf` 把宿主的 hunk 拼成 patch：`whole_file` 单元有多个 `@@` 时，按头部的行数分配各段的行。

4. **主文档一侧（`ReviewFrame.tsx`）**
   - iframe 加 `sandbox="allow-scripts allow-same-origin"`、`allow="clipboard-write"`。iframe 每次加载都新建一个通道。
   - `ReviewViewProps` 译成 `files`、`render`、`scroll_to`；主题跟随 `onThemeChange`。
   - 自己取 `git/file_diff`（T130 约定的查询键），`lines_blob` 先取 blob。
   - `hunk_action`：令牌一致、操作在范围之内、`hunk_ids` 都在自己取到的 hunk 里，才调 `act`。还原只进 T130 的确认控件（来源为 `view`）。`draft_comment` 校验后才调 `comment`。
   - `key_forward` 在 iframe 元素上派发合成的 `keydown`，不算真实输入。
   - 取 `file_diff` 失败时，这个文件保持"正在加载"。
   - 替换并删除了 `ReviewPanel.tsx` 里的占位 `FileList`，连同 T232 给它加的 `focus` 滚动；现在由审查文档自己响应 `focus`。diff 视图和 240 宽的文件树并排。

5. **常驻**
   - 侧边面板第一次打开后、或窗口第一次空闲时（`requestIdleCallback`）挂载，之后只隐藏、不卸载。
   - `Review` 不按会话加 key，iframe 跨会话保留；没有会话、没有改动时也只是隐藏。会话相关的状态单独重置：文件树按会话加 key，`useActions` 换会话时清掉待确认的还原。
   - 面板隐藏或没有会话时，`git/diff`、`git/status` 都不发请求。合并 T233 时也给 `GitActions` 加了可选的 `active`，同样处理。

6. **用例**
   - **E2E-SEC-review-iso**（pr，`@smoke`），逐项核对：
     - 两个文档的源不同，审查文档访问 `parent.document` 得到 `SecurityError`；
     - 文件里的 `<style>`、`<img>`、`<a>`、`<base>`、`<link>`、`<meta>` 在两个文档里都按文本显示；
     - 发给审查文档的 window 消息、第二个端口都被忽略；审查文档在 window 上发的消息也被忽略；
     - 令牌过期的还原、未取到的 hunk 的暂存都不执行；端口上合法的还原只弹出确认，文件不变；
     - 页面缩放会传到 iframe；两个文档都没有 CSP 违规。
   - **UNIT-review-injection**：截图工具里的 Playwright 用例。文件名和行里的标记按文本显示，两个文档里都找不到 `data-injected` 元素。
   - 协议的单元测试：schema 的拒绝、`patchOf`、多个 `@@` 的分配。
   - 审查文档的行为用例：折叠、从文件树跳转、按键转发、主题。

## 依赖

- **@pierre/diffs 1.6.4**（Apache-2.0）
  - 2026-11-04T00:39:01Z 发布，加入时还没过 pnpm 12 默认的 1 天最短发布时间，所以开发期间在 `pnpm-workspace.yaml` 里加了临时例外 `minimumReleaseAgeExclude: ['@pierre/diffs@1.6.4']`。
  - 加入例外：`2a3ba451`（WIP 提交里）；格式化：`83e3e673`；去掉例外：`69676119`（2026-11-09）。去掉之后 `pnpm install --frozen-lockfile` 通过，PR 上最后一轮 CI 就是没有例外的。
  - 1.6.3 → 1.6.4 的核对：文件清单相同，`dist/` 逐字节一致，只有 `package.json` 不同。1.6.4 把 1.6.3 的 `workspace:*`、`catalog:` 依赖写成了精确版本，并去掉了 `prepublishOnly`，没有任何 scripts。也就是说只修了发布元数据，没有新增的安装脚本，也没有新增网络或文件访问。
  - 带进来的依赖：shiki 4.6.0（经 pierre 的 `^3 || ^4`）、`@pierre/highlights`、`@pierre/theme`、`@pierre/theming`、`diff`、`hast-util-to-html`、`lru_map`。许可证门禁核对通过：review 36 个、review worker 31 个 npm 包。
- **zod 4.6.7** 加进 ui-core，版本与 `@cupper/protocol`、`@cupper/host-client` 相同。
- **共享工具改动：许可证门禁改为对 SPDX 标识大小写不敏感**（`051989c8`，`tools/license-gate/src/gate.ts`）。`@pierre/*` 的 `package.json` 写的是小写 `apache-2.0`，原来按精确比较会判为不在清单里；补了单元测试。这一改动影响所有 npm 依赖的判定，规则只是放宽大小写，清单不变。
- **体积**：`dist/review` 约 17 MB，主要是 shiki 的语法和主题，每种一个 chunk，按需加载。Worker 包里又带了一份语法，因为 pierre 的 worker 代码里有动态 import；实际上语言由主线程解析后发给 Worker，Worker 里那份不会加载。

## 关键取舍

- **用 `CodeView`，不用 T104 原型的 `Virtualizer` 加 `MultiFileDiff`**：`CodeView` 是 pierre 为多文件准备的虚拟列表，自带按文件折叠、滚到文件或行、吸顶的文件头，而且自己就是滚动容器。
  - 实测踩到一处：pierre 按 `cacheKey` 缓存布局，所以只给带行的 diff 设 `cacheKey`。否则空的占位 diff 和后来的 diff 用同一个键，后者不会重新排版。
- **常驻的实现**：`Review` 不按会话加 key。如果按会话加 key，iframe 每次换会话都要重新加载；冷启动高亮要 2.5–3 秒（report-q29）。代价是会话相关的状态要单独重置，见上文。
- **地址用 `review.html`，不用 `index.html`**：vite 的 `root` 是全局的，两个入口在同一个根目录下，文件名不能重复。
- **审查文档认的源用 `location.ancestorOrigins[0]`**：生产里由 CSP 的 `frame-ancestors app://cupper` 保证它只能是 `app://cupper`；mock 下就是截图页的源。
- **拆分**：开工前估约 1,320 行，超过当时的硬线，拆出 T238，停止线随之改为 1,250，硬线 1,400。

## 与设计 5.7.5 消息表的差异

- `render {diff_token, path, hunks, drafts, expanded}` 拆成两条：`files {diff_token, files, collapsed, view, wrap}` 是整个 diff 的文件和视图状态，`render {diff_token, path, hunks}` 是一个文件的 hunk。草稿和范围的操作要到 T238 才用，T131 不下发，不留没人用的字段。
- `need_file`、`hunk_action`、`draft_comment` 都带 `diff_token`，主文档按令牌丢掉过期的消息。`hunk_action` 带 `path`，`hunk_ids` 可以不带，不带表示整个文件。
- `draft_comment` 直接用协议的 `ReviewComment`；`key_forward` 的修饰键拆成四个布尔值。
- 没有 `locale_changed`：外壳没有运行时切换语言的事件，加了也不会有人发。
- `init` 不带缩放：页面缩放由 Chromium 传给进程外 iframe。E2E-SEC-review-iso 核对了两个文档的 `devicePixelRatio` 一致。

## 2026-11-09：merge main、去掉例外、新基线

- **merge main（ad61061d，含 T132、T140、T142、T241、T243）**，合并提交 `94c86150`，三处冲突按交接事项里 T132 的接口解决：
  - `state.ts`：用 main 的 `DiffScope`、`diffScope()` 和带 `enabled` 的 `useDiff`；两边都加了 `enabled`，去掉重复。
  - `ReviewPanel.tsx`：store 改由 `Shell` 经 `ReviewContext` 提供，保留 T132 的"别的会话的 turn 回到未暂存"；`Review` 仍不按会话加 key，保证 iframe 常驻；`turn` 范围的操作取 `ACTIONS.turn`，只有还原。
  - `review.test.tsx`：用 main 包的 `ReviewContext`，补上 `active`。
- **T132 的截图用例**（`522c6ed9`）："改动卡片打开审查面板"原来在面板里找占位列表的文件名。现在改在审查文档里找，夹具补了 `git/file_diff`；面板头部的总增删数仍在主文档里检查。
- **去掉临时例外**（`69676119`），见"依赖"。
- **新基线**（`c3267511`）：ui-baseline 42586061723 的 66 张里只有 `stream-changes-review.png` 变了。差异全在右侧面板，是审查文档换掉了占位视图；其他基线和 meta.json 都没变。

## 验证方式

- 本机，都是单份：
  - tsc 通过（ui-core、desktop、shell-api、ui-screenshots、e2e、license-gate）；
  - vitest：ui-core 121 个、desktop 103 个、shell-api、license-gate 都通过；
  - 全仓 eslint、prettier 通过；`license-gate check` 没有问题；
  - E2E-SEC-review-iso：本机构建 e2e 版本后单份跑过，通过。测试按后台模式运行：窗口不激活，没有真实按键，不碰剪贴板。
  - 截图工具：用临时配置在无头 Chromium 里跑了 review、stream、shell，功能都通过，没有写入基线。
- runner 上：
  - e2e-repeat，E2E-SEC-review-iso × 10，全过：https://github.com/boris-w/cupper/actions/runs/41789627056
  - ui-baseline 41790326784（a03c6808）：review 8 张、review-tree 4 张、review-commit 3 张，逐张核对后提交（30241dbf），diff 视图都渲染了行号和高亮；
  - ui-baseline 42586061723（69676119）：只变了 `stream-changes-review.png`，提交 c3267511；
  - PR #75 在 c3267511 上 8 个检查全过：https://github.com/boris-w/cupper/actions/runs/42588398906

对照验收标准：

| 验收项 | 结果 |
|---|---|
| E2E-SEC-review-iso 通过，并在 runner 上连跑 10 次 | 通过（41789627056） |
| UNIT-review-injection 通过 | 通过，在 PR 的 ui 检查里 |
| 审查文档里的 securitypolicyviolation 为 0 | 是，E2E 和截图用例都核对了 |
| 新入口过了许可证门禁 | 是：review 和 review worker 两项，PR 的 license 检查通过 |
| 渲染进程的安全约束都守住 | 是：ui-core 不用 innerHTML 一类写法；文案走 i18n；agent 的字符串不进样式；还原只经确认控件 |
| PR 上 8 个检查全过 | 是（42588398906） |
| review 和文件树的基线只由 ui-baseline 生成 | 是 |

## 规模

- `cargo xtask size`：+1,266 −134（不含文档、基线、lock 文件），在改过的硬线 1,400 以内。
- merge main 前是 +1,245；多出来的约 20 行是冲突解决和 T132 用例的夹具。

## 提交列表

```
c3267511 test(ui-screenshots): T132's review shot with the review document
69676119 chore(deps): drop the release-age exclusion for @pierre/diffs
522c6ed9 test(ui-screenshots): a turn's changes open in the review document
94c86150 Merge branch 'main' into feat/T131-review-document
30241dbf test(ui-screenshots): baselines with the review document
a03c6808 Merge branch 'main' into feat/T131-review-document
55d30dd9 fix(ui-core): a file_diff that fails leaves the file loading
c796ac7c docs(T131): notes on the review document's structure and protocol
83e3e673 chore(deps): format the temporary release-age exclusion
f08848d0 test(e2e): E2E-SEC-review-iso
ff36bf5f build(desktop): pack the review document
bbd5113a test(review): the review document's protocol, injection and behaviour
4ed2be5f test(ui-screenshots): serve the review document from a second origin
96790996 feat(desktop): build and serve the review document
2a98b055 feat(ui-core): the review document and its frame in the review panel
f934d83b Merge branch 'main' into feat/T131-review-document
2a3ba451 wip(review): review document origin, CSPs, port protocol and frame (unfinished)
051989c8 fix(license-gate): match SPDX identifiers whatever their case
```

（`2a3ba451` 是 2026-11-04 用户要断网时按要求提交的 WIP，后续提交在它的基础上完成。）

## 遗留问题 / 范围外发现

- **T238（已在本机准备好）**：hunk 按钮、行内评论、剪贴板的核实。已 merge T131 分支，等 T131 合并后 merge main 再开 PR。`turn` 范围的处理写在它的 notes 里。
- **行号跳转**：`scroll_to` 支持 `line`，交给 `CodeView.scrollTo({type: "line"})`。现在主文档只有文件树会设 `focus`，而且只带路径；带行号的跳转（例如消息流里的编辑条目）由以后的任务来设。
- **体积**：Worker 包里重复的那份 shiki 语法会多占约一半的 `dist/review`。要去掉，需要 pierre 提供不带动态 import 的 worker 入口，或者我们自己打一个精简的 worker；目前不影响运行，记作可选的优化。
- **新会话页**：iframe 在没有会话时也会预加载，只是隐藏，不会发 `git` 请求。
- **T145 和 T238 的基线**：T145 给输入框加了"+"，T131 合并后，两边各自 merge main 时都要重新生成受影响的基线。
