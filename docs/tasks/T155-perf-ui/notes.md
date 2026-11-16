# T155 笔记

## 开工前的估算（2026-11-16）

### 做法

- 四个用例放进 `tools/e2e/src/perf-ui.spec.ts`（或按用例拆成几个文件），带 `@perf` 标签，进 nightly 的 e2e 作业，与 PERF-bridge-gate 的放法一致。
  - 标题以编号开头，trace-ids 据此认作执行过。
  - 每个指标按 gate.spec 的格式写进 `CUPPER_PERF_OUT`（`{metric, unit, samples}`），控制台打一行汇总。
  - 只记录，不按预算判定。
- 测量助手从 `spikes/T103-sp6-render/measure.ts` 搬进 `tools/e2e/src/perf.ts`：统计、帧采样、CPU 和内存采样、`test/seed`（长等待，id 不与 `initialize` 冲突）、带 `ts` 的事件、经链接打开会话并计时到首屏。spike 本身不改。
- 计时用渲染进程的标记和事件的 `ts`（T262）。
- 夹具：
  - **代码块**：
    - 宿主的 `test/seed` 加一种配比 `sp1_code`：与 `sp1` 相同，另外每 5 条 agent 消息里有一条带一段短的 Rust 代码块。这是协议里开放枚举的附加值，经 T143 的高亮。
    - 流式消息：先让假 agent `say` 一段代码块，再开始 `flood_rate`，同一条消息里先是代码块、后是洪流。假 agent 的 `say` 目前不转义 `\n`，要像 `write_file` 一样把 `\n` 换成换行（一行）。
  - **折叠的回合**：灌入的回合都是已结束的回合，在界面里按 T146 折叠，场景 4 本来就在折叠的历史上滚动、翻页。

### 场景 8 的办法（单列）

- T078、T103 在 Playwright 下遮住窗口，页面一直是 `visible`。我推测原因是 Playwright 对每个页面开了 CDP 的焦点模拟（`Emulation.setFocusEmulationEnabled`）。Chromium 实现焦点模拟时会给 WebContents 加一个"捕获者"，有捕获者的页面一律当作可见。这一点还没核实。
- 办法：
  - 用例在遮挡前经 `cupper.window.context().newCDPSession(cupper.window)` 对这个页面关掉焦点模拟；
  - 用 `throttled` 启动（不带关闭后台节流的开关）；
  - 主进程放置顶窗口遮住，再查 `visibilityState`。还是 `visible` 的话，再试把窗口最小化。
- 都不成就记"环境不合格"、不计结果。办法和结论都写进汇报。先在第一次 nightly 上看结果。

### 行数估计

| 项 | 内容 | 估计 |
|---|---|---|
| 测量助手 | 从 spike 搬进 `tools/e2e/src/perf.ts`，加 PERF 记录的写法 | 160 |
| PERF-ui-stream | 场景 2（洪流 + 打字：帧时间、输入延迟、输出延迟；流式消息带代码块）+ 场景 3（4 路流 + blob + 重新同步：帧时间、流重置；6 路作为压力报告）。不带 T103 的 `fs_usage` 那部分 | 160 |
| PERF-ui-long-session | 场景 4（1,000、5,000 和单回合 2,000，用 `sp1_code`：首屏、滚动帧时间、内存、锚点位移） | 100 |
| PERF-ui-switch | 场景 7：LRU 内外，加"从通知跳过去"（`permission` 造待处理的请求，窗口在别处时宿主记下通知，`test/notification_click` 点它，计时到首屏） | 90 |
| PERF-ui-occluded | 场景 8，含上面的办法 | 70 |
| 宿主和假 agent | `sp1_code`（宿主 seed、协议的附加值、`pnpm gen`、宿主的单元测试）；假 agent 的 `say` 转义 `\n` | 45 |
| **合计** | | **约 625** |

超过停止线 600。可以减的，按顺序：
1. 不录 Chromium trace。任务书写的是"可以顺带"，上面没有算进去，就是不做。
2. 不采 CPU（Electron 的 `percentCPUUsage` 口径本来就待核实），约 −25。
3. PERF-ui-stream 里的 6 路压力报告只记帧时间和流重置，不另外统计 blob 和重新同步，约 −10。
