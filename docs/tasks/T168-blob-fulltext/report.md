# T168 汇报（请求面板与用户气泡的全文）

- 执行者：子 agent（前端工程师）
- 分支：`feat/T168-blob-fulltext`
- 完成日期：2026-10-20

T168 已完成，`cargo xtask ci pr --record` 在干净的 worktree 里全部通过，摘要 `pr-ba45c499a1a2.json` 已提交：
- 请求面板里被截断的内容，展开就下载全文，转义后显示；
- 请求里每个 blob 都展开、而且载入完成以后，允许类选项才放开；
- 用户气泡里的长文本块，点"显示更多"才下载。

规模：新增 387 行，删除 67 行，超过约 350 的停止线，已按裁定接受。这次是全部写完才统计行数，没有每个模块统计一次，下个任务改正。

## 做了什么

**host-client `getBlob(ref)`**（`client.ts`，挨着 `putBlob`）
- 发 `blob/get`，同时经 `onBlobChunk` 按 `offset` 收集这个哈希的分片。分片一定先于结果到达，所以拿到结果时分片已经齐了。
- 下面几种情况一律报错：
  - 分片按 offset 排序后有缺口或超长；
  - 片数不等于结果里的 `chunks`；
  - 总长不等于 `length`；
  - 字节的 sha-256 和 `BlobRef.hash` 不符。
- 空 blob 是一个空分片，也能正确读出来。

**全文的 hook**（`state/blob.ts`）：`useBlobText(blob)`
- 状态分 `idle`、`loading`、`loaded`、`failed`，调用 `load()` 才下载，用 `TextDecoder` 解码。
- 用一个计数器保证只有最近一次下载的结果生效。组件卸载时计数器加一，还在路上的结果直接丢弃。这个写法在 StrictMode 下也没问题。

**请求面板**（`request/fulltext.tsx`、`RequestPanel.tsx`、`kinds.tsx`）
- T083 的 `Terminal` 换成 `ContentBox`，命令和详情两处共用。
  - 文本内容照旧直接显示。
  - blob 内容收起时显示预览，展开时下载全文；载入中显示"正在载入全文…"，失败时显示"全文没有载入"和"重试"。
  - 显示的文字都经过 `escapeInvisible`，作为 `<pre>` 里的文本节点。
  - 展开后高度上限是 `max-h-60`，框内滚动，和计划正文一致，收起时仍是 `max-h-24`。这是评审时补的一处。
- 权限请求的分支改成 `PermissionBody` 组件，因为 hook 不能写在 switch 的分支里。它记录每个 blob 当前是否"展开且已载入"，传给 `OptionsKind` 的值是 `truncated = content_truncated && !全部看过`。
- "已截断"的提示改成"已截断：展开看到全文后才能允许"，和设计 5.3.6 一致。

**用户气泡**（`rows/basic.tsx`）
- `text_blob` 块平时显示"长文本（N KB）"，有长文本块的消息总带"显示更多"按钮。
- 点"显示更多"才下载，下载后按纯文本显示，然后照常按 12 行折叠，在"显示更多"和"收起"之间切换。失败时显示"没有载入，点此重试"。

## 关键取舍

**放开条件**
- 允许类选项只在请求里每个 blob 内容（命令、详情）都展开、而且载入完成时才可选。不要求滚到底，和任务书一致。
- 收起任意一个、下载失败、哈希不符时，回到只能拒绝。再次展开后才重新放开；已经载入过的不会再下载一次。
- 标了 `content_truncated` 却没有 blob 内容时（不合协议），一直只能拒绝，按保守处理。
- 路径清单没有 blob 形态：按协议，超出预算的完整清单放进 `detail`（blob），所以已经被"详情"那一处覆盖。

**安全 #6 下为什么不会被绕过**
- **换请求就重新挂载**：`ComposerArea` 用 `key={request_id}` 渲染面板。新请求进来时 `request_id` 变了，面板整个重新挂载，所有"已看过"状态和展开状态都回到初始值，旧下载的结果因为计数器失效被丢弃。上一个请求里展开过的状态带不到下一个请求。
- **先核对哈希再显示**：显示的全文必须分片齐全、而且 sha-256 等于 `BlobRef.hash` 才算载入，否则只会是失败状态，选项不放开。宿主的请求摘要按哈希计入 blob，所以用户看到的就是摘要覆盖、批准将要生效的那份内容。agent 不能先给一份无害的文字、再换成另一份。
- **每次选择时都重新检查**：键盘和点击最后都走 `choose()`，它按当前的 `truncated` 检查 `isEnabled`。收起之后，Enter、数字键、点击都选不了允许类选项。
- **没有可插入的空隙**：放开依赖 `ContentBox` 的 effect 回报"已看过"。收起时，在一次提交和 effect 之间父组件还是旧值，但离散事件引起的 passive effect 会在下一个离散事件之前执行完，中间插不进用户输入。
- **没有可注入的地方**：全文只作为文本节点进 `<pre>`，先经 `escapeInvisible` 转义，不进样式，也不用 innerHTML 一类的 API。

**其他取舍**
- **`useBlobText` 放在 `state/`**：请求面板和用户气泡都要用，所以放在共用的 `state/`，而不是 `request/`。
- **气泡的下载入口用"显示更多"**：任务书的例子就是这么写的，所以不另加按钮。下载后直接展开，"收起"后按 12 行折叠。
- **"长文本"后面的大小改用全角括号**：本来用中点"·"，但截图夹具字体里没有这个字符，CI 的字体用例会失败。全角括号其他文案已经在用，所以换成括号，不重新生成字体子集。

## 验证方式

- **UNIT-blob-fulltext**，测试名以编号开头：
  - getBlob（`blob.test.ts`，3 条）：
    - 乱序到达的分片按 offset 拼对；
    - 空 blob 是一个空分片；
    - 片数不足时报错；
    - 字节和哈希不符时报错。
  - 请求面板（`request-panel.test.tsx`，2 条，jsdom，MockHost 连真的 `HostClient`，假时钟）：
    - 展开前：允许类选项不可选，显示预览，没有发出 `blob/get`。
    - 展开载入后：允许类选项可选；全文里的 `String.fromCodePoint(0x202e)` 显示为 `\u{202E}`；框带 `overflow-y-auto`，也就是不用滚到底；"已截断"提示消失。
    - 收起又回到不可选；再展开又可选，而且只下载过一次。
    - 哈希不符时：显示"全文没有载入"，选项不可选，错的内容不显示；点"重试"拿到正确内容后放开。
  - 用户气泡（`messages.test.tsx`，1 条）：点开前显示"长文本（30 KB）"、没有发出 `blob/get`；点"显示更多"后显示全文；"收起"后回到"显示更多"。
- **ui-core 单元测试** 76 条全部通过；ESLint（含 T136 的规则）、prettier、`tsc --build` 都通过。
- **截图基线没有变化**：现有用例里只有未截断的文本命令，`ContentBox` 渲染出来和原来的 `Terminal` 一样，所以没有更新基线，也没有截审阅图。新增的几种状态（载入中、失败重试、气泡里的大小）不在任何 UI 用例里，靠单元测试覆盖。
- **UI-csp-zero** 为 0（ui 作业里通过）。
- **F2**：没有查看本机的 Codex App 包。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T168-blob-fulltext/ci/`。第一次跑时 ts 作业的字体用例失败（缺"·"），改用全角括号后重跑，全部通过，摘要 `pr-ba45c499a1a2.json` 已提交。跑的期间没有往工作区写文件。

  | 作业 | 结果 |
  |---|---|
  | rust | 1,876 通过 |
  | protocol | 185 通过 |
  | ts | 465 通过 |
  | license | 通过 |
  | testdata-scan | 3 通过（本机没有敏感词表，那一步跳过） |
  | ui | 27 通过 |
  | e2e | 4 通过 |

- **试合并**：用 `git merge-tree` 试合并最新的 main，没有冲突。
- **规模**：新增 387 行，删除 67 行，净增 320（实现约 242，测试约 145；不含 notes、lock、截图基线）。开工前估计 310，停止线约 350，裁定接受。超出的主要是请求面板的搬动：`Terminal` 改成 `ContentBox`，权限分支拆成 `PermissionBody`，约 58 行。

## 提交列表

```
52e9f12 docs(T168): record the ci pr summary
ba45c49 fix(i18n): put the long text's size in full-width brackets
e69bf9a docs(T168): record the ruling and the content box cap
f2735f7 fix(ui-core): cap the expanded content box and scroll within it
e842d17 docs(T168): record the actual size
7ef1639 feat(ui-core): download a long text block of the user's bubble on demand
571470e feat(ui-core): show truncated request content in full before allowing
c2c9e46 feat(host-client): read blobs with getBlob
08d4cb0 docs(T168): record the checks and the size estimate
```

## 遗留问题 / 范围外发现

- **`text_blob` 没有预览字段**：协议里 `text_blob` 只有 `blob {hash, media_type, size}`，所以下载前只能显示大小，做不到任务书说的"预览和大小"。要显示预览，得在协议的 `text_blob` 里加 `preview`，由 T164 的发送端填。
- **请求结束后下载不会停**：协议没有取消 `blob/get` 的方法，请求结束后已经发出的下载仍会传完，界面只是丢弃结果。宿主那边是否需要取消，留给协议和宿主的任务决定。
- **新状态没有截图用例**：被截断内容的展开态、载入中、失败重试，以及气泡里的"长文本（N KB）"，都没有截图用例。需要视觉回归时，可以在 `request.spec.ts` 和消息流的用例里各加一张。
- **其他 blob 还没接入**：工具输出、diff 里的 blob 按任务书不做（T145 等）。`getBlob` 和 `useBlobText` 可以直接复用；大于几 MB 的输出可能还需要按范围读（`blob/get` 的 offset、length）。
