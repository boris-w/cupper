# T196 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`fix/T196-flaky-under-load`
- 完成日期：2026-10-26
- PR：https://github.com/boris-w/cupper/pull/5 。8 个检查全绿，运行是 https://github.com/boris-w/cupper/actions/runs/40340754324 （第 2 次运行，提交 7fdada4）。第 1 次运行时 `ts` 撞上了 T198 记下的 bridge `settle()` 偶发用例，只重跑了这一个作业。

## 做了什么

四处偶发失败，加上编排者裁定一并修的宿主副本首次执行问题，都按"先在高负载下复现、量出实际等待，再修"的顺序做完，没有加任何重试。数据、根因和修法都写在 `docs/tasks/T196-flaky-under-load/notes.md`。

负载的造法：后台跑 5–8 份 `cupper-host` 库测试二进制的死循环。pid 都记下了，测完先放停止文件，再用 SIGTERM 收掉。

**第 1 处：`unit_trust_scan_forking_directory_links_hit_the_entry_limit`（墙钟上限）**
- 复现：负载约 25 时，耗时在 0.65–6.06 s 之间。CI 那次用了 23.4 s，超过了 10 s 的上限。
- 根因：扫描走到第 20,001 个条目就立即停下，产品行为没有问题。10 s 的墙钟上限会被负载拉长。
- 修法：
  - `walk.rs` 加了 `walk_counting`，把走过的条目数交出来；原来的 `walk` 改为调用它，行为不变。
  - 用例改为断言恰好走了 `MAX_ENTRIES` 个条目，去掉墙钟上限。
- 改动：`walk.rs` 增 23 删 5，`tests.rs` 增 5 删 8。

**第 2 处：`it_lifecycle_host_exit_and_sigterm_exit_in_order`（"Sigterm: no host/shutdown"）**
- 根因：客户端每次读都要比对宿主的令牌。宿主退出之后，比对得到 `PeerExited`，这一次读到的字节就被丢掉。负载高时，读任务会被拖到宿主退出之后才执行。
- 第一版修法改了产品代码：让 `cupper-cli` 在 `PeerExited` 时照样交付读到的数据。review 指出这违反设计 5.8.1：每次读都比对令牌，防的正是 fork 出来、共用同一连接的进程在宿主退出后写入伪造数据。已用两个 revert 提交撤回，`cupper-cli` 和桌面中继的 diff 都是 0。
- 定稿选 (b)：只改用例。
  - 核对设计：5.2.13 只承诺 `host/shutdown` 在连接关闭前进入发送队列；5.8.1 要求宿主退出后读到的数据一律拒收；5.7.1 规定宿主断开时客户端走重连。所以客户端只看到"宿主已退出"也是设计允许的结果。
  - 用例照原来的顺序先读。读到的话必须是 `host/shutdown`，然后流结束；读不到也可以。两种情况下，关闭原因都必须是 `Eof` 或 `Refused(PeerExited)`。
  - `host/exit` 的回复被拒收也按同样的办法处理。这是连跑时 200 次中出现 1 次才发现的。
  - "宿主确实发了 `host/shutdown`"改由一条 testkit 原始连接断言：这条连接不比对令牌，在宿主退出之后读，与时序无关。
- 同类：升级用例收不到 `host/shutdown` 时会 panic，也照此修了。

**第 3 处：`it_cli_commands_read_elevate_write_and_answer`（第三条 `send` 得到 `queued`）**
- 复现：负载约 43 时，30 次里失败 4 次。
- 根因：用例的缺陷，宿主没有问题。agent 先回话，0.9–52 ms 之后回合才结束；用例在这个间隙里发了下一条，按规则就该排队。
- 修法：等到回话之后，再等 `turns` 表里没有 `running` 的回合。改动：增 4。

**第 4 处：fake-agent 的 `unit_replay_*`（"no response 1"、"the agent did not exit"）**
- 在最新 main 上（已含 T189）照原样连跑 200 次全过，说明 T189 的修复管不到这里。
- 找到条件：假 agent 刚链接好之后的第一次启动。
  - 每次先重新链接再跑，13 次里失败 4 次，报错和 T191 那次一模一样。
  - 负载 36–45 时，刚链接的二进制第一次启动要 3.8–11 s，之后只要几毫秒。期间子进程在睡眠，XProtect 服务在忙，是系统在扫描新文件。
- 修法：测试客户端在第一次 spawn 之前，先用 `--fault exit_on_start` 把假 agent 跑一次，等它退出，最多等 60 s。改动：增 30。

**同类：从宿主副本启动的两条用例（编排者裁定一并修）**
- 用例：升级用例，以及 IT-runtime 的 `it_runtime_startup_installs_from_the_registry_or_waits_for_an_upgrade`。
- 现象：67 MB 的宿主副本第一次启动要 12–29 s，而用例开头的 `connect` 只等 30 s。
- 修法一：宿主测试的 `common` 里加了 `first_start`，在计时开始前用 `--build-info` 把副本跑一次，最多等 120 s。
- 修法二：运行时用例只做修法一还不够。安装器会把 node 复制一份再执行，这份新文件也要先等扫描；量了 25 次，到 `ready` 要 5.4–44.3 s。这份副本由宿主在安装时才生成，用例没法提前跑。所以等 `ready` 的上限改成宿主自己给这一步的上限：`RUNTIME_DOWNLOAD` 加 `RUNTIME_BUNDLE` 再加 `PATIENCE`，共 210 s。
- 改动：`common/mod.rs` 增 24，`lifecycle.rs` 增 8 删 1。

**规模**：+171 −37（`cargo xtask size`），在停止线 260 之内。另有 `notes.md` 160 行。

## 关键取舍

- **第 1 处**：断言走过的条目数，不放宽墙钟上限。条目数不受负载影响，而且直接证明扫描在上限处停下了。
- **第 2 处**：
  - 不选 (a)（宿主退出前多等至多 1 s）。它只能缩小窗口，关不上：读任务排在宿主退出之后时，每次都还是收不到（实测 40 次里 40 次）。它还会给每次有序退出加一段等待，而设计并不要求客户端一定收到 `host/shutdown`。
  - 不保留"先等宿主退出再读"那种写法。那样客户端每次都只能看到宿主已退出，读到 `host/shutdown` 的那一支永远测不到。
- **第 3 处**：等回合结束，不等固定时长。回合的行和 `send` 的处置都由同一个会话 actor 写，所以只要读到回合已结束，下一条 `send` 一定会开始新回合。
- **第 4 处和宿主副本**：在计时开始前先把副本跑一次，不去放大用例的等待上限，让计时只算被测的东西。只有运行时安装那一处跑不了副本，才把上限对齐产品自己的上限，并不比产品更宽松。

## 验证方式

各用例在负载下的连跑结果（负载为 1 分钟平均负载）：

| 用例 | 次数 | 负载 | 结果 | 修前 |
|---|---|---|---|---|
| 第 1 处 trust | 200 | 中位数约 32，最高 52 | 全过，0.63–14.40 s | 负载约 25 时 0.65–6.06 s，CI 那次 23.4 s 超过 10 s 上限 |
| 第 2 处退出用例（定稿） | 200 | 20–49 | 全过，中位数 0.52 s | 原用例被拒收时报 "no host/shutdown" |
| 第 3 处 cli-commands | 200 + 200（revert 后重跑） | 41–68 / 34–57 | 全过 | 30 次里失败 4 次 |
| 第 4 处 replay（每次换上新复制的假 agent） | 200 | 29–58 | 全过 | 同样做法 30 次里失败 6 次 |
| 升级用例 | 200 | 25–67 | 全过 | 100 次里失败 11 次 |
| 运行时用例（4 路并行，各 50 次） | 200 | 25–67 | 全过 | 20 次里失败 8 次 |

- 第 2 处另跑了 200 次加打点的副本，看客户端实际收到什么：`HostExit` 那一轮 199 次读到 `host/shutdown`；`Sigterm` 那一轮 195 次读到，4 次只看到宿主已退出；另有 1 次 `host/exit` 的回复被拒收。后两种结果现在都被接受。
- 本机 `cargo xtask ci rust` 在 06fed4c 上全过（2185 passed）。
- 按新流程，没有跑本机带记录的 `ci pr`，以 PR 上的 GitHub CI 为准，8 个检查全绿。
- 测试拉起的进程：连跑时留下过 1 个宿主，按路径（本用例的临时目录）和启动时间核实后用 SIGTERM 收掉。现在没有残留进程，工作区干净。

## 提交列表

```
7fdada4 Merge branch 'main' into fix/T196-flaky-under-load
06fed4c docs(task): T196 notes on the host copies' first start
29c93e0 test(host): IT-runtime waits for the install as long as the host allows it
74e4e82 test(host): start a copy of the host once before a test times it
4dc8ef1 docs(task): T196 estimate for the host copies' first start
da215eb docs(task): T196 item 2 redone as a test fix, runs and findings
b1d75f9 test(host): a refused host/exit reply and the upgrade exit take the host gone
d904abd test(host): IT-lifecycle reads the exit in its own order again
c407500 Revert "fix(cli): read what a host sent before it exited"
afc0603 Revert "test(host): the upgrade exit's interrupt reply always comes now"
7035de3 test(host): IT-lifecycle takes either outcome a client may see on exit
789b5c0 Merge branch 'main' into fix/T196-flaky-under-load
9a63fc2 docs(task): T196 notes on the two added flaky tests
4972173 test(fake-agent): start the agent once before any test times it
728b6bf docs(task): T196 estimate for the two added flaky tests
84e0960 test(host): IT-cli-commands waits for a turn's end before the next send
eed9ab6 Merge branch 'main' into fix/T196-flaky-under-load
40054c0 docs(task): T196 the desktop relay has the same pattern
5fd2867 docs(task): T196 reproduction, causes and runs
2690469 test(host): the upgrade exit's interrupt reply always comes now
fd48d35 test(host): IT-lifecycle reads the host's last frames after it exited
4df61b6 fix(cli): read what a host sent before it exited
c588b06 test(host): the entry limit's test counts the entries, not the seconds
3df0e7e docs(task): estimate T196
```

（三个合并节点由编排者合入 main，其余是本任务的提交。）

## 遗留问题 / 范围外发现

1. **cargo 重新链接后，`cupper-host` 的第一次执行**也要等系统扫描。这一次会落在最先启动宿主的那个用例上，而它开头的 `connect` 只等 30 s。修法可以照旧：在测试里第一次启动宿主之前，先用 `--build-info` 跑一次。没有改，按编排者的安排明天归入 T199。
2. **`fake_agent_host_from` 开头的 `connect` 失败（panic）时，已经拉起的宿主没人回收**，会一直留到它自己空闲退出。建议在 `connect` 之前就把它包进 `Running`。没有改。
3. **宿主的 `VERSION_PROBE` 只有 5 s**。CLI 更新后的第一次 `--version` 也会被系统扫描，这次在负载 35–50 下量到首次执行要 4–29 s，所以机器忙时可能超时。这只是推断，没有用真实 CLI 量过。
4. **桌面中继 `relay.rs`**：之前判断它有同类缺陷，这个判断撤回。它的做法正是 5.8.1 的要求，不要改。
5. **过程上的偏差**：
   - 第 3 处的估计晚于它的提交才补写。
   - 第 2 处的第一版放宽了安全规则，已撤回，并在 `notes.md` 里记录了原因。
