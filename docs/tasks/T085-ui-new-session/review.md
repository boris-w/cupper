# T085 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**（修复 1 轮）

## 核对

- **规模**：366 行（实现 175、组件测试 133、截图用例 58），在约 380 的线内；开工前估了数。
- **CI**：执行者的摘要 `pr-f82c3dddb009.json` 通过（rust 1876、protocol 185、ts 457、license、testdata-scan、ui 33、e2e 5）。修复第 1 轮之后合并 main，`ci pr --record` 通过：`pr-3bf1c96616d9.json`（rust 1876、protocol 185、ts 462、license、testdata-scan、ui 35、e2e 4）。之后 main 进了 T163（只动宿主，与本分支不重叠），直接合并。
- **修复第 1 轮**（执行者自己指出，编排者要求合并前消掉）：`newsession/NewSessionPage.tsx` 与 `app/pages.tsx` 互相 import。`Slots` 与 `SlotsContext` 挪到新模块 `app/slots.ts`，`pages.tsx`、`NewSessionPage.tsx`、`Shell.tsx`、`App.tsx` 都从那里取，没有留转发导出；对外导出的名字不变。madge 检查 ui-core 没有循环；增 20、删 16，截图基线不变。
- **截图**：编排者看过审阅截图（有项目的亮色、agent 下拉的暗色），标题、项目选择器、动作条与设计 5.7.4 一致。6 张既有基线随之更新（默认页就是新建会话页），执行者逐张核过，变化只在主区。

## 逐项意见

1. **agent 的可用状态取自索引视图的 `AgentStatus`**，不是任务书写的 `agents/capabilities`：任务书写错了，后者只有缓存的能力描述，没有可用状态和原因码。索引视图有 `state`、`cli`、`auth.state`、`adapter.runtime`，还随 `agents.updated` 实时更新。推导写成导出的纯函数 `unavailable(status)`，原因按语义码走 i18n。
2. **agent 不可用时 M1a 仍允许发送**，发出后由宿主按语义码报错，走 `useSend` 已有的失败路径（草稿放回、toast）。按 AG-4"不支持的功能不出现"，是否直接禁止发送留给 M1b 加入 Codex 的任务一并定，记交接。
3. **接线**：`useSend` 新增 `create` 参数，`session/create` 不再写死 agent 和工作区；会话页的输入框不传，行为不变。`create` 用 `useMemo` 保持引用稳定。
4. **外部字符串**：项目名经 `escapeInvisible`，不进样式。
5. **`Menu` 的 `align`**：默认仍是 end，旧调用方不受影响。

## 记下的点

- **文案只加了 zh-CN**，与现有做法一致。
- **组件测试的模拟宿主只对 `index` 流回放索引 ops**：否则新建的会话流也收到这些 ops，视图报错后反复重订阅、耗尽内存。以后写类似测试的任务照这个做法。
