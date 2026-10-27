# T199 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`fix/T199-timing-tests`
- 完成日期：2026-10-27
- PR：https://github.com/boris-w/cupper/pull/11
- CI：https://github.com/boris-w/cupper/actions/runs/40457825691 （a28f9b2，8 个检查全绿）

## 做了什么

**负载复现被中止**：开工时照 T196 的办法在本机造负载，加上别的任务，系统负载到了 100 左右，用户的电脑没法用。编排者停掉了循环，我按 pid 收掉了残留。之后照守则第 10 条，本机只跑单份测试；负载下的连跑放到 runner 上，用 `test-repeat` 或 `e2e-repeat`。

**第 1 处 `it_gw_sync_reconnect_replays_without_gaps`（"no range events live"）**
- 根因在用例的时序，合并窗口本身没变。发送端的 200 ms 窗口从回合的第一条事件开始计时。agent 的第一段输出要是来得晚，落在窗口末尾，第一段和其余部分就分在两批里，凑不成 range。
- 复现：负载 28–110 下打点跑了 150 次，有 15 次只有 1 个 range，离失败只差一个。不加负载时让 agent 先 sleep 150 ms 再 flood，5 次里失败 4 次。
- 修法：用例改用手动时钟；writer 看到回合结束后再推进 200 ms，让窗口结束。
- 验证：同样几种 sleep 一共 30 次全过；runner 上 2 份并行加压跑 200 次全过（run 40445262092 的 results.txt）。

**第 2 处 E2E-NFR1-4（窗口断线后重连超时）：已查明是产品问题，转交 T200**
- runner 上 550 次里失败 5 次。两次采样（run 40453156576 的 repeat72 和 repeat104）显示，Electron 主线程停在原生模态框 `NSAlert runModal` 里，一直不动。
- 主进程没有 `uncaughtException` 和 `unhandledRejection` 的处理，Electron 遇到未捕获异常会默认弹出同步错误框。e2e 的对话框替身没有替换这个错误框，所以它真的弹了出来，挡住了主线程。
- 抛出点在 socket 关闭之后、`connectHost` 之前。读码没找到具体是哪一处。用户那边也会碰到：弹出错误框，App 卡住。
- T199 只加了诊断，没改产品：
  - fixture 在用例失败时先附日志、再退出 App；
  - `disconnectWindow` 里的 `disconnected` 注解、5 s 的探测和 `sample` 采样；
  - 主进程的建连日志，只记耗时和原因码，留着方便排查"窗口连不上"。
- 证据、栈、日志出处和建议都在 `notes.md` 的"交接给 T200"一节。

**第 3 处 `it_nfr9_crash_under_load_kill9_again_and_again`（"no load in round 4"）**
- 不是首次执行扫描：kill -9 后宿主 0.2–0.6 s 就重新连上，适配器 0.3 s 内起来。
- 根因：假 agent 单线程跑脚本，flood 时不让出，要把 20 万条全部排进队列才写出第一条。这段时间负载 5 时 1.8 s，负载约 100 时 15–28 s，全算进了 30 s 的等待。之后 200 条 delta 按 coalescer 40 ms 的节奏出来，还要至少 8 s。
- 修法：仍是每轮 20 万条，改为每次 flood 2,000 条、中间 sleep 1 ms。
- 验证：runner 上 100 次全过（run 40445328766 的 results.txt；只跑 100 次是你定的，原因已写进 notes）。
- 用例要验的东西没有削弱：kill 的时机和恢复后的检查都没变。

**第 4 处：cargo 重新链接后 `cupper-host` 的第一次执行**
- 修法：`common::start` 在第一次启动某个程序前，先用 `--build-info` 把它跑一次，每个路径只跑一次；`gateway.rs` 的孤立启动用例同样处理；T196 加的两处显式调用随之去掉。
- 核对：每次先重新链接再跑。修前第一次 connect 要 1.14–1.16 s；修后 first_start 用 1.06–1.13 s，接着的 connect 只要 47–69 ms。

**第 5 处 `fake_agent_host_from` 在 connect 失败时回收宿主**
- 修法：改为返回 `Running`，调用处跟着改。
- 核对：用一个从不监听的桩程序，修前留下残留进程，修后没有。

**补充：`test-repeat.yml`**
- cargo 或 Vitest 的单个用例重复 N 次，可选几份并行加压。参数：kind、package、args、filter、times、parallel、runner。
- 防"假通过"：单次运行限 20 分钟；过滤条件一个用例都没匹配上也算失败。
- 原版有一个错：runner 的 `bash -e` 让全部通过时也被判成失败。8216080 修了（经 PR #10）。

## 关键取舍

- 第 1 处改用手动时钟，用例不再依赖真实时间；只放宽等待修不好这个问题。
- 第 3 处保留"20 万条、200 条 delta 后 kill"，只去掉 agent 排队那一段对等待的影响。
- 第 2 处按你的决定拆给 T200，T199 不改产品代码，只留诊断。
- 遵守第 10 条：负载下的验证都在 runner 上做。

## 验证方式

- runner：第 1 处 200/200 通过，第 3 处 100/100 通过。第 2 处修前跑了 50、100、200、200 次，用来定位。
- 本机：cargo fmt；clippy（test-harness，以及 test-harness+dev-auth）；lifecycle、gateway、gateway_sync、driver_k、crash 各单份跑；prettier、eslint、tsc --build、desktop 单测；actionlint。
- PR #11：8 个检查全绿。
- 规模：合并 main 后 +126 −59（`test-repeat` 已经由 PR #9 进了 main），在停止线 330 之内。

## 提交列表

```
a28f9b2 Merge branch 'main' into fix/T199-timing-tests
fd789ba docs(task): T199 hands item 2 to T200 with the samples and logs
a252264 docs(task): T199 notes on the runs on the runner
fcde9c1 test(e2e): bound the dropped-window probes and sample the main process
5b0a306 test(e2e): a failed test's logs are attached before the app quits
8216080 fix(ci): test-repeat's summary survives the runner's bash -e
69ecd1a docs(task): T199 notes on item 2 so far and the runs on the runner
83feb2a test(e2e): a window that does not reconnect leaves what it showed and the logs
7d0fef5 docs(task): T199 notes on items 1, 3, 4 and 5 and test-repeat
ddfb4fb test(host): IT-NFR9-crash-under-load floods 2,000 fragments at a time
8f684fe test(host): a freshly linked host starts once before the first test times it
16932c1 ci: test-repeat runs one cargo or Vitest test N times, loaded if asked
9b8e118 Merge branch 'main' into fix/T199-timing-tests
2fccdb3 test(host): IT-gw-sync's live ranges come in a window the test ends
d60812c test(host): the fake-agent host is stopped even when its connect fails
2f4ce8c docs(task): estimate T199
```

## 遗留问题 / 范围外发现

1. 第 2 处的产品修复交 T200：主进程断开后有一处异常没被处理，被 Electron 默认的同步错误框挡住了主线程。
2. 本机用 `parallel=2` 测脚本时，`unit_replay_each_start_plays_the_next_segment` 在 22 次里失败过 1 次，日志已经丢了，之后 38 次没再出现。没有展开查。
3. 宿主的 `VERSION_PROBE` 只有 5 s：CLI 更新后的第一次执行也要等扫描，机器忙时这一次有可能超时。这一条是从 T196 带过来的推断，没有量过。
