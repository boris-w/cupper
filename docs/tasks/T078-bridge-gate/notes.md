# T078 工作笔记

## 开工前的核对

- 交接：T076 的预测量（`docs/tasks/T076-desktop-window-bridge/premeasure.test.ts`、它的 `notes.md`：Node 里、没有原生扩展和渲染进程，p99 约 1.4 ms）；e2e 断连钩子；T077 的入方向多一次复制；T167、T093、T178 的夹具、持锁入口、不联网。
- 设计 5.7.1 的判定负载与判定线：p99 < 5 ms、没有流重置；可见与遮挡各一次；6 条流的压力报告只作参考。
- 界面的订阅策略（`ui-core/src/state/subscriptions.ts`）正好是"索引流 + 当前会话 + 最近 3 个"，所以 4 条会话流用真实界面打开四个会话得到，当前会话还会真的渲染。
- 重新同步：宿主只在客户端带着游标订阅时才重放事件（`store/streams/state.rs` 的 `subscribe_mode`），新订阅一律是快照，而快照最多 20 个回合、2,000 个条目。`test/seed` 只能新建会话，界面自己的订阅拿不到"从游标 0 起重放 5,000 个事件"。所以重新同步由 e2e 构建渲染进程里的一个钩子发起：对 `test/seed` 灌好的会话以游标 0 订阅，事件交给 T080 的 `StreamStore` 和 `SessionView` 应用。
- 索引流的负载：会话流式输出时索引几乎不动；用 `test/seed` 每 200 ms 新建一个空会话，让索引每秒约 5 帧（实测帧数和大小写进报告）。
- 5 MB 的 blob：测试先用原始客户端（`harness` 身份）上传，渲染进程经 T168 的 `getBlob` 下载。
- 遮挡：夹具为了别的用例关掉了被遮挡窗口的降频（T092 的三个开关），测量闸门要用默认行为，所以 `launch` 加一个不带这些开关的选项；遮挡用主进程里新开的一个窗口盖住 App 窗口，以渲染进程的 `document.visibilityState` 为准。
- 主进程事件循环延迟：经 `app.evaluate` 在主进程里用 `process.getBuiltinModule("node:perf_hooks")` 的 `monitorEventLoopDelay({resolution: 1})`，不在 App 代码里加东西。
- T077 的多一次复制：入方向每块数据都进 `FrameDecoder`（拼接后切帧，帧体再复制一次），JSON 只在重发表非空时解析。用任务目录下的一次性基准测它的耗时，再对照闸门里的延迟。

## 规模估计（开工前）

| 模块 | 内容 | 估计 |
|---|---|---|
| 渲染进程的闸门钩子 | `__cupperGate`：每条流的帧数与抽样字节、`getBlob`、从游标 0 的重新同步；`main.ts` 接线；`flavors.test.ts` 加名字 | 80 |
| 夹具与原始客户端 | `launch` 的默认降频选项；`harness` 接入、上传 blob | 35 |
| 主进程一侧的测量辅助 | 事件循环延迟、CPU、遮挡窗口 | 40 |
| PERF-bridge-gate 用例 | 准备（项目、灌入、blob、4 个会话、打开）、索引负载、判定（可见、遮挡）、压力报告（6 条流） | 170 |
| 合计 | | 约 **325** |

T077 复制的基准放任务目录（`docs/` 不计入）。停止线 850，硬停止线 950。每次提交后跑 `cargo xtask size`，记在下面。

## 按模块统计
