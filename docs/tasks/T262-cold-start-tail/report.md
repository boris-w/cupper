# T262 汇报

- 执行者：子 agent（前端与 e2e）
- 分支：`perf/T262-cold-start-tail`
- 完成日期：2026-11-16

T103 测到的冷启动长尾不在 App 里：慢在 Playwright 的 `cupper.launch()` 返回得晚，App 本身在约 2 s 内已经可用，所以没有修产品代码。三个启动标记已加进产品；PR #115（https://github.com/boris-w/cupper/pull/115）在 `8a69d843` 上 8 个检查全部通过（CI 运行 43793746779）。规模：产品代码和测试 +125（停止线 250）；`spikes/` 另计，+49 −19。

## 做了什么

**渲染进程的启动标记**（a2e21669，cb302e62）。三个都放在 ui-core 里，由 `app/startup.ts` 记录，每个文档只打一次；打标记只读状态，不改变行为。

| 标记 | 含义 | 打点位置 |
|---|---|---|
| `cupper:connected` | 窗口的 HostClient 第一次 `ready`，即窗口数据连接握手完成 | `HostProvider` |
| `cupper:index` | 索引流第一次 `live`，即快照已应用到视图 | 索引专用的 `StreamStore` 子类，在 `Subscriptions` 里 |
| `cupper:ready` | 索引已应用，且输入框已挂载，两者都成立的那一刻 | `Composer` 挂载时，以及索引 live 时 |

- "输入框可用"按 Composer 挂载算：输入框本身从不禁用，只有发送按钮会。
- 设计 5.9.6 的标记清单请编排者按上表合并时补；"当前会话已渲染"要等 Q45。
- 测试：
  - 单元测试覆盖每个标记只打一次，以及 `ready` 在"索引先到"和"输入框先挂载"两种顺序下都只在两者都成立后才打。
  - App 级组件测试用 mock 宿主把整个 App 跑起来，断言三个标记按顺序出现。我验证过：去掉索引那一处，这个测试会失败。
- **PR 上的 E2E-NFR1-1 失败是我第一版造成的，已修。** 我给索引的 sink 包了一层，e2e 构建的 `__cupperTest.views()` 从 sink 上读 `view`，包装后读不到，`complete` 永远不成立。改为索引用自己的 `StreamStore` 子类，在 `live()` 里打标记，sink 仍是 store 本身（cb302e62）。之后 PR 的 e2e 作业通过。

**spike 的场景 1**（f386ceb6、f2e74571、bbfba33e）：
- ready 改用 `cupper:ready` 的时刻。
- 每次冷启动按以下各段记录，都从启动前的时刻算起：

| 段 | 来源 |
|---|---|
| 主进程起来、宿主起来、控制连接就绪 | 日志 |
| 页面开始导航 | `timeOrigin` |
| DOM 就绪 | 导航计时 |
| 首次绘制 | FCP |
| 窗口连接、索引已应用、ready | 标记 |
| `cupper.launch()` 返回（`launched`） | 测试端 |
| 测试看到 ready（`polled`） | 测试端 |
| 页面的长任务：合计和最后结束时刻 | `longtask` 观察器 |

## 关键取舍

- **标记放在 ui-core，不放在 host-client**：主进程的控制连接也用 HostClient，放在那里主进程也会打标记。
- **没有修产品代码**：原因在测试工具，App 里没有可修的地方。
- **没有修前修后对照**：因为没有修复，以下三次运行就是全部数据。编排者已同意这样记。

## 验证方式

**runner 上三次场景 1，各 30 次冷启动：**

| 运行 | 提交 | ready p50 / p95 / 最大 | 测试看到 ready（`polled`）最长 |
|---|---|---|---|
| [43421848900](https://github.com/boris-w/cupper/actions/runs/43421848900) | f386ceb6 | 1.24 / 1.82 / 1.94 s | （这次没有记） |
| [43789779212](https://github.com/boris-w/cupper/actions/runs/43789779212) | f2e74571 | 1.24 / 2.05 / 2.21 s | 19.2 s（30 次中 15 次超过 3 s） |
| [43792051950](https://github.com/boris-w/cupper/actions/runs/43792051950) | bbfba33e | 1.17 / 2.04 / 2.09 s | 35.1 s |

- 三次运行里，主进程、宿主、控制连接、页面、DOM、首次绘制、窗口连接、索引、ready 各段都在 2.2 s 以内，没有长尾。
- 43792051950 里慢的那几次，`launched` 与 `polled` 几乎重合：

| `launched` | `polled` |
|---|---|
| 33.1 s | 35.1 s |
| 27.8 s | 30.8 s |
| 14.2 s | 15.1 s |
| 9.9 s | 20.8 s |
| 7.4 s | 7.4 s |
| 7.2 s | 7.4 s |

- 页面的长任务合计最多 328 ms，最后一个结束在 2.1 s 以内，页面不忙。
- 所以多出来的时间在 Playwright 的 `_electron.launch`、`firstWindow` 和对话框替身的接入。个别 `polled` 比 `launched` 再晚几秒，应是 Playwright 在页面里轮询时的 CDP 往返，长任务解释不了。
- **结论**：T103 的长尾是测量方式造成的。它从 Playwright 拿到窗口、再在页面里轮询来计时。产品在 runner 上冷启动到可用 p95 ≈ 2.0 s，与 2 s 的预算基本持平。

**本机和 CI：**
- ui-core 的 158 个测试通过，tsc、eslint、prettier 干净。
- spike 代码在仓库的基础配置下类型检查通过。
- PR #115 在 8a69d843 上 8/8 通过。

**对照验收标准：**
- 三个标记在产品里，单元测试和组件测试覆盖了顺序，标记只读状态、不改变行为。
- runner 上的分段数据如上；尾部的原因已查清，不在应用代码里，所以没有修。
- PR 8 个检查全部通过。

## 提交列表

```
8a69d843 docs(task): T262 notes on where the cold start's tail was
bbfba33e test(spike): scenario 1 records Playwright's launch and the page's long tasks
f2e74571 test(spike): scenario 1 also records the first paint and when the test saw ready
cb302e62 fix(ui): the index's start mark comes from its store, not a wrapper sink
f386ceb6 test(spike): scenario 1 reads the renderer's start marks
a2e21669 feat(ui): the renderer marks its start: connected, index, ready
378584f3 docs(task): T262 estimate by module
```

## 遗留问题 / 范围外发现

- **给 T155 的建议**：冷启动从 `cupper:ready` 计时（`performance.timeOrigin + startTime`，与启动前的时刻相减），不要用 Playwright 的 launch 返回时间，也不要用页面内轮询的时刻。在 runner 上，这两者会被 Playwright 自身的开销拉长到几十秒。
- **设计 5.9.6** 需要补三个标记的名字和含义（见上表），由编排者合并时补。
- **T103 报告的修正**：报告里冷启动"p95 30–36 s、尾部有数量级问题"的判读应改为"测量方式造成"。去向由编排者定。
- 过程记录在 `docs/tasks/T262-cold-start-tail/notes.md`（已提交）。
