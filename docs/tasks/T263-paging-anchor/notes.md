# T263 笔记

## 开工前的估算（2026-11-13）

读过 T103 报告第 2、4 节、`long.spec.ts`、`StreamList.tsx` 的复原逻辑和 `@tanstack/virtual-core` 3.17.11 的测量代码之后的初步判断（待复现确认）：

- `locate` 读的 `virtualizer.measurementsCache` 是渲染时算出的一份惰性视图，背后是一个 Float64Array；提交阶段 `measureElement` 同步量出的新行写进 `itemSizeCache`，但这份视图要到下一次 `getMeasurements()` 才重算。复原时新行可能已经量过，算锚点位置用的却还是 44。
- 这些新行是在列表还在顶部（虚拟器的 scrollOffset 约为 0）时量的，不在视口上方，虚拟器不会替它们补偿。
- 虚拟器只在最近 150 ms 内没有滚动事件时同步测量（`isScrolling`），否则交给 ResizeObserver，那时列表已经复原、新行在视口上方，虚拟器会补偿。这能解释为什么只有一部分页位移、位移又总是同一个值（种子数据每页的行一样）。

估算（`cargo xtask size` 口径，不含 docs）：

| 部分 | 行数 |
|---|---|
| `StreamList.tsx` / `scroll.ts`：复原时用最新的测量 | 10–30 |
| `stream.test.tsx`：行高可按行设置；UNIT-stream-anchor 新用例（估算与实测不同、同步测量的路径） | 40–70 |
| 合计 | 50–100（停止线 150） |

如果复现下来不是这个原因（例如还要在复原后继续校正锚点），再报一次。

## 原因（已复现，2026-11-13）

开工前的判断成立。

- **单元测试**：UNIT-stream-anchor 新用例。第一回合的消息高 70 px，其他行 40 px。用户滚到顶部 300 ms 后（虚拟器的 `isScrolling` 已经复位）才载入历史。
  - 修前：锚点行下移 22 px，即 (70 − 44) + (40 − 44)。
  - 临时让 `locate` 先刷新测量后：0。
- **本机 headless Chromium 短冒烟**（截图工具的页面加 mock 宿主，翻 12 页；临时脚本，不提交）：
  - 滚到顶部后立即点：12 页全为 0。
  - 滚到顶部 400 ms 后才点：12 页全部位移，127–194 px（假数据的行高与 44 差得多）。
  - 临时修法下两种都是 12 页全为 0。
- **机制**：
  1. 历史进来的那次渲染，列表还在顶部，虚拟范围是新行。
  2. 提交时 `measureElement` 同步量了这些新行；虚拟器只在最近 150 ms 内没有滚动事件时才同步量。
  3. 新行不在视口上方，虚拟器不补偿。
  4. 随后 `useLayoutEffect` 里的 `locate` 读 `virtualizer.measurementsCache`。这是渲染时 `getMeasurements()` 算出的惰性视图，还没有这些测量，锚点按 44 估算。
  5. 下一次渲染用上实测值，锚点行就移了 Σ(实测 − 44)。
  6. 150 ms 内到达的页改由 ResizeObserver 测量。那时列表已经复原，新行在视口上方，虚拟器会补偿，所以是 0。
- **runner 上的数据**：38 页为 0，是页在滚动后 150 ms 内到达；22 页都是 26 px，是种子数据每页的新行一样，Σ(实测 − 44) 恒为 26。

## 修法

- `locate` 改用 `virtualizer.takeSnapshot()`：virtual-core 3.17.11 类型定义里的公开方法，按当前的测量返回量过的行。锚点行在屏幕上出现过，一定量过。代码注释里写了为什么不能读 `measurementsCache`。
- 编排者 2026-11-13 同意。备选是先调 `getTotalSize()` 刷新再读 `measurementsCache`，但那依赖库内部的写法，没有采用。
- 修后单元测试通过，ui-core 的单元测试全部通过。本机 headless 短冒烟两种时机都是 12 页全为 0。临时脚本已删，没有提交。
