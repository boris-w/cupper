# T199 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前的规模估计

第 1–3 处的根因要先复现才知道，估计给区间。

| 处 | 估计 |
|---|---|
| 第 1 处 `it_gw_sync_reconnect_replays_without_gaps`：若是用例的时序，改用例约 15；若是合并窗口在负载下真的变了，产品修法约 60 | 15–60 |
| 第 2 处 E2E-NFR1-4 的重连超时：看重连的退避和轮询，改用例或重连约 10–40 | 10–40 |
| 第 3 处 `it_nfr9_crash_under_load_kill9_again_and_again`：看是首次执行扫描、恢复的等待，还是负载下真的慢，约 10–40 | 10–40 |
| 第 4 处 cargo 构建的 `cupper-host` 第一次执行：`common` 里启动宿主的入口按程序路径先跑一次（`first_start` 改为同步、按路径只跑一次），T196 那两处显式调用随之去掉 | 30 |
| 第 5 处 `fake_agent_host_from` 返回 `Running`，调用处跟着改 | 25 |
| **合计** | **约 90–195** |

在停止线 260 之内。复现与测量用的脚本是一次性的，不提交。

补充（2026-10-27）：新增 `test-repeat.yml`，停止线改为 330，硬停止线 380。

| 处 | 估计 |
|---|---|
| `test-repeat.yml`（cargo 或 Vitest，重复 N 次，可选几份并行加压） | 120–140 |

## 负载复现被中止

开工后，先照 T196 的办法造了负载：后台跑 3–8 份 `cupper-host` 库测试的死循环。加上别的任务的负载，系统负载到了 100 左右，用户的电脑卡得没法用。编排者用 SIGTERM 停掉了这些循环，我也按 pid 核对后，收掉了剩下的两份崩溃压力用例和它们拉起的宿主。之后改了规则（`CLAUDE.md` 守则第 10 条）：本机不再造高负载，负载下的连跑改到 runner 上，用 `test-repeat`、`e2e-repeat`。

下面"负载下"的数据都是中止前拿到的，负载里有别的任务的份。之后的测量都是本机单份、不加负载（负载约 4–6）。修后在负载下各跑 200 次的验证，在 runner 上用 `test-repeat` 做；修前的失败，以 CI 上已经出现过的那几次为证据。

## 第 1 处：`it_gw_sync_reconnect_replays_without_gaps`（"no range events live"）

- **CI 上的现场**：PR #3 的 rust 作业，run 40316625556 的第 1 次运行（job 121550735981）。`gateway_sync` 的 12 个用例里只有这一个失败，报 "no range events live"；第 2 次运行通过。
- **负载下（中止前）**：
  - 原用例在负载 62–153 下跑了 40 次，失败 2 次。两次都是 `Peer::until` 等满 30 s 的 "not done in time"，没有出现 range 的断言失败。
  - 加打点的副本在负载 28–110 下跑了 150 次，全过。打点记下 windowed 客户端收到的每一批事件。
  - 每次运行里，实时到达的 range 个数分布在 1–29。其中 15 次只有 1 个，离失败只差一个。
- **机制**：
  - windowed 客户端设了 `coalesce_ms: 200`。发送端取到第一条实时事件后，再等 200 ms，然后把这期间攒下的一起发出。相邻的 delta 只有落在同一批里，才会合并成 range。
  - 宿主的 coalescer 把 agent 的第一段立即写成一条 delta，其余部分最多等 40 ms 再写一条。
  - 窗口从回合的第一条事件（`turn.started`）开始计时。agent 的第一段要是来得晚，落在窗口末尾，那么"第一段"进了这一批，"其余"进了下一批，单独发出。第二次 flood 前隔了 300 ms，也是单独一批。结果一个 range 都没有。
- **不加负载复现**：用一个副本把脚本换成"先 sleep d ms，再 flood"，各跑 5 次。
  - d = 0：5 次全过；d = 100：5 次全过；d = 150：失败 4 次；d = 170：失败 2 次；d = 190：5 次全过。
  - 失败只出现在第一段恰好落在窗口末尾的时候。这说明是用例依赖时序，不是合并窗口在负载下变了：每批之间的间隔都在 205–230 ms。
- **修法**（只改用例）：
  - 用例改用手动时钟（`"clock": "manual"`）。发送端的 200 ms 窗口和 coalescer 的 40 ms 都走这个时钟。
  - writer 客户端（没有窗口）看到第 3 回合结束后，`advance` 200 ms。这时窗口才结束，整个回合的事件都在同一批里，相邻的 delta 一定合并。
  - 前半段的断线重放不依赖定时器，照旧。
- **修后（本机单份）**：照原样跑 3 次全过，0.8–1.9 s。同一个副本、同样的 d = 0、100、150、170、190、400，各 5 次，共 30 次全过。
- **改动**：`gateway_sync.rs` 增 7 删 3。负载下 200 次的验证在 runner 上做。

## 第 3 处：`it_nfr9_crash_under_load_kill9_again_and_again`（"no load in round 4"）

- **CI 上的现场**：PR #4，run 40322065826。宿主重启后，在 `PATIENCE`（30 s）内没等到 200 条新 delta。
- **先查首次执行扫描**：不是。加计时的副本显示：
  - 每轮 `kill -9` 后，重启宿主到连上要 0.2–0.6 s；
  - 适配器在发送后 0.3 s 内就起来，回合在 0.2 s 内开始；
  - 五轮一样慢，不只是重启后的那几轮。
- **量（中止前，负载约 100–110）**：
  - 从发送到第一条 delta 要 15–28 s，到第 200 条要 28–41 s。三次完整运行里的大多数轮次，都超过了 30 s 的上限，只是副本把上限放到了 300 s 才看得到。
  - 不加负载（负载约 5）时，第一条 1.9–2.2 s，第 200 条约 11 s。
- **第一条为什么这么晚**：
  - 从 raw_frames 看，宿主读到第一段 flood 的时间，就是第一条 delta 的时间（n = 200,000 时分别是 1,856 ms 和 1,868 ms）。所以晚在宿主收到之前。
  - 单独驱动假 agent，第一段 flood 的输出要等：n = 2,000 时 19 ms，20,000 时 186 ms，200,000 时 1,837 ms，和条数成正比，每条约 9 µs。
  - 原因：假 agent 在单线程上跑脚本，flood 一条接一条地 `send_notification`，中间不让出。所以写出线程直到 20 万条都排进队列，才写出第一条。负载高时，这一段 CPU 密集的排队就从 1.8 s 拉长到 15–28 s。
- **之后的 200 条 delta**：输入没有填满 16 KB 时，coalescer 按 40 ms 的节奏写 delta。所以 200 条至少要 8 s 的真实时间。不加负载时实测约 9 s，负载约 100 时 12.5–13.5 s。
- **根因**：用例的写法。30 s 的等待把假 agent 的排队时间也算了进去，再加上 delta 节奏本身至少 8 s，负载一高就超时。宿主没有问题，也和扫描无关。
- **修法**（只改用例）：
  - 同样是 20 万条，改为每次 flood 2,000 条，中间 `sleep 1`（1 ms），共 100 次。
  - 每次 flood 只排 2,000 条（不加负载时约 18 ms），sleep 时写出线程就能把它们写出去。agent 仍然是尽快地写，整轮都在写。
  - 等待和 200 条的门槛都不变。
- **修后（本机单份，负载约 4–6）**：第一条 delta 在 31–319 ms，第 200 条在 9.4–9.8 s（修前 11 s）；整个用例 49–50 s（修前 58 s）。负载下 200 次在 runner 上做。
- **改动**：`crash_concurrency.rs` 增 6 删 3。

## 第 4 处：cargo 重新链接后 `cupper-host` 的第一次执行

- **修法**：
  - `common` 的 `first_start` 改为同步，按程序路径记下已经跑过的，每个路径只跑一次。它用 `--build-info` 把程序跑到退出，最多等 120 s；超时就趁子进程还没被回收、pid 仍属于它时 kill。
  - `common::start`（`spawn`、`spawn_with`、`spawn_from` 和直接调 `start` 的用例都经过它）在启动之前先调它。
  - T196 在升级用例和 `bundled_host` 里加的两处显式调用，随之去掉。
  - `gateway.rs` 的孤立启动用例有 30 s 的 `ExitWatch`，被启动的是 `cupper-host` 和网关夹具。它不经过 `common::start`，所以引入 `common`，在 `start_orphaned` 里也先跑一次。
- **核对**（每次先 `touch` 宿主的 `main.rs`、`cargo build` 重新链接，再跑 `it_lifecycle_host_exit_and_sigterm_exit_in_order`，修前修后各 4 次，负载约 5）：
  - 修前：第一次 `connect` 要 1,137–1,160 ms（扫描落在里面），之后的 `connect` 只要 45 ms。
  - 修后：`first_start` 用了 1,064–1,126 ms，接着的 `connect` 只要 47–69 ms。
  - 扫描从计时里挪了出来。T196 量过，负载约 35 时这一段要 12–29 s。
- **其他测试二进制**：`startup.rs` 没有计时的等待；`migrations.rs` 启动的是它自己；`gold_b.rs` 在进程内。都不用改。
- **改动**：`common/mod.rs` 增 25 删 16（多出的主要是按路径只跑一次），`gateway.rs` 增 5，`lifecycle.rs` 删 2。

## 第 5 处：`fake_agent_host_from` 在 `connect` 失败时回收宿主

- **修法**：`fake_agent_host` 和 `fake_agent_host_from` 改为返回 `Running`，在 `connect` 之前就把宿主包进去。调用处随之改：
  - `gateway_sync.rs` 的 5 处，不再自己包一层 `Running`；
  - `lifecycle.rs` 里 `terminate` 和 `exit_code` 改用 `.0`，原来包的 `Reaped` 去掉；
  - `driver_k.rs` 直接返回它。
- **核对**：临时用例（不提交）用一个从不监听的桩脚本当宿主，`exec /bin/sleep 299.123`。`connect` 等满 30 s 后 panic。
  - 修前：留下一个 `sleep 299.123` 进程。按命令行和启动时间核实后，用 SIGTERM 收掉。
  - 修后：没有留下进程。
- **改动**：4 个文件共增 27 删 27。

## `test-repeat.yml`

- **输入**：`kind`、`package`、`args`、`filter`、`times`（每一份跑几次）、`parallel`（几份同时跑）、`runner`。一次调用共跑 `times × parallel` 次。
  - `args` 是在任务书之外加的：宿主的用例要靠它传 `--features test-harness --test …`。
- **写法**：照 `e2e-repeat.yml`。权限只读；输入经环境变量传进脚本，`args` 用 `read -r -a` 拆成数组，不用 `eval`；action 锁定到 SHA；复用 `setup`。
- **cargo**：先构建假 agent 和 test-harness 版的 CLI，和 rust 作业一样。再用 `--no-run --message-format=json-render-diagnostics` 拿到测试二进制。每次运行，把每个二进制带着过滤条件各跑一遍。
- **vitest**：在项目目录里执行 `vitest run -t <filter>`。
- **判失败**：
  - 一次运行用 `perl alarm` 限 20 分钟，超时记为失败；
  - 输出里没有"有用例跑过并通过"的那一行，也记为失败，免得过滤条件写错了也能"通过 N 次"；
  - 任何一次失败，作业就失败；失败那几次的日志作为构件上传。
- **本机核对**：actionlint 1.7.12 无告警。用 macOS 自带的 bash 3.2，单份跑了两种 kind 的小样例，覆盖通过、失败、超时和没匹配上用例四种情况。
- 编排者把它单独挑进了 PR #9，先合进 main，好让 `workflow_dispatch` 能触发。

## 修后在 runner 上的连跑（取舍）

- 第 1 处：`test-repeat`，`times=100`、`parallel=2`，共 200 次。
- 第 3 处：`times=50`、`parallel=2`，共 100 次，不跑 200 次（编排者定）。这条用例不加负载时一次约 50 s，2 份并行跑 200 次会超过作业 150 分钟的上限。在负载下跑 100 次，作为证据已经足够。

## 第 2 处：E2E-NFR1-4 "a window whose connection dropped resumes from its cursor"

- **本机那次的现场**：T192 跑本机 `ci pr` 时出的（负载约 47，记录在 T043 worktree 的 ci-logs 里）。
  - 用例跑了 68 s，同一批的其他 NFR1-4 用例都是 10–12 s。
  - 失败在 `disconnectWindow` 等 "the window connects again" 的 20 s 轮询上。
  - 附件（main.log、host.log）已经被后来的运行覆盖了。
- **runner 上修前复现**：main 上 `e2e-repeat` 跑 50 次（run 40443859457），失败 1 次（repeat18）。
  - 通过的 49 次，每次 6.9–11.3 s。
  - 失败的那次，轮询等满 20 s 还没看到新连接。随后拆除超出了用例 90 s 的时限，日志没能附上，只留下 error-context.md。
- **读代码的怀疑点**：
  - 窗口连接断了以后，桥只重连一次（`#lost`）：先等 control 就绪，再调一次 `connect`。
  - `connect` 里 `launcher.open` 的握手超时是 `TIMEOUT_MS = 5000`。超时或失败后，窗口状态置为 `failed`，不再重试。
  - control 连接也是这样：断了就重连，`launcher.open` 失败就 `fail`。
  - 设计 5.7.1 写的是"宿主断开或重启：主进程先重连 control，再为每个窗口重连数据连接"，没有写重试次数，也没有写退避。`ConnectionState` 的 `failed` 带原因，T071 的裁定里原因是拉起失败一类，ConnectionToast 会一直显示到连接再次就绪。
  - 拆除超时还说明一点：失败之后 App 也没能按时退出。
- **为了看清停在哪一步，补了诊断**（83feb2a）：
  - e2e 的 `disconnectWindow` 轮询一放弃，就先附上窗口自己报的连接状态、等了多久、main.log 和 host.log；通过时把重连耗时记成 `reconnect_ms` 注解。
  - 主进程每次建窗口连接都记下耗时；失败时记下原因。
  - 请编排者在本分支上用 `e2e-repeat` 跑 100 次。

### runner 上的修后连跑结果

- **第 1 处**：run 40445262092（提交 7d0fef5），`times=100`、`parallel=2`。构件 `results.txt` 里 200 次全是 ok。
  - 作业被标成失败，是 `test-repeat.yml` 自己的错：runner 的 bash 带 `-e`，200 次全过时 `grep -c ' FAILED$'` 找不到匹配、退出码为 1，脚本没打完汇总就退出了。8216080 已修（Run 这一步 `set +e`），编排者另开 PR #10 挑进了 main。
- **第 3 处**：run 40445328766（7d0fef5），`times=50`、`parallel=2`。`results.txt` 里 100 次全是 ok，作业被标成失败的原因同上。
- 本机测这个 workflow 脚本时，用过一次 `parallel=2` 的小样例（fake-agent 的 `unit_replay_each_start_plays_the_next_segment`）。22 次里失败过 1 次，日志被我清目录时删了，查不出原因；之后又跑了 38 次，都没再出现。编排者提醒：本机并行也算加压，以后本机只跑单份小样例，并行的情况放到 runner 上验。

### 第 2 处：runner 上的诊断

- **run 40445893797**（83feb2a，100 次）：失败 1 次（repeat26），99 次通过；通过时重连耗时 8–52 ms，中位数 14 ms。
  - 失败那次是用例超过 90 s 的时限，不是轮询失败。拆除也超时了，总共 180 s。诊断没触发，日志也没附上。
- **5b0a306 的改动**：fixture 在用例失败时先附日志、再退出 App；`disconnectWindow` 在断开 hook 返回后记一个注解。
- **run 40448998123**（5b0a306，200 次）：失败 1 次（repeat63），199 次通过；重连耗时 8–40 ms，中位数 14 ms。失败那次的日志附上了：
  - 断开 hook 正常返回（04:35:10.904）。host.log 在 10.906 记下窗口连接关闭（`peer_closed`）。
  - 此后 App 进程一次也没再连上宿主。测试的 `status` 每秒照常连接、应答，一直到 04:35:30。
  - main.log 停在断开之前的那行 "window connected"。之后既没有 "window connected"，也没有 "window connection failed"，主进程再没记过一行日志。
  - 轮询在 20 s 后放弃。catch 里读窗口状态的 `evaluate` 当时没设上限，挂住了，所以报的是用例超时。拆除时退出 App 的 `app.evaluate` 也挂住了。
- **结论**：窗口的数据连接被断开后，大约每 100 次有 1 次，Electron 主进程不再应答，`connect()` 既不成功也不失败。这是产品问题。
  - 重连路径上的同步调用只有原生扩展的 `connectHost`。e2e 是 dev-auth 构建，它只做 `UnixStream::connect`、核对 uid、`Relay::start`，读代码看不出会阻塞的地方。
  - 照约定停下，报给编排者。
- **再一步的诊断**（fcde9c1，不改行为）：
  - 两个探测都限 5 s。
  - 轮询放弃时，用 `/usr/bin/sample` 对主进程采样 3 s，附上所有线程的栈。
