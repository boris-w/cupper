# T220 汇报

- 执行者：子 agent（e2e 与桌面工程师）
- 分支：`fix/T220-bridge-gate-stress`（509d990，编排者已推送）
- PR：https://github.com/boris-w/cupper/pull/39
- 完成日期：2026-11-03

## 做了什么

1. **查原因：是测试问题，不是侧边栏在流式期间不停重排。** 证据整理在分支上的 `docs/tasks/T220-bridge-gate-stress/notes.md`，要点如下：
   - **按钮没动，是检查本身没返回。** 失败的调用日志里，按钮已经定位到，之后只有一行 "waiting for element to be visible, enabled and stable"，接下来 30 s 没有任何记录，也没有重试。Playwright 1.64.1 每次检查失败都会先记一行 "element is not stable/visible/enabled" 或 "intercepts pointer events"，再重试。一行都没有，说明那一次检查始终没返回。它判断"稳定"要靠渲染进程连续出两帧动画帧（`_checkElementIsStable`）。
   - **流式输出不会让侧边栏重排。** 侧边栏按 `last_activity_at` 排序。宿主只在回合开始/结束、待处理请求、工作区就绪时更新这个值（`index/status.rs` 的 `notes`），流式片段不动它。nightly 的快照里，6 个运行中的会话按创建顺序排着。
   - **点击一次比一次慢。** 从 host.log 看，窗口点开 g3、g4、g5 的间隔依次是 0.8 s、8.7 s、14.3 s，g6 等了 30 s 也没点开。6 路洪流在创建会话时就开始了，点得越晚，屏幕上那个会话积累的文字越多。快照里屏幕上的 g5 是一个段落：4,754,502 个字符，没有换行，还在增长。
   - **runner 的渲染进程跟不上这档负载。** 开始测量前测到 4 帧/s，作者机器上（T078）是 102–107 帧/s。同一次 nightly 的判定（可见）用例里，`getBlob` 的 evaluate 到约 41.6 s 才返回，20 s 的测量段被拉长到 51.7 s。
2. **修前对照** https://github.com/boris-w/cupper/actions/runs/41572714595 （main 5b92e9c，压力用例 ×30，workers=2）：**26 通过，4 失败**。
   - 3 次和 nightly 一样，卡在 g6 的稳定检查上。
   - 1 次是第二个测试问题：侧边栏每个项目只显示最近活动的 5 个会话（`SHOWN_SESSIONS = 5`），6 个 g 会话加 1 个种子会话一共 7 个。同时创建时，各回合开始的先后不固定，这一次 g2 排到了 g3 前面，g3 被挤进"展开显示"，`getByRole` 一直找不到它。
3. **修测试**（`tools/e2e/src/gate.spec.ts` 的 `run()`）：
   - 逐个创建会话，每个只说 `say gN`，等它第一回合结束再建下一个。这样最后建的 4 个一定在最近 5 个里。
   - 趁渲染进程空闲，按原来的顺序点开最后 4 个，门钩子跟随其余会话。
   - 再用 CLI 给每个会话 `send` 洪流，等每条流都出现洪流的 `item.delta` 后，才开始测帧率和计时。
4. 规模：+22 −7（`cargo xtask size`）。

## 关键取舍

- **负载参数一项没改**：路数、速率、片长、洪流时长、窗口持有 4 个会话加门钩子跟随其余、5 s 的 blob、10 s 的重同步、不断增长的索引都照旧。
- **测量段的实际负载变重了。** 修前，洪流从创建会话时开始、共 50 s，准备步骤要 10–40 s，测量开始时洪流往往已经过半，测量段后半常常没有负载。修后洪流在测量前一刻才开始，整个测量段都压着设计的负载。下面的数字也反映了这一点。
- **没采用的方案**：`force` 点击或 `dispatchEvent`（还是要等渲染进程主线程）；洪流前加 `sleep`（靠时间猜）。
- 两个判定用例也用 `run()`，一起改了。

## 验证方式

- 本机只跑了普通检查：prettier、eslint、`tsc --build tools/e2e`，都通过。按要求没有在本机跑压测。
- **修后对照** https://github.com/boris-w/cupper/actions/runs/41581454952 （509d990，同样参数）：**30/30 通过**。

| | 修前（通过的 26 次） | 修后（30 次） |
|---|---|---|
| 测量前的动画帧（帧/s） | 1–6，中位 3 | 4–36，中位 16 |
| 测量段实际时长（s，设计 20） | 20–58.5，中位 29.7 | 23.3–109.7，中位 71.5 |
| 出现流重置的次数 | 1 次（最多 1） | 18 次（最多 5） |
| 主进程 p99（ms，只记录） | 5.05–9.17 | 6.1–9.85 |
| 单次用时（s，超时 180） | 45.9–104.7 | 57.4–139.3 |

- **PR 的 ci** https://github.com/boris-w/cupper/actions/runs/41581407133 ：8 个检查全部通过（compat-framework、e2e、license、protocol、rust、testdata-scan、ts、ui）。
- 验收标准：
  - [x] 原因有证据。
  - [x] 修后在 runner 上连跑 30 次全部通过。
  - [x] PR 上 8 个检查通过。

## 提交列表

```
509d990 docs(T220): the runs before the fix, and the sidebar's five sessions
f84be6e test(e2e): the bridge gate makes its sessions one turn at a time
ebfb4ff docs(T220): notes on why the stress run could not open its sessions
0ab0c18 test(e2e): the bridge gate opens its sessions before they flood
```

notes.md 只记到修前对照，修后的数字只在这份汇报里。我没有再提交，以免 PR 的 CI 要重跑。

## 遗留问题 / 范围外发现

1. **合并前请先验证两个判定用例（风险）。** 它们也用 `run()`，并断言没有流重置（`reset` 为 0），这条断言不受 `CUPPER_PERF_RECORD_ONLY` 控制。PR 的 ci 只跑 @smoke，这次 e2e-repeat 也只跑了压力用例，所以它们在修后、满负载下还没有在 runner 上跑过。压力用例修后有 18/30 次出现重置；判定负载每路速率是压力用例的一半、只有 4 路，会不会触发重置还不知道。建议合并或换 `gate/m1a` 候选前，先在这个分支上手动跑一次 nightly。如果判定用例在 runner 上出现重置，要决定是否把"没有流重置"也改成在托管 runner 上只记录（M1a 同步会 D1 的口径），这不在 T220 的范围内。
2. **超时余量**：修后单次最长 139 s，离 180 s 的超时只剩约 40 s。
3. **测量段被拉长**：修后 30 次里有 23 次测量段超过 45 s，接近或超过 50 s 的洪流，所以测量段的尾部可能已经没有负载。原因是 evaluate 要排在忙碌的渲染进程后面。runner 上的 p99 可比性因此有限。
4. **渲染进程在洪流下很慢**：屏幕上那一段几 MB、没有换行的文字很可能被 streamdown 整块反复渲染（推测）。这属于 SP-6（M1b，设计 5.9.7）的范围。
