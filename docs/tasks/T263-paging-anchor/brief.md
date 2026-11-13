# T263 向前翻页时锚点位移

- 派发日期：2026-11-13
- 执行者：子 agent（前端，ui-core；T257 的执行者）
- 分支：`fix/T263-paging-anchor`
- 依赖：T103（已合并）。T179 依赖本任务

## 背景

设计 5.7.4 的长会话有两条滚动规则，其一是"向前插入历史后视口不跳动（以视口顶部的行为锚点）"；5.9.6 的预算是锚点元素位移 ≤ 1 px。T103 的场景 4 在 runner 上翻 5,000 条的会话 60 页，38 页位移为 0，**22 页位移 26 px**（`docs/tasks/T103-sp6-render/report-sp6.md` 第 2、4 节）。UNIT-stream-anchor 是绿的，没有覆盖到这种情况。

T103 的推测（未验证）：`packages/ui-core/src/stream/StreamList.tsx` 的 `useLayoutEffect` 与 `ScrollKeeper.restore` 在历史插入后只复原一次锚点，用的是 `virtualizer.measurementsCache[index].start`，新插入的行此时还按 `estimateSize: () => 44` 估算；实测后锚点的位置又变了，没有人再校正。每次都正好 26 px，像是某一行估算值与实测值之差。

## 范围

**做**（计划 T263 一行）：
1. 查清原因（加临时日志或单元测试复现均可，临时日志不进提交）。
2. 修：新插入的行实测完之后，锚点仍在原来的位置（≤ 1 px）；不破坏 T245 的"只有用户能让列表离开底部"和 T256 的"发送后回到底部"。
3. UNIT-stream-anchor 补"估算高度与实测高度不同"的用例，修前应当失败。
4. runner 上用 T103 的场景 4 复测（`spikes/T103-sp6-render/long.spec.ts`，经 `spike-run`，命令 `bash run.sh 4`）：60 页里没有位移超过 1 px 的页。

**不做**：长会话的性能优化（T159）；滚动条、贴底规则的其他改动。

## 验收标准

- [ ] UNIT-stream-anchor 的新用例修前失败、修后通过，原有用例不变。
- [ ] runner 上场景 4 复测，汇报里有每页的位移数据。
- [ ] 截图基线如有变化只由 ui-baseline 生成，每张写明原因。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接。

## 规模

- 停止线：150；硬停止线：200。开工前在 `notes.md` 里估行数报给 `main`；原因查清后再报一次。

## 参考资料

- 设计：5.7.4（长会话、两条滚动规则）、5.9.6（向前翻页时的视口稳定）。
- 卷宗：T103（场景 4、报告）、T245（滚轮意图）、T256（`ScrollKeeper.follow()`）、T146（回合折叠）。

## 注意

- 测量在 runner 上跑（守则第 10 条）；本机只跑单元测试和单份短冒烟。
- 渲染进程的约束照旧。
- 要推送或触发 `spike-run` 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T043`，编排者已切到新分支 `fix/T263-paging-anchor`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
