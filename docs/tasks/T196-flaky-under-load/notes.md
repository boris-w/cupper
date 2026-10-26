# T196 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前的规模估计

| 模块 | 估计 |
|---|---|
| 第 1 处：扫描交出走过的条目数，用例改为断言它（去掉墙钟上限） | 20 |
| 第 2 处：视根因而定；若是产品的退出竞态，就地修约 40；若只是用例等得太紧，约 10 | 10–40 |
| 同类排查中顺带改的用例 | 20 |
| **合计** | **约 50–80** |

在停止线 200 之内。复现与测量用的脚本是一次性的，不提交。

补充的第 3、4 处（停止线改为 260）：

| 模块 | 估计 |
|---|---|
| 第 3 处：用例在下一条 `send` 前等上一个回合结束 | 10 |
| 第 4 处：视在最新 main 上的复现而定；若是假 agent 新链接后第一次启动慢，就在测试客户端里先把它跑一次，约 25 | 0–25 |
| **合计** | **约 10–35** |

第 3 处的估计是和第 4 处一起补写的，晚于第 3 处的提交（漏了先写估计这一步）；它的实际改动是 4 行。合计后仍在停止线 260 之内。

## 负载的造法

后台跑 3 份（后来 5 份）`cupper-host` 库测试二进制的死循环（脚本记下 pid，测完用一个停止文件加 SIGTERM 收掉）。期间平均负载 19–35，最高 52；其他 agent 的任务也在同时跑。

## 第 1 处：`unit_trust_scan_forking_directory_links_hit_the_entry_limit`

- **复现**：负载约 25 时连跑 20 次，耗时在 0.65 s 到 6.06 s 之间，分布很散。CI 那次负载约 16，用了 23.4 s，超过 10 s 的墙钟上限。
- **根因**：用例的产品行为没有问题。走到第 20,001 个条目时 `Walker::entry` 当场返回 `TooManyEntries`，不会再往下走。墙钟上限只是间接证明"没走完 2^20 次"，而它会被负载拉长：负载高时，文件系统元数据调用排队很久。
- **修法**：`walk` 加一个计数版本 `walk_counting`，把走过的条目数交出来（失败时也交）；原来的 `walk` 调它，行为不变。用例改为断言 `too_many_entries` 且走过的条目数正好是 `MAX_ENTRIES`，去掉墙钟上限。计数不受负载影响。改动：`walk.rs` 增 23 删 5，`tests.rs` 增 5 删 8。
- **修后**：同样的负载下（中位数约 32，最高 52）连跑 200 次全过。耗时最短 0.63 s、中位数 3.33 s、最长 14.40 s，按旧的 10 s 上限会失败几次。

## 第 2 处：`it_lifecycle_host_exit_and_sigterm_exit_in_order` 报 "Sigterm: no host/shutdown"

- **先量**：
  - 负载约 30 时，"SIGTERM 到客户端收到第一条消息"共跑 200 次全部成功：中位数 2.2 ms，p95 4.9 ms，最长 12.9 ms。
  - 原用例连跑 100 次也全过。
  - 这说明宿主在 SIGTERM 下照常发 `host/shutdown`，而且很快，等待本身并不紧。
- **找到条件**：让客户端在宿主退出之后才去读。客户端的读任务跑在测试唯一的线程上，测试先阻塞等宿主退出，再 `next()`。这样 20 次里 20 次都收不到 `host/shutdown`：
  - `next()` 返回 `None`，关闭原因是 `Refused(PeerExited, getsockopt(LOCAL_PEERTOKEN), 57)`；
  - 宿主那边的审计是 `conn.closed {reason: host_exiting, after_ms: 8–53}`，说明宿主确实把帧发出去了，并且在宽限期内正常收尾。
- **根因：客户端库 `cupper-cli` 的产品缺陷，不在宿主，也不是等得太紧**。
  - `read_frames` 每读一次就核对一次宿主的令牌，宿主一退出，核对就因 `PeerExited` 失败；
  - 失败时直接返回，把这次刚读到的字节（`host/shutdown`，以及 `host/exit` 的回复）丢掉了；
  - 平时读任务很快，赶在宿主退出前读完；负载高时它被拖到宿主退出之后，消息就丢了。
  - T049 在 `run.rs` 里见过同一现象（"A host that writes and closes is refused as gone before its data is read"），当时只把这种拒绝当作"宿主已不在"，没有修数据丢失。
- **修法**（产品，单独提交）：
  - 令牌核对遇到 `PeerExited` 时不再返回，照常交付读到的帧，继续读到流结束，以 `Eof` 收尾。
  - 这样做安全：连接是已建立的流，对端退出后没人能再往里写；缓冲区里的字节只能是被钉住的宿主写的。`PeerExeced`（对端换了程序）和令牌不符仍然立即断开。
  - `run.rs` 里专门把 `Refused(PeerExited)` 当作"宿主已不在"的那一支随之不再出现，删掉，连同它的测试样例。
  - 改动：`client.rs` 增 5 删 1，`run.rs` 增 3 删 10。
- **用例**：IT-lifecycle 改为先等宿主退出、再读客户端消息，这是客户端最坏的先后顺序。`host/shutdown` 和流结束仍要都到；`HostExit` 和 `Sigterm` 两种触发都这样。
  - 去掉客户端修复后，这条用例 3 次里 3 次失败（`HostExit` 那一轮就失败）；
  - 修复后，同样负载下连跑 200 次全过，耗时中位数 0.49 s，最长 6.02 s。

## 同类排查

- `trust/tests.rs`：没有别的墙钟上限。
- `tests/lifecycle.rs`：
  - `it_self_check_a_changed_executable_makes_the_host_wait_for_an_upgrade` 同样在宿主退出前后读 `host/shutdown`，也会碰上同一个客户端缺陷，已由客户端修复覆盖。它原来容忍 `session/interrupt` 的回复可能丢失（"Its reply races the exit"）；这个回复排在退出之前发出，修复后一定能读到，所以改为 `unwrap()`。
  - `it_clock_advance_returns_once_the_host_settled` 用的是 testkit 的原始客户端，不核对令牌，不受影响。

## 第 3 处：`it_cli_commands_read_elevate_write_and_answer` 第三条 `send` 得到 `queued`

- **复现**：负载约 43 时连跑 30 次，失败 4 次，报 `question colour?`：`left (0, "queued")`，`right (0, "started")`。
- **先量**：临时加了打点（已撤掉，不提交）。用例看到 agent 回话（"said"）的那一刻，上一个回合常常还是 `running`；它在 0.9–52 ms 后才结束。
- **根因：用例的缺陷，宿主没有问题**。
  - 假 agent 先说出它拿到的结果，再回 `session/prompt` 的响应；宿主收到响应才结束回合。
  - 用例只等到 agent 回话，就发下一条。负载高时，这一条落在上一个回合结束之前，按 `queue` 模式排队，这正是宿主该有的行为。
- **修法**：每轮在等到回话之后，再等 `turns` 里没有 `running` 的回合，然后才发下一条。
  - 回合的行和 `send` 的处置都由同一个会话 actor 写和判断（单写者），所以行一旦读到已结束，actor 处理下一条 `send` 时上一回合必定已经结束。
  - 改动：`lifecycle.rs` 增 4。
  - 后面的 `send {other} "say again" --wait` 不受影响：`--wait` 按回话里的 `turn_id` 或 `queue_id` 等，排队了也能等到。
- **修后**：负载 41–68（中位数 54）下连跑 200 次全过，耗时最短 0.23 s、中位数 0.43 s、最长 6.29 s。

## 第 4 处：fake-agent 的 `unit_replay_*` 报 "no response 1" 和 "the agent did not exit"

- **T191 那次的现场**：三个用例同时失败，前两个是 `no response 1; seen: []`，第三个是 `the agent did not exit`，测试二进制总共用了 10.37 s。也就是三个 agent 在 10 s 里一个字都没写，正好是 `WAIT` 和 `exit` 各自 10 s 的上限。
- **在最新 main 上先照原样连跑**（含 T189 的修复）：负载 41–58（中位数 49.6）下连跑 200 次，全过，每次 0.29–0.38 s。照原样复现不出来。
- **找到条件：假 agent 的二进制刚链接好之后的第一次启动**。
  - CI 里先构建、再跑测试，`replay.rs` 是第一个启动假 agent 的测试二进制（`persona.rs` 不启动它，`scripts.rs` 排在后面）。
  - 每次先重新链接假 agent（`touch` 源文件后 `cargo build`），再跑 `replay.rs`：13 次里失败 4 次，失败的总耗时 10.52–11.04 s，三个用例的报错和 T191 那次一模一样。通过的 9 次也要 4.45–9.33 s，紧接着的第二次只要 0.31–0.96 s。
  - 单独量启动：负载 36–45 下，刚链接好的假 agent 带一个让它立即退出的参数，第一次启动到退出用了 3.77–10.98 s（6 次），第二、三次只要 3–13 ms。
  - `spawn` 本身 2–4 ms 就返回；子进程这段时间处于睡眠、CPU 为 0，同时系统的 XProtect 服务和一个 CoreServices 进程在忙。可见是系统在第一次执行新可执行文件时对它做扫描，扫描完才放行。把二进制复制一份再换回去，也会重新扫描（0.75–10.7 s）。
- **根因：用例的等待把系统扫描新二进制的时间算成了 agent 的沉默**。T189 修的 `Fake::exit` 管不到这里：`wait_for` 的 10 s 和 `exit` 的 200×50 ms 都在扫描的耗时范围之内。
- **修法**（只改测试客户端）：`Fake::spawn_with` 第一次被调用时，先用 `--fault exit_on_start` 把假 agent 跑一次，等它退出（最多 60 s，用 `Once` 保证同一测试二进制只跑一次），之后才开始计时。这个用例拉起的子进程到 60 s 仍不退出时，趁它还没被回收、pid 仍属于它，才对它 `kill`。改动：`tests/common/mod.rs` 增 30。
- **修后**：每次都先把假 agent 复制一份换上（等于新链接，会重新扫描），再跑 `replay.rs`。负载 28.7–57.6（中位数 41.1）下连跑 200 次全过；总耗时最短 3.74 s、中位数 8.12 s、最长 17.21 s，其中 72 次超过 10 s，超出修前的等待上限。
- **对照**：修前的测试二进制用同样的做法跑 30 次（负载 32.8–49.0），失败 6 次，报的正是 12 处 "no response 1; seen: []" 和 5 处 "the agent did not exit"。

## 同类排查（第 3、4 处）

- 宿主测试里别的 `started` 断言（`interaction_trust.rs`）等的是 agent 的输出本身，不是下一条 `send` 的处置，不受第 3 处影响。
- `scripts.rs` 也经 `Fake::spawn_with` 启动假 agent，同样受益于第 4 处的修复。
- 宿主的集成测试直接启动 `cupper-host`，以及经适配器启动假 agent，等待上限是 `PATIENCE` 30 s。这次见到的最长扫描约 17 s，暂时还在上限之内，本任务没有改。

## 范围外（未改）

桌面端的原生扩展 `cupper-desktop-native` 的 `relay.rs` 有同样的写法：每次读后 `peer_token(&self.host) != Ok(self.pinned)`，宿主一退出就以 `TokenChanged` 结束，把这次读到的字节丢掉。桌面端在宿主退出时可能收不到 `host/shutdown` 或最后的回复，修法同上。这在桌面模块，本任务没有改，汇报里提出。

宿主的 `VERSION_PROBE` 是 5 s（CLI、git 的 `--version`）。CLI 更新后的第一次执行也会被系统扫描；按这次在负载 40 左右量到的 4–17 s，机器忙时这一次探测可能超时。这只是推断：本任务不运行真实的 CLI，没有量过，留给编排者判断要不要跟进。
