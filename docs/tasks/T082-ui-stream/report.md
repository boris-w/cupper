# T082 汇报

- 执行者：子 agent（工程师；T077、T134、T136、T135、T029、T164 的执行者接手）
- 分支：`feat/T082-ui-stream`
- 完成日期：2026-10-20

## 做了什么

收窄后的 T082 已完成：会话视图拍平成行，行列表按行虚拟化，支持加载更早的历史、插入历史后视口不跳、贴底和 ↓ 按钮，并接进了会话页。CI 全部通过，摘要已提交。规模 888 行，按裁定接受，超出硬停止线的经过见下文"规模"。

**1. 行的形状与共用注册表**（`packages/ui-core/src/rows/stream.ts`，最先提交，供 T166 参考）
- `StreamRow` 的每种行都带着要显示的数据，取自视图最近一次刷新时的状态，所以行组件只看 `row`，不必自己订阅视图。
- 共用实例 `streamRows`；组件用 `registerRow(kind, Component)` 登记。

**2. 拍平成行**（`rows/flatten.ts`）
- 顺序：回合标题，接着该回合的条目，再是尾部，最后是视图 `unknown` 列表里的事件。
- 同一 `group_id` 连续两个及以上的工具调用合成一个活动组行，只有一个时是普通的工具行。
- 请求条目成为 `request` 行，留给 T165；本版本不认识的条目种类成为 `unknown` 行。
- 还有更早的历史时，顶部放一个 `history` 行。

**3. host-client 的 `historyPage(sessionId, sink, cursor?)`**：请求 `history/page`，把结果里的事件逐个交给视图，返回下一页的游标。

**4. 行列表**（`stream/StreamList.tsx`、`scroll.ts`、`actions.ts`）
- 用 TanStack Virtual 3.14.13 按行虚拟化，行高动态测量。新依赖 MIT 许可，锁精确版本。
- **加载更早的历史**：视图的第一个回合编号大于 1 时显示。游标取那个回合；一页切开了一个回合时，沿用上一页返回的 `next`。
- **插入历史后视口不跳**：加载前记下视口顶部那一行和它离顶部的距离。等这一行被前面新插入的行推后了，再把视口放回原处；这时新插入的行还只有估计的高度，之后真实测量造成的高度变化由 TanStack 自动补偿。
- **贴底**：离底部 24 px 以内算贴底，此时跟着新内容滚动。用户向上滚动后停止贴底，显示 ↓ 按钮，点一下回到底部。

**5. 本任务负责的几种行**（`rows/basic.tsx`）
- 加载更早的历史：更多、加载中、失败重试三种状态。
- 回合标题：
  - 运行中和正常结束都写"已处理 {时长}"，运行中每秒更新；
  - 用户中断写"你在 {时长} 后停止了"，宿主中断写"已中断"，失败写"失败"；
  - 不折叠（回合折叠归 T143、T146）。
- 用户气泡：
  - 最宽 70%，超过 12 行折叠，用"显示更多/收起"切换；
  - 悬停时出现"复制"；
  - 带审查评论时显示"{n} 条评论"；
  - 长文本块和图片块只标注类型，不显示内容。
- 未知内容：用 T128 的占位。

**6. 接进会话页**：消息流放在横幅和输入框之间。文案加在 `stream` 命名空间。

## 给 T166 的接入点

- **行的形状**：以 `rows/stream.ts` 的代码为准，`notes.md` 里有对照表。每种行都有 `kind` 和 `key`，下面是 T166 要登记的六种：

  | `kind` | 数据 |
  |---|---|
  | `agent_message` | `itemId`、`text`、`streaming` |
  | `thought` | `itemId`、`text`、`running`、`durationMs?` |
  | `tool` | `ToolData`：`itemId`、`call: ToolCall`、`output`（已流出的输出）、`running`、`end?: ItemEnd` |
  | `activity_group` | `groupId`、`tools: ToolData[]`、`running`；展开状态由行组件自己保存 |
  | `notice` | `itemId`、`notice: Notice` |
  | `error` | `itemId`、`error: ErrorItem` |

  其余种类：`plan`、`late_tool_output` 留给 T146，`request` 留给 T165。

- **登记**：
  - 组件放在 `rows/` 下，在模块里调用 `registerRow("agent_message", AgentMessageRow)`；
  - 组件的属性类型是 `{ row: RowOf<"agent_message"> }`，每种 `kind` 只能登记一次；
  - 在 `rows/index.ts` 里加一行 `import "./<模块>";`，行列表经这个文件一次性载入全部登记；
  - 没有登记的 `kind` 显示占位。
- **消息流在会话页里的位置**：`app/pages.tsx` 的 `SessionPage` 里，`SessionBanners` 之下、输入框之上，用 `<StreamList sessionId={sessionId} />`。
  - 每行外面套了一层，加上行间距 `pt-(--layout-item-gap)`，行组件本身不必再加上间距；
  - 行列表把 `useStreamActions()`（目前只有 `loadEarlier`）提供给各行。
- **样式约束**：T136 的规则适用，不写 CSS 自定义属性。agent 的字符串只作为文本节点，或经清洗的 Markdown 渲染。

## 关键取舍

- **行带数据**：行组件是 `row` 的纯函数，T166 不必订阅视图，可以拿造出来的行直接测试和截图。代价是每帧至多重新拍平一次，长会话里是一次线性扫描。
- **是否还有更早的历史，看第一个回合的编号**：回合编号从 1 开始，所以编号是 1 就到头了。视图重建后，按新的第一个回合重新算游标，不会沿用旧游标而跳过回合。
- **锚点**：只在被记下的那一行确实被推后时才恢复，所以加载中的那次重绘不会把锚点提前用掉。

## 规模

新增 888 行、删除 3 行，超过 850 的硬停止线（按裁定接受）。

| 部分 | 新增 | 草估 |
|---|---|---|
| 实现 | 630 | 约 555 |
| 测试 | 258 | 约 170 |

- 实现部分多在行的形状上：为了让 T166 并行开发，各种行的数据字段都要定下来。
- 测试超出较多：拍平测试要造一个完整的视图夹具；jsdom 里要给虚拟列表打桩；格式化工具把期望数组展开成每项一行。
- 我的失误：实现写完时统计为 630，按测试的草估推算约 775，就继续写了测试，没有给测试留够余量。下次给测试多留余量。

## 验证方式

- **UNIT-stream-rows**（`rows/flatten.test.ts`）：
  - 行的顺序和种类：回合、条目、活动组、单个工具、请求、未知条目、尾部、`unknown` 列表；
  - 每种行带的数据：回合的状态和用时、消息、是否流式、活动组里的工具、提示等。
- **UNIT-stream-anchor**（`stream/stream.test.tsx`）：
  - `ScrollKeeper`：还没有行插进来时继续保持锚点；插入后算出的位置正确；只恢复一次；贴底的 24 px 阈值。
  - 组件：点"加载更早的历史"后，用第一个回合的游标请求 `history/page`，第一个回合出现，按钮随之消失。
  - 组件：向上滚动后出现 ↓，点击后消失。
- **`cargo xtask ci pr --record docs/tasks/T082-ui-stream/ci/` 通过**，摘要 `pr-a04fafcffb44.json` 已提交：

  | 作业 | 结果 |
  |---|---|
  | rust | 1854 通过 |
  | protocol | 185 通过 |
  | ts | 442 通过 |
  | license | 1 通过 |
  | testdata-scan | 3 通过 |
  | ui（截图回归，含 UI-csp-zero） | 16 通过 |

  ESLint（含 T136 的规则）和字体子集检查也通过。

## 提交列表

```
12a1190 docs(task): record the T082 pr CI summary
a04fafc docs(task): record the T082 line counts and stop at the hard limit
e87b6a0 feat(ui-core): the message stream's row list
4d98720 feat(ui-core): the stream's row shapes and their registry
70972f5 docs(task): estimate T082 and propose a split before starting
```

## 遗留问题 / 范围外发现

1. **插入历史后视口不跳，还没在真实环境里确认**：jsdom 没有布局，所以锚点的计算只在 `ScrollKeeper` 的单元测试里验证过，组件测试只验证了插入和请求。留给 T090 之后的 e2e 确认。
2. **截图基线没有覆盖消息流**：现有截图夹具里的会话没有条目，所以基线没变。消息流的截图（UI-stream-markdown、UI-stream-tools）归 T166。
3. **长文本块在气泡里只显示"长文本"**：host-client 还没有 `getBlob`，读不出内容。
4. 分支基于 de98370，没有同步 main。