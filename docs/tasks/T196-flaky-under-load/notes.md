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

## 范围外（未改）

桌面端的原生扩展 `cupper-desktop-native` 的 `relay.rs` 有同样的写法：每次读后 `peer_token(&self.host) != Ok(self.pinned)`，宿主一退出就以 `TokenChanged` 结束，把这次读到的字节丢掉。桌面端在宿主退出时可能收不到 `host/shutdown` 或最后的回复，修法同上。这在桌面模块，本任务没有改，汇报里提出。
