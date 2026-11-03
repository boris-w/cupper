# T220 笔记：压力用例点不开会话的原因

失败：nightly 41445179537（候选 5da6f1c）的 e2e，`gate.spec.ts:246` 压力用例（6 路、每秒 200 片段、每片 500 字符）。附件取自构件 `e2e-macos-15-playwright` 的 `rest/`，e2e 原始日志取自 `e2e-macos-15-logs`；对照用 T219 分支通过的 nightly 41437271713 的 `e2e-macos-15-logs`。

## 证据

1. **按钮没有动，是检查本身没返回。** 调用日志只有：定位到按钮（`locator resolved to <button …>`）→ `attempting click action` → `waiting for element to be visible, enabled and stable`，之后 30 s 什么也没有。Playwright 1.64.1 的 `_retryAction` 每次检查失败都会记一行 `element is not stable / visible / enabled` 或 `… intercepts pointer events`，然后重试；这里一行都没有，说明那一次 `checkElementStates` 在 30 s 内没返回。它的"稳定"检查靠渲染进程的 `requestAnimationFrame` 连续取两帧的位置（`_checkElementIsStable`），渲染进程不出帧就一直等。
2. **侧边栏没有重排。** 侧边栏按 `last_activity_at` 倒序（`ui-core/src/sidebar/parts.tsx`）；宿主只在回合开始、结束、待处理请求、工作区就绪这几种状态变化时更新它（`index/status.rs` 的 `notes`，`Note` 枚举），流式片段不更新。页面快照里 6 个会话都是"运行中"，顺序是创建顺序 g6、g5、g4、g3，g6 的按钮就在列表里，没有遮挡。
3. **每次点击越来越慢。** host.log 里窗口订阅会话流的时间：g3 12:48:04.57（`cupper sessions` 之后 0.8 s，含 5 MB blob 上传）、g4 13.27（+8.7 s）、g5 27.57（+14.3 s），g6 到 12:48:57 失败都没订阅。6 路洪流在 12:48:01.8–03.1 随会话创建就开始了，点得越晚，屏幕上那个会话积累的文字越多。
4. **屏幕上的会话是一段几 MB、还在增长的文字。** 快照里 g5 的回答是一个段落，4,754,502 个字符、没有换行（9,509 个片段），"已处理 54 秒"，仍在输出。助手文本经 streamdown 按 Markdown 渲染；推测这一整段只能作为一个不断变长的块反复渲染，渲染进程主线程被占满。
5. **runner 上渲染进程本来就跟不上这档负载。** `gate` 行里点开会话、开始测量前测的每秒动画帧：作者机器 T078 压力用例 102–107；T219 nightly 压力用例 4、判定（可见）16；本次 nightly 判定（可见）4。本次判定（可见）用例里，5 s 处的 `getBlob` evaluate 在开始后约 41.6 s 才返回（`TimeoutNegativeWarning: -31647`），20 s 的测量段拉长到 51.7 s。

## 结论

测试问题：会话一创建就开始洪流，种子、blob 和前面的点击让屏幕上的会话先积累了几秒到几十秒的文字，之后才去点侧边栏；托管 runner（3 核虚拟机）的渲染进程在这档负载下几乎不出帧，依赖动画帧的点击检查等不到。不是"侧边栏在流式期间重排"那种产品问题。渲染进程在这么大负载下变慢属于 SP-6（M1b 的渲染压力测试，设计 5.9.7）的范围。

## 修法

`run()` 里会话先只说一句 `say gN`，等第一回合结束，窗口按原来的顺序点开最后 4 个、门钩子跟随其余的，然后 CLI 给每个会话 `send` 洪流，等每条流都出现洪流的 `item.delta` 后再测帧率、开始计时。点击发生在渲染进程空闲时，不再和洪流赛跑。

负载不变：会话数、速率、片段长度、洪流时长（`SECONDS + 30`）、窗口持有的 4 个会话和门钩子跟随的其余会话、5 s 的 blob、10 s 的重同步、增长的索引都照旧。差别只是测量开始时屏幕上的会话积累的文字少了（洪流开始后约数秒，而不是此前不定的 5–30 s），这部分本来就随准备步骤的快慢浮动。两个判定用例共用 `run()`，一并受益。

考虑过的其他做法：
- `click({ force: true })` 或 `dispatchEvent("click")`：跳过稳定检查，但 force 仍要滚动和派发真实输入，`dispatchEvent` 也要等渲染进程主线程；仍在和洪流赛跑，只是把门槛放低。
- 洪流前加 `sleep`：靠时间猜，准备步骤慢了还是会撞上。
