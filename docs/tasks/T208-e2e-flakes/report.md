# T208 汇报

- 执行者：子 agent（测试工程师）
- 分支：`fix/T208-e2e-flakes`
- 完成日期：2026-11-02
- PR：https://github.com/boris-w/cupper/pull/31
- CI：https://github.com/boris-w/cupper/actions/runs/41387182805 （be46b0b，8 个检查全部通过）
- 最后一个提交 b99ee65 只在 notes.md 里补了确认运行的结果，已请你推送。

## 做了什么

**第 1 处：`E2E-quit-confirm: idle …`，根因在测试，已修测试**

- **退出码 3 的含义**：`HostGone`，即宿主不在了。CLI 带 `--json` 时把错误打印在 stdout 上，所以 CI 的失败信息里 stderr 是空的。
- **根因**：
  - e2e 构建开了 `dev-auth`，宿主的父进程是 App 的主进程。
  - App 退出后，宿主在下一次 `supervisor_observe`（1 s）时发现自己成了孤儿。这时没有运行中的回合，它就按 5.8.1 有序退出。
  - 测试在 App 关掉后立刻调 `cupper status`，和宿主的这次退出抢时间。
  - 10 分钟的空闲规则只适用于发布构建。T007 按空闲规则写了这个用例。
- **本机证据**：
  - `close` 之后马上调 `status`：退出码 0。
  - 等 1.5 s 再调：退出码 3，输出 `host_gone`，和 CI 的失败一样。
  - host.log 里，`close` 之后约 0.6 s 出现 `orderly exit {exit: Orphaned}`；审计里记着 `host.exiting {reason: orphaned}`。
- **修法**（541d1cb）：
  - 保留"不弹确认框"这一半。
  - 改为在 15 s 上限内等宿主自己退出，再读宿主的审计：`host.exiting` 必须正好是 `[{reason: "orphaned", force: false}]`，由此证明 App 没有发 `host/exit`。反向核对过：App 发了 `host/exit` 时，审计记的是 `requested`。
  - 去掉 status 和 pid 的检查。
  - 标题改为 "…leaves the host to its own exit rule"，编号不变。
  - 注释里写明，发布构建那一半由 IT-idle-exit、UNIT-orphan-policy 和 SMOKE-SIGNED-reattach（T181）覆盖。
- **顺带**（f425ae7）：`Cli.ok()` 失败时，失败信息带上 stdout 里的 JSON 错误。

**第 2 处：E2E-NFR1-4 "after a reload, earlier history …"，根因在产品，已修产品**

- **怎么抓到的**：
  - 不加负载时 200 次全过。
  - 给 `e2e-repeat` 加了 `workers` 输入（9733858，常驻）。
  - 带探针（228eb5e，已 revert）、两份并行跑 300 次，失败 11 次，3.7%。
- **失败的样子**：
  - 11 次失败里，列表都停在底部，页面快照显示的是 t17–t23。
  - 8 次失败在 `say t4` 不可见，2 次在 `say t4` 的 `boundingBox`，1 次在点击时按钮脱离 DOM（和 CI 那次一样）。
  - main.log 和 host.log 都干净：没有重连，没有 renderer gone。
  - 逐帧记录里，任何一帧看到的列表都在底部，但测试在帧与帧之间看到过顶部的那几行。
- **根因**：
  - `ScrollKeeper.stuck` 只在 React 的 `onScroll` 里更新。
  - TanStack Virtual 3.17.11 在最后一个滚动事件之后 150 ms，用一个定时器从 DOM 读取偏移量并重新渲染。定时器是任务，可以插在两帧之间。
  - 测试在两帧之间设了 `scrollTop = 0`。定时器先于下一帧的滚动事件触发时，虚拟列表按 0 渲染出顶部的行，但 `stuck` 还是真，`Rows` 的 layout effect 就把 `scrollTop` 写回底部。
  - 下一帧的滚动事件看到列表在底部，`stuck` 保持为真。从此列表停在底部，历史按钮不会再回来。
- **影响面**：用户用滚轮或键盘滚动时，偏移量和滚动事件在同一帧里，碰不上。在列表还在稳定的时候由脚本滚动（例如页内查找、辅助功能），会碰上。
- **修法**（8525a89，`packages/ui-core/src/stream`）：
  - keeper 记下列表最后一次被看到滚动、或被自己放置时的位置。"自己放置"包括 layout effect 的写入，以及经包了 `elementScroll` 的 `scrollToFn` 做的写入，也就是虚拟列表在测量时的修正。
  - layout effect 把列表放回底部之前，先看列表在不在记下的位置。偏离超过 1 px，就当作一次事件还没到的滚动，先重新判断 `stuck`。
- **单测**（UNIT-stream-anchor）：
  - 新增一例，经 TanStack 自己的定时器复现问题。修前失败（`scrollTop` 是 1000，应为 0），修后通过。
  - 新增"在底部时跟随新内容"一例，防止修法把跟随弄坏。
  - 新增 keeper 的纯函数一例。

## 关键取舍

- **第 1 处修测试，不修产品**：dev-auth 构建的宿主按设计就该这样退出，你也已确认。
- **第 2 处修产品，不修测试**：也可以让测试等列表稳定后再滚动，或者改用滚轮滚动，但那样只是把竞争藏起来。layout effect 不应在滚动事件到达之前把列表拽回底部（你已确认）。
- **探针用提交和 revert 的方式**：最终 diff 里没有探针。逐帧记录和失败附件的结论都写进了 notes.md。

## 验证方式

- **runner 上的连跑（e2e-repeat）**：

  | 用例 | 运行 | 提交 | 同时几份 | 结果 |
  |---|---|---|---|---|
  | idle | [41002041448](https://github.com/boris-w/cupper/actions/runs/41002041448) | 修前 7771c71 | 1 | 200/200 |
  | idle | [41003132122](https://github.com/boris-w/cupper/actions/runs/41003132122) | 修后 541d1cb | 1 | 200/200 |
  | idle | [41007332700](https://github.com/boris-w/cupper/actions/runs/41007332700) | 修后 228eb5e | 2 | 200/200 |
  | NFR1-4 | [41002049698](https://github.com/boris-w/cupper/actions/runs/41002049698) | 修前 7771c71 | 1 | 200/200 |
  | NFR1-4 | [41007324304](https://github.com/boris-w/cupper/actions/runs/41007324304) | 修前（带探针）228eb5e | 2 | **289/300，失败 11 次** |
  | NFR1-4 | [41378847141](https://github.com/boris-w/cupper/actions/runs/41378847141) | 修后 32a6f20 | 2 | **300/300** |
  | NFR1-4 | [41387190303](https://github.com/boris-w/cupper/actions/runs/41387190303) | 修后 be46b0b | 2 | **100/100** |

- **修前失败率**：两个用例单份跑都量到 0/200。第 1 处的问题由本机的确定性复现证实；第 2 处在两份并行时是 11/300。
- **用例时长**：
  - 41378847141 中途出现台阶：从 03:10:30 起，每个用例 37–45 s，两份同时开始、同时结束。
  - 本机单份对比，修前修后的耗时和 CPU 时间一样，修法没有多做事。
  - 你让重跑的 41387190303：中位数 17.4 s，每 25 个一组的中位数在 16.3–18.3 s，没有台阶。所以那次是 runner 的问题。
- **本机检查**：
  - ui-core 的 vitest 85/85 通过；eslint、prettier 和 `tsc` 都干净；`tools/e2e` 的 `tsc` 也干净。
  - 两个用例单份跑都通过。
  - 修前在 jsdom 里能确定地复现失败。
- **对照验收标准**：
  1. 两处根因都有证据：本机复现、审计与日志、runner 上的逐帧记录和页面快照。详见 notes.md。
  2. 修后两个用例各通过 100 次以上：idle 400 次（单份 200、两份并行 200），NFR1-4 两份并行 400 次，全部通过。
  3. PR #31 的 8 个检查全部通过（run 41387182805）。
- **规模**：`cargo xtask size` 报 +105 −15，停止线 150。

## 提交列表

```
b99ee65 docs(task): T208 notes on the repeat that puts the durations down to the runner
be46b0b docs(task): T208 notes on item 2's runs after the fix
32a6f20 docs(task): T208 notes on item 2's cause, the probe's runs and the fix
8525a89 fix(ui-core): a scroll is not undone by a render that comes before its event
82908ce Revert "test(e2e): a probe records E2E-NFR1-4's list frame by frame after the reload"
b60d922 docs(task): T208 notes on the runs so far and where item 2 stands
228eb5e test(e2e): a probe records E2E-NFR1-4's list frame by frame after the reload
9733858 ci: e2e-repeat can run its copies two at a time, as `xtask ci e2e` does
541d1cb test(e2e): E2E-quit-confirm idle checks why the host exited, not that it stays
f425ae7 test(e2e): a CLI command that must succeed says the error it printed
8bb075f docs(task): T208 estimate and item 1's cause, the orphaned e2e host
```

## 过程中的疏漏

- 修好第 2 处、重新构建之后，我在本机把整个 `src/nfr1.spec.ts` 跑了一遍（单份），没有排除 `@focus`。结果有 3 个 `@focus` 用例在本机跑了约 11 s，违反守则第 10 条和任务书的注意事项。之后都加了 `--grep-invert @focus`。

## 遗留问题 / 范围外发现

1. **CI 失败时留不下 e2e 的附件**：`xtask ci e2e` 的 focus 那一轮和前一轮共用 `dist/test-results`，Playwright 每轮开始时会把它清空。两次 CI 失败都只剩 JSON 报告。你已说另开任务。
2. **别的 e2e 用例可能也会和孤儿退出抢时间**：e2e 构建里，凡是在 App 退出之后还去连宿主的用例都可能撞上。`app.spec.ts` 的另外两个退出用例在退出时都有运行中的回合，宿主不会因为成了孤儿而退出，没有这个问题。别的 spec 没有核查。
3. **T214（PR #30）也改了 `app.spec.ts` 的 E2E-quit-confirm 段**：冲突由你处理。
4. **e2e 的脚本滚动还有一处时序上的小坑**：第 2 处修好后，测试里两帧之间的 `scrollTop = 0` 不再被撤回。不过列表上的"回到底部"按钮仍要等滚动事件到达后才出现。这是预期行为，记在这里备查。
