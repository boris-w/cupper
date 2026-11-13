# T103 笔记

## 开工前的估算（2026-11-13）

### 做法

- 测量脚本放在 `spikes/T103-sp6-render/`，是一组 Playwright 用例，直接用 `tools/e2e` 的夹具（`Cupper`：临时根目录、假 agent、CLI、`launch`、`connected`）、`nfr1.ts` 的 `session`/`openSession`、`input.ts` 的 `focusWindow`/`press`，以及渲染进程里已有的 `window.__cupperTest`、`window.__cupperGate`（T078）。
- 用例不 `import "@playwright/test"`：spike 目录不在 pnpm 工作区里，从那里解析会得到第二份 Playwright。配置写成不 import 任何包的普通对象，用 `tools/e2e` 的 Playwright 来跑（`pnpm --dir ../../tools/e2e exec playwright test -c <spike 的配置>`），`globalSetup` 用 `tools/e2e/src/setup.ts`。
- `gate.spec.ts` 里的 `harness`（`test_harness` 连接，`test/seed` 走它）、`putBlob`、`cover`、`frameRate` 写在用例文件里，spike 引用不到。按任务书"能直接用的就直接用，不要复制一份走样的"，把这四个挪进 `tools/e2e/src/perf.ts`，`gate.spec.ts` 和 spike 都从那里引用。行为不变，T155 把场景改写成每晚的回归时也用得上。
- 夹具：`test/seed` 的 `sp1` 已经按 SP-1 的分布生成（T047：43% 消息约 67 字、42% 工具调用、15% 思考约 44 字），`large_turn_items` 生成单回合 2,000 个条目。spike 只组合 profile，不再自己生成内容。

### 与设计的出入（开工前先说明）

- **`cupper:ready` 这个 mark 产品里没有**，App 启动时也不恢复上次的会话，总是停在新会话页。不改产品代码的前提下，场景 1 在渲染进程里按 rAF 轮询这几个条件：索引视图已应用（`__cupperTest.views().index.complete`）、新会话页的输入框可用；同时满足的那一帧就算 ready，时间为 `performance.timeOrigin + now`。起点是 Node 侧调用 `_electron.launch` 之前的 `Date.now()`。"当前会话已渲染"换成"打开那个 1,000 条消息的会话到首屏"，单独计时（与场景 4、7 同法）。误差最多一帧，以及 Playwright 接上渲染进程之前的那一段（如果条件在接上之前就已满足，就记下来）。
- **输出到屏幕的延迟**：假 agent 的 `flood_rate` 片段里没有时间戳（第 i 个片段是 i%26 对应字母重复 len 次）。渲染进程每帧读一次正在流式输出那条消息的文字长度，长度除以 len 就是上屏的片段数；宿主事件的 `ts` 从 `host.db` 读出。对每个 `item.delta` 事件，延迟 = 第一个显示到它的帧 − 事件的 `ts`。这里不含宿主写入前那最多 40 ms 的合并等待，报告里写明。
- **场景 8**：T078 记录过，在 Playwright 下页面被遮住、最小化、隐藏时都还是 `visible`。所以这一项很可能按"环境不合格"记录。会用 `throttled` 启动（不带关闭后台节流的开关），断言前先记下 `document.visibilityState`。

### 行数估计（只算测量脚本，报告和数据文件不计）

| 项 | 内容 | 估计 |
|---|---|---|
| 共用 | `playwright.config.ts`（约 20）；`run.sh`：构建 e2e、按参数跑场景、结果写进 `results/`（约 30）；`measure.ts`：渲染进程的 rAF 帧采样（开始、停止、p50/p95/最大值）、`app.getAppMetrics` 的 CPU 和内存、宿主的 CPU（`ps`）、结果写盘（约 90） | 140 |
| 挪到 `tools/e2e/src/perf.ts` | `harness`、`putBlob`、`cover`、`frameRate`（xtask 按新增行算，挪动也计入） | 65 |
| 场景 1 | 灌 200 个会话和 1 个 1,000 条的会话；冷启动 30 次（退出 App、等宿主退出）、热启动 30 次（只杀掉 App 进程，宿主留着）；第一次启动单独计时 | 70 |
| 场景 2 | `flood_rate 200 120 <len>`；聚焦后用 `sendInputEvent` 每 100 ms 打一个字，主进程记下发送时刻，渲染进程按帧读输入框；帧时间；每秒采样 CPU；上屏延迟（见上） | 110 |
| 场景 3 | 4 个会话同时流式输出，第 5 s 拉 5 MB blob，第 10 s 重新同步 5,000 个事件；渲染帧时间、`resets`；再跑 6 条流作为压力报告；从 `host.db` 读各条流相邻事件的 `ts` 间隔（T206 那条：最长停顿，以及与宿主日志里 WAL 检查点的时间对照） | 100 |
| 场景 4 | 1,000、5,000 条消息的会话，以及单回合 2,000 个条目的会话：打开到首屏、从顶滚到底的帧时间、渲染进程内存；向前翻页加载历史前后，比较视口顶行的 `getBoundingClientRect().top` | 120 |
| 场景 7 | LRU 里的会话、不在 LRU 里的会话、"下一个需要处理的会话"（假 agent 的 `permission` 造一个待处理的请求）：从发出切换到目标会话首屏可见的帧 | 70 |
| 场景 8 | `throttled` 启动、洪流 10 分钟、主进程放置顶窗口遮住；断言 `visibilityState`；移开后的首帧时间、`resets` | 60 |
| **合计** | | **约 735** |

停止线 800。按我前几个任务的偏差（多出来的主要在用例和共用部分，约 1.3–1.5 倍），有越过停止线的风险。如果越线，可以拆的办法：场景 3 少做 T206 的检查点对照（约 −30），场景 4 只做 1,000 和 2,000 两种会话（约 −15）。也可以把场景 8 缩成只做"遮住后断言 `visibilityState`"：不合格就记录、不跑那 10 分钟（约 −30）。

### `spike-run` 的调用方式

- `dir`：`spikes/T103-sp6-render`
- `command`：`bash run.sh <场景…>`。`run.sh` 先构建（照 e2e-repeat：`cargo xtask runtime-pack`、`cargo build --locked -p cupper-fake-agent`、`pnpm build:e2e`，runner 上约 9 分钟），再按参数跑对应的用例，结果写进 `results/`。
- 分三次触发，避免一次作业太长、一个场景拖累其他场景：
  1. `bash run.sh 1 2 7`：构建约 9 分钟 + 场景 1 约 10 分钟 + 2 约 4 分钟 + 7 约 3 分钟，`minutes` 填 60；
  2. `bash run.sh 3 4`：构建 + 场景 3 约 8 分钟 + 4 约 8 分钟，`minutes` 填 60；
  3. `bash run.sh 8`：构建 + 约 15 分钟，`minutes` 填 45。
- 本机只做一次单份、短时的冒烟（例如 `bash run.sh 7`，不构建、用已有的 e2e 构建），确认脚本能跑通；测量一律放在 runner 上。
