# T103 汇报

- 执行者：子 agent（前端与 e2e）
- 分支：`spike/T103-sp6-render`
- 完成日期：2026-11-13

场景 1–4、7、8 都在 runner 上跑出了数据，场景 8 按"环境不合格"记录。PR #114（https://github.com/boris-w/cupper/pull/114）在 `f00af90a` 上 8 个检查全部通过（CI 运行 43414985330）。规模 +837，停止线 880。

**需要你处理一件事**：`report-sp6.md` 没有提交。我的写文件工具拒绝子 agent 写报告文件（"Subagents should return findings as text"），我没有改用 shell 绕过。全文附在本汇报末尾的"附：report-sp6.md 全文"，请存为 `docs/tasks/T103-sp6-render/report-sp6.md`；要我自己写的话请告诉我。

## 做了什么

- **把 `gate.spec.ts` 里的 `harness`、`putBlob`、`cover`、`frameRate` 搬到 `tools/e2e/src/perf.ts`**（31176520，提交信息写明是搬移）。场景 8 需要遮住窗口但不停帧，所以 `cover()` 加了一个默认为 true 的 `{ stopFrames }` 选项，gate.spec 的行为不变。
- **`spikes/T103-sp6-render/`**：
  - 不 import `@playwright/test` 的配置，用 `tools/e2e` 的 Playwright 和全局 setup 来跑。
  - `run.sh`：照 e2e-repeat 构建 e2e 应用，再跑指定的场景；每个场景有自己的输出目录。
  - `measure.ts`：帧采样、统计、结果写盘、CPU 和内存采样、`test/seed`、从 `host.db` 读带 `ts` 的事件、经会话链接打开会话并计时到首屏。
  - 六个场景各一个用例文件，外加 README。
- **在 runner 上跑了 9 次 spike-run**，期间修了几个脚本问题，每次的原因都写在提交里：
  - `seed` 的请求 id 和 `initialize` 撞了；
  - 场景 4 用程序滚动会被列表复原（T245：只有用户能让列表离开底部），改为先发 wheel 事件再滚；
  - 场景 3 两次运行之间 `fs_usage` 没停下（它以 root 运行，信号发给 sudo 传不到）；
  - 场景 1 加了按日志分段计时。
- 冷启动慢的那几次查到了是哪一段：控制连接就绪之后、渲染进程 ready 之前，即窗口这一侧（详见附文）。

## 关键取舍

- **`cupper:ready` 和"当前会话已渲染"**：没有改产品代码。用替代定义来测（索引视图完整，且输入框可用），"打开 1,000 条的会话"单独计时。报告里作为偏差单列。
- **热启动**：e2e 构建做不到。`dev_key` 只认拉起宿主的那个进程，App 退出后宿主孤立并退出（设计 5.8.1）。所以场景 1 只测冷启动，热启动要等 Q35 或发布构建。
- **输出到屏幕的延迟**：从宿主事件的 `ts` 起算，不含宿主写入前最多 40 ms 的合并等待。没有改假 agent。
- **T206 的交叉核对**：用 `sudo -n fs_usage` 记宿主的 fsync，只在 runner 上运行；本机要密码，自动跳过。

## 验证方式

- **runner 上的运行**：

| 运行 | 内容 | 结果 |
|---|---|---|
| 43409302612 | `run.sh 1 2 7` | 场景 2 的数据 |
| 43409310647 | `run.sh 8` | 环境不合格 |
| 43410584669 | `run.sh 1 7` | 场景 7 的数据 |
| 43412878272、43414975876 | 场景 1 | 30 次冷启动；后一次带分段 |
| 43410588822、43412882455、43414734406 | 场景 3、4 | 最终数据取最后一次 |
| 43409306527 | — | 已取消 |

- **本机**：spike 代码在仓库的基础配置下类型检查通过；`tools/e2e` 的 tsc、eslint、prettier 通过。
- **本机冒烟没做成**：屏幕锁着，e2e 的全局 setup 拒绝运行。按你的意见，第一轮 runner 就当冒烟。
- **对照验收标准**：
  - 场景 1–4、7、8 在 runner 上都至少跑了一次，场景 8 记"环境不合格"。
  - 报告覆盖第 3、4、5 项（见附文）。
  - PR 8 个检查全部通过。

## 提交列表

```
f00af90a test(spike): scenario 1 records where each cold start spent its time
c2104941 fix(spike): scenario 3 stops fs_usage itself, as root
557d2fef fix(spike): scenario 4 scrolls as the user does, after a wheel event
674da26d fix(spike): scenario 1 times cold starts only; each scenario keeps its output
5dc620c3 fix(spike): scenario 4 pages back before it scrolls; fs_usage ends between runs
30eb52a7 fix(spike): seed's request takes an id of its own
9fa83330 docs(spike): T103 README
ff11221a test(spike): SP-6 scenario 8, streaming covered then uncovered
c6a1ce7b test(spike): SP-6 scenario 7, switching sessions in and out of the LRU
7e3a5352 test(spike): SP-6 scenario 4, long sessions scrolled and paged back
ed74de9a test(spike): SP-6 scenario 3, four and six streams with a blob and a resync
ab792d14 test(spike): SP-6 scenario 2, one session's flood while typing
0e27eb65 test(spike): SP-6 scaffolding and scenario 1, cold and warm starts
31176520 refactor(e2e): move the gate's measuring helpers to perf.ts
08329069 docs(task): T103 estimate by scenario and the spike-run calls
```

## 遗留问题 / 范围外发现

以下由你定去向，详见附文第 3、4 节：

- **冷启动尾部有数量级问题**：p95 30–36 s，预算 2 s；p50 2–3 s。慢在窗口这一侧（页面加载、窗口数据连接、索引快照、输入框可用之一）。要渲染进程里的分段标记才能再往下查。
- **翻页时锚点位移**：5,000 条的会话翻了 60 页，有 22 页位移 26 px（预算 ≤ 1 px）。推测原因在 `StreamList.tsx` 的 `useLayoutEffect` 和 `ScrollKeeper.restore`：锚点只复原一次，用的是新行还没实测时按 44 px 估算的位置；未验证。
- **`next_needs_action` 没有处理方**：菜单里有（Command+Alt+A），ui-core 不处理，所以场景 7 的这一项测不了。
- **产品里没有 `cupper:ready` 标记，启动时也不恢复当前会话。**
- **热启动要等 Q35 或发布构建才能测。**
- **场景 8**：在 Playwright 下页面始终是 `visible`，要真机或 Playwright 之外的启动方式才能测。
- **Electron 的 `percentCPUUsage` 口径待核实**：主进程约 0.6，偏低。
- **T206**：单份负载下没有复现，可用 e2e-repeat 一次 3 份并开 `fs_usage` 再验证。
