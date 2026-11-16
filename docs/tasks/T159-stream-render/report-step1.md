# T159 第一步汇报（剖析）

- 执行者：子 agent（前端，ui-core）
- 分支：`perf/T159-stream-render`
- 日期：2026-11-16
- 状态：第一步完成，等你批准方案后再进第二步。

## 做了什么

**1. 给 T103 的 spike 加了录 trace 的功能**（诊断代码，不计停止线；只有设了环境变量才生效，不设时场景与 T103 相同）
- `trace.ts`：通过窗口的 DevTools 协议会话录 trace，输出 JSON，各段起止用 `console.timeStamp("sp6:<label>:start|end")` 标出。
  - 第一次用的是主进程的 `contentTracing`（run 43797367278），但 Electron 45 写出的是 Perfetto 的 protobuf，汇总脚本读不了，后来改成现在的做法。
  - `performance.mark` 在 trace 里不出现，所以改用 `console.timeStamp`。
- 场景 2 录 4 段：打字后第 10、35、60、85 s 各录 4 s，并记下消息长度。
- 场景 3 只录 4 路，两段：只有流、含重新同步。
- 场景 7 录不在 LRU 和在 LRU 的切换，各 8 次。
- `SP6_UNMINIFIED=1`：照 `build:e2e` 构建，只是渲染进程加 `--minify false`，采样里能看到函数名，再按产物里的 `//#region` 注释对回模块。
- `summarize.mjs`：用例跑完后在 runner 上汇总，统计主线程按事件的自身时间，采样按函数和模块的自身时间、包含时间，然后把 trace 压成 gz 上传。

**2. 写了 `docs/tasks/T159-stream-render/profile.md`**：结论、证据表、方案，见下文。

## 结论（runner run 43798875882）

1. **场景 2、3**：每次流式更新，正在增长的那条消息都被整条重新解析（streamdown → remark-parse / micromark）、整段重新排版。洪流是一整段不换行的文字，整条就是一个块，开销随长度线性增长，主线程从第一段起就 100% 忙。

   | 消息长度 | 每次更新合计 | Markdown 解析 | 排版 |
   |---|---|---|---|
   | 约 170K 字 | 117 ms | 42 ms | 42 ms |
   | 约 500K 字 | 353 ms | 136 ms | 121 ms |
   | 约 830K 字 | 343 ms | 185 ms | 62 ms |
   | 约 1.15M 字 | 634 ms | 260 ms | 237 ms |

   - 解析约 0.25 ms/KB，四段都在这个值附近。
   - 场景 3 机制相同：屏幕上那一路的单次排版在 3.5 s 里从 32 ms 涨到 145 ms。
2. **场景 7**：不在 LRU 的切换，8 次合计主线程忙 806 ms，其中 zod 自身占 471 ms，约每次 59 ms。
   - 原因：`zEventBody` 是约 30 个分支的普通 union，条目类 op 要先在前面多个分支里失败才能命中；`#apply` 里的 `isKnownEvent` 又校验一遍。
   - 其余开销小：React 约 21 ms，streamdown 约 5 ms，视图应用约 4 ms，排版约 2 ms。
3. **不是瓶颈的**：
   - 高亮没有出现在 trace 里。
   - virtual-core 的大头是 `getMaxScrollOffset` 读 `scrollHeight` 触发的强制布局。这些排版本来就要在帧里做，只是提前做了。

## 方案（等你批准）

- **A. 大块拆开渲染**（场景 2、3）
  - 做法：给 streamdown 传 `parseMarkdownIntoBlocksFn`。正文只增长时，只从上次最后一个块起重新切；超过 16 KB 的块在空白处切片，代码块按行切并补上围栏。这样每次只解析、排版最后一片。
  - 代价：超大段落每 16 KB 多一个段落间距；超大代码块显示成几个相连的代码框。正常消息不受影响。
  - 预计：85 s 那段每次更新从约 634 ms 降到几十 ms。
  - 规模：约 140–190 行。
- **B. 事件体按类型校验**（场景 7，顺带翻页和非 delta 的实时事件）
  - 做法：在 gen-protocol-ts 的 postprocess 里，把 `zEventBody` 生成为 `z.union([z.discriminatedUnion("type", 已知分支), zUnknownEvent])`，语义不变。
  - 证据：本机 jitless 微基准，1,000 个 op，28.8 → 3.5 ms，输出逐个相同。
  - 预计：切换 p95 约 110 ms，进入 150 ms 的预算。
  - 规模：约 25–40 行，生成物不计。
- **C（可选）. 去掉 `#apply` 的重复校验**
  - 规模约 15–25 行，等 B 复测后再定。
- **建议**：A、B 都做，各一个提交；停止线 250。

## 验证方式

- spike 文件：本机用临时 tsconfig 跑 tsc，通过；prettier 通过（spikes 不在 eslint 范围内）。
- 汇总脚本和 CDP 录制：在本机 headless Chromium 上冒烟验证过（临时脚本，已删）。
- runner：43798875882 成功，产物 `spike-results` 里有 8 个 trace（gz）和对应的 summary。

## 提交列表

```
3bb677d4 docs(task): T159 profile of the stream's rendering and of switching
022bf210 fix(spike): SP-6 records its traces as JSON, through the window's DevTools session
14ce44aa test(spike): SP-6 scenarios 2, 3 and 7 record Chromium traces for T159
```

- 14ce44aa、022bf210 已由你推送；3bb677d4 只在本机，需要推送。

## 遗留问题 / 范围外发现

- virtual-core 的 `_clampedAdjustment` 在洪流期间一直不为空，所以每次提交都会重读 `scrollHeight`。是否会把列表滚到过时的位置没有观察到，记在 profile.md 第 4 节。
- 开着 trace 的场景数值偏慢，只用来定位，不和 T103 比。顺带记一下，场景 7 不在 LRU 的切换这次是 p50 / p95 91 / 165 ms，T103 是 206 / 407 ms，原因没有核实。
- **第二步**：等你批准后，用 SendMessage 叫我继续。
