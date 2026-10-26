# T078 汇报（测量闸门：主进程转发）

- 执行者：子 agent（工程师）
- 分支：`test/T078-bridge-gate`
- 完成日期：2026-10-26

**结论：达标。** 判定负载下：
- 主进程事件循环延迟的 p99，可见时 1.60–1.63 ms，遮挡（停帧）时 1.54–1.65 ms，都远低于 5 ms；
- 6 次判定运行和 3 次压力运行都没有发生流重置；
- 主进程保持窗口转发，不需要触发 T079。

需要留意的局限：e2e 下 Chromium 不会把被遮挡的窗口判为隐藏，所以"遮挡"那一份是盖住窗口后手动停掉动画帧来模拟的。详见"关键取舍"。

`ci pr --record` 通过，摘要已提交。弹窗测量在用户确认的时段内完成，没有遇到锁屏。规模：`cargo xtask size` 为 +338 −11，停止线 850。

## 做了什么

**渲染进程的闸门钩子**（`apps/desktop/src/renderer/gate.ts`，只在 e2e 构建里，挂在 `window.__cupperGate`）
- 开始计数后，按流统计帧数和事件数，每 16 帧抽一帧量大小；
- 跟随窗口持有范围之外的流（压力报告用）；
- 经 T168 的 `getBlob` 下载 blob；
- 对灌好的会话以游标 0 订阅，宿主按重放发来，T080 的 `StreamStore` 加 `SessionView` 到达即应用，客户端照常确认。
- `flavors.test.ts` 把 `__cupperGate` 加进检查的名字，证明它不进其他构建。

**PERF-bridge-gate**（`tools/e2e/src/gate.spec.ts`，`nightly`）
- 负载：
  - 4 个假 agent 会话跑 `flood_rate 100 … 500`，窗口用界面自己的订阅依次打开，持有"当前会话 + 最近 3 个"，当前会话真的在渲染；
  - 第 5 s 下载 5 MB 的 blob（由测试先以 harness 身份上传）；
  - 第 10 s 重新同步 `test/seed` 灌入的 5,001 个事件；
  - 索引流：每 200 ms 用 `test/seed` 新建 3 个会话，得到每秒约 5 帧、每帧约 1 KB。
- 测量：经 `app.evaluate` 在主进程里开 `monitorEventLoopDelay({resolution: 1})` 和 CPU 计时，每段 20 s；流重置从客户端计数器读。
- 三条用例：
  - 可见；
  - 遮挡：另一个窗口盖住，并停掉页面的动画帧；
  - 压力：6 条流、每条每秒 200 个片段。
- 判定用例断言 p99 < 5 ms 且没有流重置；压力用例只记录、不断言。每次运行输出一行 `gate {...}`。
- 夹具的 `launch` 加了 `throttled` 选项，用于保留 Chromium 默认的节流行为。

**T077 的多一次复制**：任务目录下做了一次性基准（`copy.test.ts`、`copy.config.ts`，不进 CI）。

## 关键取舍

**5,000 个事件的重新同步**
- 由测量钩子以游标 0 订阅灌好的会话来模拟（你已认可）。
- 理由：宿主只对带游标的订阅重放事件（`store/streams/state.rs` 的 `subscribe_mode`），新订阅一律拿快照，而快照最多 20 个回合、2,000 个条目。`test/seed` 又只能新建会话，所以界面自己的订阅产生不出"重放 5,000 个事件"。
- 钩子先发一次普通订阅拿到 `epoch`，再以 `after_seq: 0` 订阅，宿主按重放发来全部 5,001 个事件，约 20 帧、每帧约 55 KB。

**遮挡**
- 在 Playwright 下，窗口被盖住、最小化、甚至 `hide()` 之后，页面的 `visibilityState` 仍是 `visible`，动画帧仍是每秒 120 帧。关掉 Playwright 的焦点模拟也一样，所以只是盖住窗口测不出任何不同。
- 对 store 来说，隐藏页面唯一要紧的变化是动画帧停止：设计 5.7.1 规定只有"通知组件重新渲染"合并到动画帧，应用和确认不依赖它。所以遮挡那一份在盖住窗口之外，把 `requestAnimationFrame` 换成不触发。实测停帧时应用和确认照常，没有积压，也没有重置。
- Chromium 自己判定的隐藏状态（计时器节流等）在 e2e 下复现不了，列在遗留问题。

**用真实界面持有 4 条会话流**：界面的订阅策略正好是"索引流 + 当前会话 + 最近 3 个"，所以没有在主进程打桩，也没有另做一个负载页面。只有压力报告多出来的 2 条流经钩子跟随。

**判定用例带断言**：它就是闸门本身。按实测余量（p99 约 1.6 ms，判定线 5 ms），在参考机器上每晚跑应当稳定。

## 验证方式

**判定报告**
- 环境：
  - 机器：Apple Silicon 笔记本，M5 Pro，18 核，64 GB；
  - macOS 26.4，Electron 44.9.0（Node 24.22），e2e 构建；
  - 经 `test:e2e:one` 持锁运行，`caffeinate -d`；
  - 每段测 20 s，三种情形各跑 3 次。
- 负载参数（实测）：

  | 流 | 设计 | 实测 |
  |---|---|---|
  | 每条会话流 | 25 帧/s × 约 2 KB | 20.2–20.7 帧/s × 2.55–2.69 KB（约 53 KB/s） |
  | 索引流 | 5 帧/s × 约 1 KB | 4.9–5.0 帧/s × 0.98–1.09 KB |

  会话流的帧数比设计少、每帧更大，是宿主 40 ms 写入合并的结果，字节数和设计相当。
- 结果：

  | 状态 | p50 / p99 / max（ms），3 次 | 流重置 | 主进程 CPU | 5 MB blob | 重新同步 5,001 事件 |
  |---|---|---|---|---|---|
  | 可见 | 1.16 / 1.60、1.63、1.60 / 54.6、34.4、53.0 | 0 | 约 4% 的一个核 | 146–174 ms | 151–156 ms |
  | 遮挡（停帧） | 1.16 / 1.65、1.54、1.54 / 4.5、47.5、12.8 | 0 | 约 4% | 49–117 ms | 31–36 ms |

- 原始数字在 `docs/tasks/T078-bridge-gate/measure.jsonl`，9 行，每次运行一行。

**压力报告**（6 条流 × 每秒 200 个片段，不参与判定）
- 每条流 21.9–22.6 帧/s，每帧 4.6–4.7 KB（约 103 KB/s）。
- p99 为 1.72、1.67、1.64 ms；max 为 53.4、2.4、2.1 ms；没有流重置。
- 渲染进程的动画帧从 120 降到 102–107 帧/s；重新同步要 560–575 ms。

**与 T076 预测量的对照**

| | 环境 | p99 | max |
|---|---|---|---|
| T076 预测量 | Node 单线程，MockHost，没有原生扩展和渲染进程 | 1.36–1.37 ms | 5.9–8.9 ms |
| 本次 | 真实宿主、原生扩展转发、`MessagePortMain`、真实渲染进程 | 1.54–1.65 ms | 2–55 ms |

p99 只高了约 0.2 ms。p50 在两边都是 1.16 ms，这是 `resolution: 1` 的底数。

**T077 的多一次复制**
- 判定负载每秒约 0.21 MB，按 8 KB 一次读入，`FrameDecoder` 的拼接和切帧约 0.09 ms。
- 5 MB 的 blob 一次处理完约 3.2 ms（中位数），最坏 8–12 ms。
- 真实运行里主进程只用了约 4% 的一个核，p99 离判定线很远，这次复制不需要处理。

**CI 与检查**
- 在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T078-bridge-gate/ci/`，一次通过，摘要 `pr-3883b7eba0a1.json` 已提交。跑的期间没有往工作区写文件。闸门用例是 `nightly`，不在 PR 的 `@smoke` 里。

  | 作业 | 结果 |
  |---|---|
  | rust | 2,067 通过 |
  | protocol | 185 通过 |
  | ts | 473 通过 |
  | license | 通过 |
  | testdata-scan | 3 通过（本机没有敏感词表，那一步跳过） |
  | ui | 按路径筛选跳过 |
  | e2e | 30 通过 |

- `flavors.test.ts`、ESLint、prettier、`tsc` 都通过。
- 每次运行后，本 worktree 都没有残留进程，`$TMPDIR/cupper-e2e/` 为空。
- 用 `git merge-tree` 试合并最新的 main，没有冲突。
- `PERF-bridge-gate` 已在 `ids.toml` 的 T078 段里，测试名以它开头。

**规模**：`cargo xtask size` 为 +338 −11（钩子 +91 −4，用例与夹具 +214 −7，停帧与索引调整 +33），开工前估计约 325。

## 提交列表

```
441d9ce docs(T078): record the ci pr summary
3883b7e docs(T078): record the measurement, the covered state and the copy's cost
d3cfbdd test(e2e): the bridge gate covers the window and stops its frames by hand
7dcb702 docs(T078): a one-off benchmark of the bridge's inbound copy
4a5ab3b test(e2e): PERF-bridge-gate, the main process under the judging load
e298a23 test(desktop): the bridge gate's hooks in the e2e renderer
cf0c54d docs(T078): record the checks, the approach and the size estimate
```

## 遗留问题 / 范围外发现

- **偶发的长延迟**：9 次运行里有 5 次的 max 在 34–55 ms，p99 不受影响，判定只看 p99。来源没有查：可能是 GC，也可能是 blob 那一下集中到达。如果以后要看尾部延迟，可以给主进程加时间线采样。
- **真正的隐藏状态没测到**：Playwright 下页面始终是 `visible`，Chromium 对隐藏页面的计时器节流在 e2e 里复现不了。要测，得在不连 DevTools 的情况下启动 App，例如另写一个只经主进程 inspector 测量的脚本。设计 5.7.1 的"窗口可见和被遮挡各测一次"，这次的遮挡是"盖住加停帧"。
- **判定负载的数字表述**：宿主 40 ms 合并后，会话流实际约 20 帧/s、每帧约 2.6 KB，而不是 25 帧/s × 2 KB，字节数相当。设计 5.7.1 可以改成按字节率表述。
