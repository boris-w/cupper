# T159 汇报

- 执行者：子 agent（前端，ui-core）
- 分支：`perf/T159-stream-render`，PR #119 https://github.com/boris-w/cupper/pull/119
- 完成日期：2026-11-16

## 做了什么

**第一步：剖析**（`docs/tasks/T159-stream-render/profile.md`，trace 来自 43798875882）

- **工具**：给 T103 的 spike 加了 trace 录制，只在设了 `SP6_TRACE=1` 时生效。
  - 通过窗口的 DevTools 协议会话录 JSON：`contentTracing` 写出的是 protobuf，第一次录制（43797367278）因此作废。
  - 每段起止用 `console.timeStamp` 标记，运行后由 `summarize.mjs` 汇总。
  - `SP6_UNMINIFIED=1` 时构建不压缩的渲染进程，采样里能看到函数名和所在模块。
- **场景 2、3 的原因**：每次流式更新都把正在增长的那条消息整条重新解析（remark / micromark）、整段重新排版。
  - 洪流的消息是一整段不换行的文字，整条就是一个块，开销随长度线性增长，主线程一直 100% 忙。
  - 场景 2 约 1.15M 字时，每次更新 634 ms（解析 260 ms，排版 237 ms）。
- **场景 7 的原因**：快照里每个 op 都用 zod 校验，而 `zEventBody` 是约 30 个分支的普通 union，条目类 op 要先在前面十几个分支里失败才能命中。
  - 切到不在 LRU 的会话，每次约 59 ms 花在 zod 上，占主线程时间的一半以上。

**第二步：优化**（你批准了 A、B）

- **B（9c34e444）**：生成器把 `zEvent` 和 `zEventBody` 的已知成员改成按 `type` 判别（`z.discriminatedUnion`），兜底仍是 `zUnknownEvent`。
  - 生成物由 `pnpm gen` 生成。
  - 测试：fixtures 的每个样本分别用新旧两种 union 解析，结果一致。样本覆盖合法的已知类型、不合法的已知类型、未知类型，以及信封不合法四类。
- **A（ce2468d8）**：只在流式时，给 streamdown 换上 `rows/blocks.ts` 的切块函数。
  - 超过 16 KB 的块切片：普通文字在空格后、单词前切，没有空格时硬切；代码块按行切，每片补上围栏。
  - 文字增长时只重新切已定片段之后的部分。
  - 条目完成后按默认方式整体渲染。
  - 测试覆盖：
    - 正常长度的消息逐字增长，每一步都和默认切法相同；
    - 切片规则；
    - 增长时只重切尾部；
    - 流式时长块分段，结束后与不切片时渲染出的 DOM 相同。

## 修前修后对照（runner，不开 trace，`bash run.sh 2 3 7`）

修前 [43802122047](https://github.com/boris-w/cupper/actions/runs/43802122047)（main 66c39edb），修后 [43802125555](https://github.com/boris-w/cupper/actions/runs/43802125555)（ce2468d8）。

| 场景 | 指标 | 修前 | 修后 | 预算 |
|---|---|---|---|---|
| 2 洪流 + 打字 | 帧时间 p50 / p95 / 最大 | 34 / 417 / 850 ms（n = 864） | 16.7 / 31 / 271 ms（n = 5,631） | p95 ≤ 16.7 ms |
| | 输入延迟 p50 / p95 / 最大 | 117 / 495 / 852 ms | 16 / 37 / 117 ms | p95 ≤ 50 ms |
| | 输出到屏幕 p50 / p95 / 最大 | 748 / 1,936 / 2,583 ms | 70 / 97 / 398 ms | p95 ≤ 100 ms |
| | 渲染进程内存峰值 | 644 MB | 561 MB | ≤ 600 MB |
| 3 四路流 | 帧时间 p50 / p95 / 最大 | 118 / 402 / 489 ms | 16.7 / 24 / 182 ms | p95 ≤ 16.7 ms |
| 3 六路流 | 帧时间 p50 / p95 / 最大 | 117 / 368 / 547 ms | 16.7 / 33 / 166 ms | — |
| 3 四路流 | 5 MB blob；重新同步 5,000 个事件 | 551 ms；710 ms | 224 ms；94 ms | — |
| 3 | 流重置 | 0 | 0 | 0 |
| 7 切换 | 不在 LRU，p50 / p95 / 最大 | 94 / 169 / 247 ms | 55 / 127 / 322 ms | p95 ≤ 150 ms |
| 7 切换 | 在 LRU，p50 / p95 / 最大 | 18 / 32 / 37 ms | 16 / 35 / 37 ms | ≤ 1 帧 |

- **场景 2**：输入延迟和输出延迟的 p95 进入预算。帧时间 p95 从 417 ms 降到 31 ms，离 16.7 ms 还差一倍；runner 比参考机器慢约一个数量级，这一项要在参考硬件上复核。
  - 修前每秒只出约 8 帧，修后接近 60 帧/秒：120 s 里帧数从 864 涨到 5,631。
- **场景 3**：帧时间 p95 降到 24–33 ms。blob 和重新同步也明显变快，因为主线程不再被占满。
- **场景 7**：不在 LRU 的切换 p95 进入预算。
  - 两次运行里最慢的两个样本都是前两次切换（修后 127、322 ms，修前 169、247 ms），属于首次走到这些代码的开销。
  - 去掉前两次后，修后最长 75 ms，修前 136 ms。30 个样本的 p95 取的就是这两个首次样本之一。

## 关键取舍

- **A 只在流式时切片**（你的条件），最终结果的复制按钮、段落间距和行内格式都不走样；代价是消息结束时多一次整体解析。
- **B 只改事件体的 union**。其他开放 union 的成员是交集形式，能否改成判别 union 要另试，没有动。
- **规模**：产品代码加测试 +250，正好压在停止线 250 上（硬线 320）：protocol +26，gen-protocol-ts +21，ui-core +203。spike 的诊断代码 +404，另列。

## 验证方式

- **本机**：protocol（173 个用例）、host-client（67 个）、ui-core（24 个文件、166 个用例）的 vitest 全过；tsc 全过；相关文件的 eslint 和 prettier 全过；`pnpm gen` 两次输出一致。
- **runner**：修前修后各一次，结果见上表。
- **PR #119**：在 ce2468d8 上 8/8 通过。截图基线没有变化，没有跑 ui-baseline。

## 提交列表

```
ce2468d8 perf(ui-core): parse and lay out a long streaming block in pieces
9c34e444 perf(protocol): pick an event's known member by its type
3bb677d4 docs(task): T159 profile of the stream's rendering and of switching
022bf210 fix(spike): SP-6 records its traces as JSON, through the window's DevTools session
14ce44aa test(spike): SP-6 scenarios 2, 3 and 7 record Chromium traces for T159
```

## 关于 C 的建议

C 是去掉 `#apply` 里 `isKnownEvent` 对每个 op 的第二次校验。**建议现在不做。**
- B 之后不在 LRU 的切换 p95 已经进入预算，稳态的切换最长 75 ms。
- 剩下的 p95 是由前两次首次切换决定的，C 解决不了。C 的收益估计每次只有 3–7 ms，却要改 host-client 的 sync 和 client 两处。
- 如果参考硬件上切换仍超预算，更值得先查首次切换（首次走到这些代码时的加载和编译）。

## 遗留问题 / 范围外发现

- **场景 2 的帧时间 p95（31 ms）和场景 3 六路流的帧时间 p95（33 ms）**还没进 16.7 ms 预算，要在参考硬件上复核；runner 上已无数量级问题。
- **virtual-core 的 `_clampedAdjustment`**：洪流期间一直不为空，每次提交都会重读一次 `scrollHeight`。是否会把列表滚到过时的位置没有观察到，已记在 profile.md 第 4 节。
- **trace 诊断留在 spike 里**（`SP6_TRACE`、`SP6_UNMINIFIED`），不设时场景照旧，T155 改写成每晚回归时可以沿用。
- **profile.md 只有第一步的剖析**；上面的对照表在这份汇报里，没有再提交到分支，免得 CI 重跑。
