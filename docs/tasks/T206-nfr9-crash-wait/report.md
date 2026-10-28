# T206 汇报

- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T206-nfr9-crash-wait`
- 完成日期：2026-10-28
- PR：https://github.com/boris-w/cupper/pull/22
- CI：https://github.com/boris-w/cupper/actions/runs/40661950733 。b7c2ba4 上第 2 次运行 8 个检查全过；第 1 次运行 rust 作业撞上一条与本任务无关的偶发失败，见"范围外发现"第 2 条。

## 做了什么

**1. 查明慢在哪里**（数据都在 `notes.md`）

- **runner 的失败日志**（PR #21，run 40649145948 第 1 次运行，构件 `rust-macos-15-logs`）：
  - `crash_concurrency` 二进制跑了 84.71 s，第 3 轮报 "no load in round 3"。
  - 第 3 轮用满了 30 s，所以前 3 轮平均每轮约 18 s；本机约 9.5 s。
  - 日志里看不出那一轮等到了多少条。
- **本机单份计时**（临时加计时的副本跑了 5 次，未提交）：

| 阶段 | 用时 |
|---|---|
| 连上 | 4–126 ms |
| `session/send` 的回复 | 145–355 ms |
| 回复之后到第一条 delta | 44–64 ms |
| `kill -9` 后重启 | 23–113 ms |
| **第 1 条到第 200 条 delta** | **9.2–12.6 s** |

  - 每条 delta 只有 8.2–9.6 KB，没到 16 KB 的按大小刷出，所以全是 40 ms 窗口到期时写的；宿主 `ts` 里平均每条约 47 ms。
  - 宿主读线路的速度（约 1.9 万行/s）超过会话 actor 的处理速度，agent 的输出一直有富余。
  - 所以慢的不是宿主重启、K6 resume 或假 agent，而是 coalescer 的 200 个窗口，下限 8 s。
- **runner 上修前的连跑**：用探针 5aceb58。判定和 main 一样，但超时的那一轮会等完，并记下每轮的计时。

| | A：只跑本用例，30×3，run 40653537487 | B：整个二进制，25×2，run 40653540866 |
|---|---|---|
| 结果 | **90 次里失败 8 次（8.9%）** | 50/50 通过 |
| 200 条 delta 的用时 | 16.0–34.5 s，中位数 22.5 s | 14.1–24.3 s |
| 每条 delta 的间隔 | 中位数 112 ms | 中位数 84 ms |
| 一轮里最长的停顿 | 0.7–12.4 s | 0.3–7.6 s |
| 连上 / send / 第一条 delta / 重启 | 都在 0.7 s 以内 | 同 |

  - 8 次失败都用了 30.1–34.5 s，最后都等到了 200 条，没有一次停住。
  - 原因是两层叠加：每个窗口被拉长到约 2 倍，每轮还有一两次几秒的停顿。
  - 688 轮里，测试看到的最长静默和宿主 `ts` 里的最长间隔每一轮都对得上，所以停顿在宿主那边。

**2. 修复**（9919eb3，`crash_concurrency.rs` +20 −3）

- 每轮仍在第 200 条新 delta 时 `kill -9`。负载不变，重启后的检查也不变。
- `PATIENCE` 从限制"等完 200 条"改为限制"等下一条"：宿主只要还在写，就能等完；流停住 30 s 才失败。
- 失败信息带上轮次和已经等到的条数，例如 "round 0: no new delta for 30s after 6 of 200"。
- 文档注释写明了每轮为什么要 200 条。

**3. 探针**：5aceb58 已在 28f4e89 revert，最终 diff 里没有它。

## 关键取舍

- **PATIENCE 的用法**：套件里 `PATIENCE` 限的都是宿主的一次反应。原来的写法把它用在了 200 个窗口的总和上，这个总和随负载线性变长。现在只限两条 delta 之间的静默；最重的 A 里，最长的静默 12.4 s。
- **每轮的负载保留 200 条，不减**：
  1. 200 条最多 3.2 MB，也就是 16 万条片段，只占 20 万条的一部分，所以无论负载多高，kill 都落在流的中途。
  2. 本机实测，`host.db` 的 WAL 每轮做一两次检查点（读 WAL 文件头里的检查点序号）；每轮只要 20 条的话，五轮加起来最多一次检查点。
  3. 客户端每轮实时收到约 200 个事件，前缀检查才比得深。
  4. kill 避开了开回合那一刻；那里的写入点已由故障点用例覆盖。
- **屏障**：仍是被断言的那条会话流，符合 T205 加进 5.9 的规则。
- **没采用的做法**：
  - 手动时钟：每次重启都从同一个墙钟时间开始，kill 之后 `ts` 和 UUIDv7 会往回走；delta 只能按 16 KB 刷出，反而更慢；5.2.16 规定等外部进程时不用 `clock_advance`。
  - 减少条数：会削弱上面的第 2、3 条。
  - 改用大片段：负载的形状变了。
  - 覆盖 `item_coalesce_ms`：改的是产品的节奏。

## 验证方式

**本机**（只跑单份）：
- fmt、clippy（test-harness 的 tests）都干净。
- 本用例单份通过，49.7 s；整个 `crash_concurrency` 二进制 9/9 通过，54.9 s。
- 反例核对：把剧本临时改成只有一次 flood。修后的用例在 30 s 后报 "round 0: no new delta for 30s after 6 of 200"，没有残留进程。

**对照验收标准**：
1. **慢的原因有证据**：见上。
2. **修后 runner 上 200 次以上全过**：在 b7c2ba4 上按 A 的参数（只跑本用例，`times=35`、`parallel=3`）连跑两组，**共 210 次，全部通过**。
   - run 40661947765：Run 这一步 63.6 分钟。`results.txt` 共 105 行，105 行 ok，每份 35 行 ok，summary 为 "105 passed, 0 failed"。
   - run 40661959516：66.0 分钟。`results.txt` 同样是 105 行 ok、每份 35，"105 passed, 0 failed"。
   - 两组的 `binaries.txt` 都只有 `crash_concurrency` 一个二进制。
   - 修前同样的参数是 90 次里失败 8 次。
   - NFR-9 的验证没有削弱：
     - kill 时机：第 200 条新 delta，没变；
     - 负载：20 万条片段、全速输出，没变；
     - 每次重启后的检查：完整性、每条流的 seq 连续、客户端实时看到的是日志的前缀，没变；
     - 收尾：会话还能完成一条 `say done`，没变；
     - 改动的只有等待的上限。
3. **PR 上 8 个检查全过**：run 40661950733 第 2 次运行全过（rust 作业 18m16s）。

**规模**：`cargo xtask size` 报 +20 −3，停止线 150。

## 提交列表

```
b7c2ba4 docs(task): T206 notes on where the rounds' time goes, the fix and the load
9919eb3 test(host): IT-NFR9-crash-under-load gives each next delta 30 s, not all 200
28f4e89 Revert "test(host): a probe times IT-NFR9-crash-under-load's rounds"
5aceb58 test(host): a probe times IT-NFR9-crash-under-load's rounds
54c57f4 docs(task): estimate T206
```

## 遗留问题 / 范围外发现

1. **负载下宿主的流有几秒的停顿**（没有展开查）
   - 现象：修前的 A（负载中位数 25）里，每轮相邻两条 delta 之间最长的间隔是 0.7–12.4 s，中位数 1.9 s。超过 5 s 的有 42 轮，超过 10 s 的有 4 轮，没有超过 20 s 的。B 里最长 7.6 s；本机多是 60–224 ms，只有一次约 3 s。
   - 时间点：探针按宿主写下的 `ts` 算的是相邻两条 delta 之间最长的间隔，只记了长度，没记它落在一轮的什么位置。超过 10 s 的 4 轮（run 40653537487 的 t206-probe.txt，按测试进程号区分）：

| 进程号 | 轮次 | 最长间隔 | 这一轮的跨度 | 负载 | 备注 |
|---|---|---|---|---|---|
| 15836 | 2 | 12.45 s | 27.4 s | 21–24 | |
| 77108 | 1 | 11.11 s | 30.1 s | | 失败的一轮 |
| 79425 | 0 | 10.14 s | 23.2 s | | |
| 94065 | 3 | 10.97 s | 34.4 s | | 失败的一轮 |

   - 要知道停顿落在哪里，需要另做一个记下每段间隔起点的探针。
   - 推断，没有验证：store writer 的提交方都要等 `Receipt`；`host.db` 每轮做一两次带 `checkpoint_fullfsync` 的检查点，在 runner 的虚拟盘上可能很慢，这段时间会话 actor 写不出新的 delta。
   - 这是负载下的流延迟，不是崩溃恢复的问题。
2. **PR #22 第 1 次运行的偶发失败，与本任务无关**：`cupper-desktop-native` 的 `relay::tests::unit_native_relay_forwards_both_ways_until_the_host_closes`。
   - 现象：`relay.rs:182` 的 `read_to_end` 等满 10 s 的读超时，报 `WouldBlock`（os error 35），也就是 relay 在 10 s 内没有把 EOF 交过来。
   - 出处：rust 作业 122726884499 的 `test-native-release` 步骤。本任务没有改这个 crate；重跑后通过。没有展开查，建议另记。（编排者注：已在计划里立为 T207。）
