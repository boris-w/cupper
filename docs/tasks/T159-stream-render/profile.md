# T159 第一步：流式渲染与切换首屏的剖析

- 日期：2026-11-16
- 数据：spike-run [43798875882](https://github.com/boris-w/cupper/actions/runs/43798875882)，提交 022bf210，命令 `SP6_TRACE=1 SP6_UNMINIFIED=1 bash run.sh 2 3 7`。
  - 产物 `spike-results` 里每段有三个文件：`*.trace.json.gz`（Chrome DevTools 的 Performance 面板或 Perfetto 可以直接打开）、`*.summary.txt` 和 `*.summary.json`（`summarize.mjs` 的汇总）。
  - 下文的表都出自这些汇总，有一处另外按 trace 数了次数，见附注。
- 第一次录制（43797367278）白录了：`contentTracing` 写出的是 Perfetto 的 protobuf，汇总脚本读不了。改为通过窗口的 DevTools 协议会话录 JSON，见 notes.md。
- **口径**：
  - runner 和 T103 相同（macOS 15，M1 虚拟机，3 核）。渲染进程用的是不压缩的构建，好让采样里有函数名和模块。
  - 录 trace 会拖慢 App，所以这次运行的场景数值不能和 T103 直接比，只用来定位时间花在哪。
  - "自身时间"指函数本身的时间；"包含时间"还算上它调用的函数。

## 1. 结论

1. **场景 2、3 的时间几乎全花在"正在增长的那条消息被整条重做"上。**
   - 每次流式更新，streamdown 都把整条消息的最后一个 Markdown 块重新解析一遍（remark-parse / micromark）。浏览器也把整段文字重新排版一遍（Layout）。
   - 洪流的消息是一整段不换行的字母，整条消息就是一个块，所以两项开销都随消息长度线性增长。
   - 主线程从第一段起就 100% 忙，消息越长，每秒能做的更新越少，每次更新卡得越久。
2. **每帧的 JS 时间确实随消息长度增长**。Markdown 解析每 KB 约 0.25 ms，四段都在这个值附近（表 2.1）。
3. **场景 7，切换到不在 LRU 的会话时，首屏前一半以上的主线程时间花在 zod 校验快照里的每个 op 上**。
   - `zEventBody` 是约 30 个分支的普通 union，加一个兜底分支。条目类的 op 要先在前面十几个分支里失败（每个失败分支还会先把 `data` 校验一遍、生成错误信息），才能命中自己那一支。
   - 接着 `#apply` 里的 `isKnownEvent` 又按类型把 `data` 校验一遍。
4. **高亮（Shiki）和虚拟列表的测量都不是瓶颈**。
   - trace 里没有 Shiki：洪流没有代码块。
   - virtual-core 自身时间里最多的是 `getMaxScrollOffset`，它读 `scrollHeight` 触发了强制布局，记在它头上的其实是排版时间（见 2.3）。

## 2. 证据

### 2.1 场景 2：每次更新的开销随消息长度增长

从开始打字算起，在第 10、35、60、85 s 各录 4 s。"更新次数"是这一段里超过 5 ms 的 Layout 事件的个数，每次更新排版一次，见附注。

| 段 | 消息长度（起–止） | 主线程忙 | 更新次数 | 每次更新：Markdown 解析 | 每次更新：排版 | 每次更新合计 |
|---|---|---|---|---|---|---|
| 10 s | 138K–198K 字 | 4.08 / 4.09 s | 35 | 42 ms | 42 ms | 117 ms |
| 35 s | 476K–533K | 4.95 / 4.95 s | 14 | 136 ms | 121 ms | 353 ms |
| 60 s | 798K–869K | 4.46 / 4.46 s | 13 | 185 ms | 62 ms | 343 ms |
| 85 s | 1,125K–1,186K | 4.44 / 4.44 s | 7 | 260 ms | 237 ms | 634 ms |

- **Markdown 解析**：取 remark-parse 的包含时间，整段约 1.5–2.4 s，即这一段 JS 时间的 35–54%。按消息长度算，每 KB 分别是 0.25、0.27、0.22、0.23 ms，是线性的。
  - 自身时间最多的函数：micromark 的 `consume` / `main` / `write` / `go` / `atBreak`，gfm 自动链接扩展的 `previousEmail` / `atext`，micromark-util-character 的 `check`。
- **排版**：60 s 那段偏低，原因没查清，但总趋势是随长度增长；85 s 那段单次排版超过 200 ms，最长 358 ms。
- **其他随长度增长的开销**：
  - streamdown 每次用 marked 的 lexer 把全文切一遍块，每段约 100–165 ms；
  - remend 每段约 65–93 ms；
  - 绘制每段约 225–322 ms。
- 这次运行（开着 trace）的场景数值：帧时间 p95 450 ms，输入延迟 p95 472 ms，输出延迟 p95 2.33 s。T103 未开 trace 时分别是 281 ms、326 ms、1.25 s。

### 2.2 场景 3：4 路流，屏幕上那一路同理

| 段 | 主线程忙 | 排版 | remark-parse（包含） | 单次排版 |
|---|---|---|---|---|
| 只有流（1–4.5 s） | 3.0 / 3.0 s | 1.55 s | 0.92 s | 32 → 145 ms，逐次增长 |
| 含重新同步（10–14 s） | 3.9 / 3.9 s | 1.10 s | 1.88 s | 35–271 ms |

- 窗口里只有屏幕上的那个会话在渲染。4 路负载落在渲染进程上的，是屏幕上那条消息的增长，机制和场景 2 相同。
- 重新同步 5,000 个事件本身只占很小一部分：这段 zod 自身时间 39 ms。

### 2.3 强制布局记在谁头上

- 场景 2 的 JS 自身时间里，有两项其实是排版：
  - 10 s 那段，StreamList 的 `useLayoutEffect` 读 `scrollTop` / `scrollHeight`，930 ms；
  - 85 s 那段，virtual-core 的 `getMaxScrollOffset`，1,442 ms。
- 调用链（85 s 那段）：`commitLayoutEffectOnFiber` → `Virtualizer._willUpdate` → `_retryClampedAdjustment` → `getMaxScrollOffset`，在每次提交里读 `scrollHeight`。
- 这些读取只是让本来就要做的排版提前、同步地做完，不是额外的排版：每次更新只有一次大的 Layout。
- 消息不再整段重排之后，这部分会跟着变小。
- 另外记一点：`_retryClampedAdjustment` 只有在 `_clampedAdjustment` 不为空时才读。它在洪流期间一直不为空，说明有一次被浏览器截断的补偿写入始终没有消掉。见第 4 节。

### 2.4 场景 7：切换首屏

不在 LRU 的切换录了 8 次（第 9–16 次），每次从链接发出算到新行出现的那一帧。这 8 次合计 957 ms，主线程忙 806 ms：

| 包含时间 | 8 次合计 | 约每次 |
|---|---|---|
| host-client 处理快照（`sync.ts` 的 `snapshot`） | 506 ms | 63 ms |
| 其中 zod（自身） | 471 ms | 59 ms |
| React 渲染与提交（react-dom） | 171 ms | 21 ms |
| streamdown（各条消息首次解析） | 37 ms | 5 ms |
| virtual-core / react-virtual | 34 / 30 ms | 4 ms |
| 会话视图应用 op（session-view.ts） | 31 ms | 4 ms |
| flatten | 16 ms | 2 ms |
| 排版 | 14 ms | 2 ms |

- zod 自身时间最多的是三个 `inst._zod.parse`（162、148、38 ms）和 `finalizeIssue`（34 ms，即失败分支生成错误信息）。
- 在 LRU 的切换：8 次合计 269 ms，主线程忙 148 ms，没有明显热点。
- 这次的切换数值（开着 trace）：
  - 不在 LRU：p50 / p95 为 91 / 165 ms，预算 p95 ≤ 150 ms。T103 是 206 / 407 ms；这次明显好一些，可能是 runner 的差异，或 T262 之后启动时少了竞争，没有核实。
  - 在 LRU：17 / 33 ms。

**本机微基准**：用 `protocol/fixtures/events.json` 里的条目类和回合类事件体，jitless（渲染进程的 CSP 要求如此），1,000 个 op：

| 写法 | 每 1,000 个 op |
|---|---|
| 现在的 `zEventBody.safeParse` | 28.8 ms |
| 再加 `isKnownEvent` | 32.6 ms |
| `z.union([z.discriminatedUnion("type", 已知分支), zUnknownEvent])` | 3.5 ms |
| 再加 `isKnownEvent` | 6.9 ms |

- 两种写法对这 1,000 个 op 的输出逐个相同。

## 3. 优化方案

### 方案 A：大块拆开渲染（场景 2、3）

- **做法**：给 streamdown 传 `parseMarkdownIntoBlocksFn`（2.7.0 的公开属性，`parseMarkdownIntoBlocks` 也是导出的），在 `rows/markdown.tsx` 里换成我们的切块函数：
  1. 正文只会变长时，只从上次最后一个块的开头起重新切，前面的块原样沿用，省掉每次对全文跑一遍 marked lexer。
  2. 超过 16 KB 的块，在空白或换行处切成几片（找不到空白就硬切）。
     - streamdown 本身按块缓存，所以每次只重新解析最后一片。
     - 每片是一个独立的块元素，浏览器只重排最后一片。
     - 代码块按行切，每片补上同样的围栏。
- **代价**：
  - 超过 16 KB 的大段文字，每 16 KB 会多出一个段落间距。
  - 跨片的行内格式（如 `**…**` 跨在切点两侧）各自闭合。
  - 超过 16 KB 的代码块会显示成几个相连的代码框，跨片的多行字符串或注释可能高亮不准。
  - 正常的消息有空行分段，块都远小于 16 KB，不受影响。
- **预计收益**（按 2.1 推算）：
  - 每次更新的解析量从整条消息降到最多一片：85 s 那段从约 260 ms 降到约 4 ms；排版降到一片的量级；marked 不再扫全文。
  - remend 仍会扫全文，但它在 85 s 那段每次更新只有约 11 ms。
  - 每次更新从 117–634 ms 降到几十 ms，主线程不再被占满。帧时间、输入延迟、输出延迟的 p95 预计都会降一个数量级以上。
  - runner 本身比作者机器慢一个数量级，不一定能进入 16.7 / 50 / 100 ms 的预算。
  - 会话里已有的超大消息再次打开时，也只解析、排版一次，且是分片的。
- **范围**：ui-core 的 `rows/markdown.tsx`，加一个切块模块；单元测试覆盖切块规则（增长时的复用、16 KB 切片、空白处切、代码块按行切补围栏、普通消息不变）。截图基线不应变化：现有截图里没有超过 16 KB 的块。
- **规模**：产品代码约 70–100 行，测试约 70–90 行，合计约 140–190 行。

### 方案 B：事件体按类型校验（场景 7，顺带翻页和实时事件）

- **做法**：在 `tools/gen-protocol-ts/src/postprocess.ts` 里，把 `zEventBody` 生成为 `z.union([z.discriminatedUnion("type", [...已知分支]), zUnknownEvent])`。
  - 语义不变：已知类型且数据合法，命中对应分支；已知类型但数据不合法，以及未知类型，都落到 `zUnknownEvent`。
  - `zEvent` 是信封与 `zEventBody` 的交集，一起受益。
  - 生成物重新生成，不计行数；`fixtures.test.ts` 覆盖 known / unknown / invalid 三种情况。
- **预计收益**：
  - 快照、翻页的每个 op 和非 delta 的实时事件，zod 部分快约 8 倍。
  - 场景 7 每次切换的 zod 时间从约 59 ms 降到约 7–15 ms。不在 LRU 的切换，p50 预计约 50 ms，p95 约 110 ms，进入预算。
- **范围**：生成器加它的测试。生成器的规则对其他开放 union 是通用的，但这次只改事件体：其他开放 union 的成员是 `allOf` 生成的交集，zod 的 discriminatedUnion 是否支持要另试，留作后续。
- **规模**：约 25–40 行。

### 方案 C（可选）：去掉 `#apply` 里的重复校验

- **做法**：方案 B 之后，`isKnownEvent` 再按类型校验一次数据，占剩下 zod 时间的一半（3.5 → 6.9 ms）。可以改为由第一次校验命中哪一支来判断是不是已知事件，不再校验第二遍。
- **收益**：每次切换再省约 3–7 ms。
- **规模**：约 15–25 行，涉及 host-client 的 `sync.ts` 和 `client.ts`。

**建议**：A 和 B 都做；C 视 B 之后的复测再定。A、B 互不依赖，可以各一个提交。停止线建议 A + B 合计 250 行。

## 4. 不在这次范围、但值得记下的

- **virtual-core 的 `_clampedAdjustment` 在洪流期间一直不为空**，于是每次提交都会重试那次补偿写入，并读一次 `scrollHeight`。
  - 补偿写入在内容还没长高时被浏览器截断，此后它的回读一直没对上，也就没被清掉。
  - 是否会在某次内容长高后把列表滚到一个过时的位置，没有观察到，需要另查。
  - 方案 A 之后这次读取的代价很小。
- **测量代码本身的开销**：场景 2 每帧用正则扫一遍消息全文（`tick`），每段约 85–146 ms，约占 2–3%，影响不了结论。

## 附注：怎么数的

- "更新次数"：这一段主文档渲染进程主线程上超过 5 ms 的 Layout 事件的个数。流式期间每次更新只有一次大的排版，其余 Layout 事件都在 1 ms 以下。
- "每次更新合计"：主线程忙的时间除以更新次数。
- 按调用者归属（2.3）：用 trace 里 V8 采样的调用栈统计，脚本没有提交。
