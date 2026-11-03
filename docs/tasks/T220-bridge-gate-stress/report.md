# T220 汇报

- 执行者：子 agent（e2e 与桌面工程师）
- 分支：`fix/T220-bridge-gate-stress`（最后一个提交 cb8730a，已请编排者推送）
- PR：https://github.com/boris-w/cupper/pull/39
- 完成日期：2026-11-03

## 做了什么

1. **查原因：是测试问题，不是"侧边栏在流式期间不停重排"。** 证据记在分支上的 `docs/tasks/T220-bridge-gate-stress/notes.md`：
   - **按钮没动，是检查本身没返回。** 调用日志里按钮已经定位到，之后只有一行 "waiting for element to be visible, enabled and stable"，30 s 里没有别的记录，也没有重试。Playwright 1.64.1 每次检查失败都会先记一行 "element is not stable/visible/enabled" 或 "intercepts pointer events" 再重试；一行都没有，说明那一次检查一直没返回。它判断"稳定"要靠渲染进程连续出两帧动画帧（`_checkElementIsStable`）。
   - **流式输出不会让侧边栏重排。** 侧边栏按 `last_activity_at` 排序。宿主只在回合开始/结束、待处理请求、工作区就绪时更新这个值，流式片段不动它。nightly 的快照里，6 个运行中的会话按创建顺序排着。
   - **点击一次比一次慢。** host.log 里窗口点开 g3、g4、g5 的间隔依次是 0.8 s、8.7 s、14.3 s，g6 等了 30 s 也没点开。6 路洪流在创建会话时就开始了，点得越晚，屏幕上的会话积累的文字越多。快照里屏幕上的 g5 是一个 4,754,502 字符、没有换行、还在增长的段落。
   - **runner 的渲染进程跟不上这档负载。** 开始测量前只有 4 帧/s，作者机器（T078）上是 102–107。同一次 nightly 的判定（可见）用例里，`getBlob` 的 evaluate 拖到约 41.6 s 才返回，20 s 的测量段被拉长到 51.7 s。
2. **修前对照** https://github.com/boris-w/cupper/actions/runs/41572714595 （main 5b92e9c，压力用例 ×30，workers=2）：26 通过，4 失败。
   - 3 次和 nightly 一样，卡在 g6 的稳定检查上。
   - 1 次是第二个测试问题：侧边栏每个项目只显示最近活动的 5 个会话（`SHOWN_SESSIONS = 5`），这里一共 7 个会话。同时创建时回合开始的先后不定，那次 g2 排到了 g3 前面，g3 被挤进"展开显示"，所以找不到它。
3. **修测试**（`tools/e2e/src/gate.spec.ts` 的 `run()`）：
   - 逐个创建会话，每个只说 `say gN`，等它第一回合结束再建下一个，这样最后建的 4 个一定在最近 5 个里。
   - 趁渲染进程空闲，按原来的顺序点开最后 4 个，门钩子跟随其余会话。
   - 再用 CLI 给每个会话 `send` 洪流，等每条流都出现洪流的 `item.delta`，才开始测帧率和计时。
4. **按编排者的裁定补改**（见文末一节）：托管 runner 上"没有流重置"也只记录；e2e-repeat 设 `CUPPER_PERF_RECORD_ONLY`；用例超时提到 240 s。
5. 规模：+33 −11（`cargo xtask size`），在停止线 80 以内。

## 关键取舍

- **负载参数一项没改**：路数、速率、片长、洪流时长、窗口持有 4 个会话加门钩子跟随其余、5 s 的 blob、10 s 的重同步、不断增长的索引都照旧。
- **修后测量段的负载实际更重了。** 修前，测量开始时洪流往往已经过半，测量段后半常常没有负载；修后整个测量段都压着设计的负载。
- **没采用的方案**：`force` 点击或 `dispatchEvent`（还是要等渲染进程主线程）；洪流前加 `sleep`（靠时间猜）。

## 验证方式

- 本机只跑了普通检查：prettier、eslint、`tsc --build tools/e2e`，都通过。按要求没有在本机跑压测。
- **修后对照** https://github.com/boris-w/cupper/actions/runs/41581454952 （509d990，压力用例 ×30，workers=2）：**30/30 通过**。

| | 修前（通过的 26 次） | 修后（30 次） |
|---|---|---|
| 测量前的动画帧（帧/s） | 1–6，中位 3 | 4–36，中位 16 |
| 测量段实际时长（s，设计 20） | 20–58.5，中位 29.7 | 23.3–109.7，中位 71.5 |
| 出现流重置的次数 | 1 次 | 18 次（最多 5） |
| 主进程 p99（ms，只记录） | 5.05–9.17 | 6.1–9.85 |
| 单次用时（s） | 45.9–104.7 | 57.4–139.3 |

- **PR 的 ci**：
  - 509d990 那次 https://github.com/boris-w/cupper/actions/runs/41581407133 ：8 个检查全部通过。
  - 41373e8 那次 https://github.com/boris-w/cupper/actions/runs/41585641594 ：只有 rust 作业失败，失败的是 `driver::runtime::tests::it_adapter_restart_backs_off_then_crashed`。这是 T111 新加的重启用例，里面有竞争，和本任务的改动无关，编排者已经开 T224 处理。其余 7 个检查通过。合并前需要在最终提交上重新拿到 8 个检查全部通过。
- 验收标准：
  - [x] 原因有证据。
  - [x] 修后在 runner 上连跑全部通过（30/30，裁定后的 60/60 见文末）。
  - [ ] PR 上 8 个检查全部通过：509d990 那次全部通过；41373e8 那次因为 T224 的用例失败了一项，最终提交 cb8730a 的 ci 还要等推送后再看。

## 提交列表

```
cb8730a docs(T220): the three tests twenty times each after the ruling
41373e8 docs(T220): the runs after the fix, and the orchestrator's ruling
119109d test(e2e): the bridge gate's tests get 240 s
1336516 test(e2e): the bridge gate only records a stream reset on a runner
635e38a ci(e2e-repeat): record PERF only, as the nightly e2e job does
509d990 docs(T220): the runs before the fix, and the sidebar's five sessions
f84be6e test(e2e): the bridge gate makes its sessions one turn at a time
ebfb4ff docs(T220): notes on why the stress run could not open its sessions
0ab0c18 test(e2e): the bridge gate opens its sessions before they flood
```

## 遗留问题 / 范围外发现

1. **渲染进程在洪流下很慢。** 屏幕上那一段几 MB、没有换行的文字很可能被 streamdown 整块反复渲染（推测）。这属于 SP-6（M1b，设计 5.9.7）的范围。
2. **压力用例的测量段在 runner 上会被拉长。** 两个压力用例同时跑时，测量段中位 71.5 s，超过 50 s 的洪流，测量段尾部已经没有负载，所以 runner 上压力报告的 p99 可比性有限。nightly 里 e2e 用 2 个 worker 跑，压力用例只有一份，情况更接近裁定后那次（中位 28.8 s）。
3. rust 作业里重启用例的竞争已由 T224 跟进。

## 裁定后的改动与结果

编排者的裁定（依据 M1a 同步会 D1）和对应改动：
- **1336516**：PERF-bridge-gate 的预算是"p99 < 5 ms"和"没有流重置"两条，托管 runner 上都只记录。设了 `CUPPER_PERF_RECORD_ONLY` 时，判定用例不断言 `reset`，照常写进 `gate` 行；没设时照旧断言。文件头的注释写明了依据：流重置是设计 5.7.1 的流控行为，不丢数据。
- **635e38a**：e2e-repeat 的 Run 步骤设 `CUPPER_PERF_RECORD_ONLY: "1"`，和 nightly 的 e2e 口径一致，三个用例可以一起重复跑。
- **119109d**：`test.describe.configure` 的超时从 180 s 提到 240 s，注释写明 runner 上实测最长 139 s。
- **41373e8、cb8730a**：notes.md 补了修后对照的数字、这次裁定，以及下面这次 20 次的结果。

结果：e2e-repeat https://github.com/boris-w/cupper/actions/runs/41585786015 （41373e8，三个用例各 ×20，workers=2，设了开关）**60/60 通过**。数字取自 `gate` 行：

| 用例 | 出现流重置的次数 | 主进程 p99（ms，只记录） | 测量段实际时长（s） | 单次用时（s） |
|---|---|---|---|---|
| 判定，可见 | 0 / 20 | 5.66–10.33，中位 8.06 | 20.0–20.1 | 30.5–41.9 |
| 判定，遮挡（停帧） | 0 / 20 | 5.00–8.90，中位 6.73 | 20.0–20.1 | 28.9–36.1 |
| 压力报告 | 1 / 20（最多 2） | 4.54–7.37，中位 5.91 | 20.0–51.9，中位 28.8 | 39.4–84.8 |

- 两个判定用例在满负载下都没有出现流重置，测量段也没被拉长。它们在托管 runner 上的 p99 高于 5 ms，按 D1 只记录。
- 压力用例这次比修后那 30 次轻得多（重置 1/20 对 18/30，测量段中位 28.8 s 对 71.5 s）。原因是那 30 次里两个 worker 同时跑的都是压力用例，这次大多和较轻的判定用例错开。
- 单次最长 84.8 s，在 240 s 的超时之内。
