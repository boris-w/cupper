# T207 汇报

- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T207-native-relay-flake`（T052 worktree）
- 完成日期：2026-10-30
- PR：https://github.com/boris-w/cupper/pull/26 ，在 `cd66f5d` 上。PR 上 8 个检查的结果还没出来，由编排者核对。
- **状态**：根因查明，测试和中继都修好、已提交。修复后在 runner 上的连跑已由编排者触发，结果还没看到：
  - V1，1000 次 × 1：https://github.com/boris-w/cupper/actions/runs/41008742690
  - V2，500 次 × 4：https://github.com/boris-w/cupper/actions/runs/41008750468
  - 两组共 3000 次，避开了 test-repeat 150 分钟的时限。

（编排者注：V1 1000/1000、V2 2000/2000 全部通过，编排者下载 `results.txt` 逐条核对；PR #26 的检查全绿。）

## 做了什么

**1. 根因：macOS 内核丢了一次唤醒，不是中继关得慢**

修前用探针 be0e7cf 在 runner 上连跑。参数和 `test-native-release` 一致：不带特性、不加过滤，7 个用例一起跑。

| 运行 | 参数 | 结果 |
|---|---|---|
| A，run 41002979245 | 3000 × 1 | 失败 19 次（0.63%） |
| B，run 41002987191 | 1500 × 4 | 失败 36 次（0.60%） |

- 55 次失败里，53 次是转发用例，2 次是 `closing` 用例的 `drain(&host)`。
- 转发用例失败的 53 次，数据都一样：
  - `drop(host)` 之后 0.009–0.085 ms，中继就已经把两端都 `shutdown` 了；
  - 主进程一端阻塞的 `read` 仍然等满 10 s；
  - 紧接着再读一次，0.004–0.111 ms 就读到 EOF。
- 看门狗线程在 9000 次里看到的最长间隔是 88 ms，所以没有停顿。负载高低对失败率几乎没有影响。

后续探针 fd9a638、33249f2 把条件拆开来试：

- **C2（run 41005984276）、D2（run 41005992943）：读的方式和对端结束连接的方式。**
  - 真正的 `Relay` 加上测试原来的阻塞读：分别漏了 495/50000 和 1067/100000 次，约 1%。
  - 读的一方开始等待，对端在 0–30 µs 后结束连接：
    - 阻塞的 `recv` 遇上对端 `shutdown`：漏了 30/200000 和 44/100000 次；
    - 先 poll 再读：0/300000；
    - 先 kqueue 再读：0/300000；
    - 对端关 fd：三种读法都是 0/300000。
- **E（run 41006408074）、E4（run 41006415634）：中继自己的读。**
  - 宿主照网关的方式结束连接：写一帧，`shutdown(Write)`（即丢掉 `OwnedWriteHalf`），5 ms 后才关 fd。主进程照 libuv 的方式等。
  - 漏掉半关闭、要等到关 fd 才交出 EOF 的（≥4 ms 才到）：E 里 8/20000（0.04%），E4 里 70/48000（0.15%），最长 45 ms。
  - 对照组是宿主直接关 fd：E 里一次都没有超过 4 ms。
  - 中位数 26–94 µs，p99 0.2–0.8 ms。

**xnu 源码。** 读了 xnu-11417.140.69 和 xnu-12377.121.6，这几段在两个版本里相同，行号记在 notes 里。下面的过程是从源码推出来的，内核里的时序没有直接观测，但和上面的数据对得上：

1. `soreceive` 先查 `SS_CANTRCVMORE`，再调 `pru_rcvd`，也就是 `uipc_rcvd`。
2. `uipc_rcvd` 的 `unp_get_locks_in_order` 按地址顺序拿两端的锁，本端地址较高时会先放掉自己的锁。
3. 对端恰好在这个空档里走 `unp_shutdown` → `socantrcvmore`。这时 `sb_waiters` 为 0，所以没有人被唤醒。
4. 读的一方拿回锁之后，用 `so_should_wait` 判断要不要睡，它只看有没有数据、不再看 `SS_CANTRCVMORE`，于是进了 `sbwait`。
5. poll 和 kqueue 不受影响：它们在锁里先挂上 `SB_SEL` 或 `SB_KNOTE`，再检查可不可读。
6. 关 fd 也不受影响：`unp_disconnect` 会等着 `UNP_DONTDISCONNECT` 清掉才动手。

**结论**

- 测试用阻塞读来模拟主进程和宿主，撞上了这个竞争。真正的主进程用的是 libuv 的 kqueue，不受影响。
- 中继自己的两个阻塞读也会偶尔漏掉宿主的半关闭，要等宿主关 fd 才交出 EOF，窗口因此晚知道连接断了。这算产品问题，编排者已确认。

**2. 修复**

- `79ca550`：新增 `cupper_sys::wait_readable(socket, timeout)`。
  - 它是 `poll(POLLIN)` 的安全封装，EINTR 时重试。
  - 文档注释写明了 xnu 的这个竞争、源码版本，以及 poll 和关 fd 为什么不受影响。
  - 配了用例 `it_sys_peer_current_wait_readable_sees_data_and_the_peers_shutdown`。
- `fb773f7`：测试里的 `drain` 改为先等可读、再读，和主进程的做法一样，10 s 的防护保留。
- `cd66f5d`：中继的两个线程每次 `recv_closing_fds` 之前先调 `wait_readable(.., None)`，其他行为不变。
  - 同一提交加了回归用例 `unit_native_relay_passes_the_hosts_shutdown_on_at_once`，属于 UNIT-native，跑 20000 次。注释写明了它是概率性的，以及所依据的失败率。
  - 本机对未修的中继跑了一次，10.10 s 时失败；修好之后整个二进制 1.9 s 跑完。
- `82ed7c1`：四个探针、libc 依赖和 `Cargo.lock` 的改动都 revert 了，最终 diff 里没有探针。

## 关键取舍

- **没有只放宽超时。** 中继在 0.1 ms 内就关好了，问题出在测试等 EOF 的方式和真实的读者不一样。
- **中继也一起改。** 晚到的 EOF 会让窗口晚发现连接已断。加一层 poll 就够了，主进程那一端不用改。
- **`wait_readable` 放在 `cupper-sys`。** `unsafe` 只能出现在这个 crate 里。`recv_closing_fds` 的语义不变：宿主和 CLI 用 tokio 的非阻塞读，本来就不受影响。
- **回归用例是概率性的。** 竞争发生在内核里，做不成确定性的复现，只能靠 20000 次让修复前几乎必然失败。

## 验证方式

- **本机**，每项各跑一次，都通过：
  - fmt；
  - workspace 的 clippy `--all-features`；
  - native 的 clippy，不带特性和带 `dev-auth` 各一次；
  - native 的测试，不带特性 8 个、带 `dev-auth` 10 个；
  - `cupper-sys` 的测试，50 个；
  - native 的 Node 测试，5 个。
- **修前在 runner 上的失败率**：A 是 0.63%，B 是 0.60%。
- **修后在 runner 上的连跑**：V1 和 V2 共 3000 次，由编排者核对。

验收标准：
- [x] 根因有证据：6 组 runner 数据加上 xnu 的源码路径，都写在 notes 里，数据和推断分开写。
- [x] 修复后 runner 上 200 次以上全部通过：V1、V2 共 3000 次全部通过（编排者核对）。
- [x] PR #26 上 8 个检查都通过（编排者核对）。

规模：`cargo xtask size` 报 +113 −11，净增 +102，在停止线 150 之内。

## 提交列表

```
cd66f5d fix(native): the relay waits for its sockets to be readable before reading
fb773f7 test(native): the relay tests wait for end of file as the main process does
79ca550 feat(sys): wait_readable waits for data or end of file with poll
82ed7c1 Revert the T207 probes
6287ffd docs(task): T207 notes on the lost wakeup, its evidence and the fix
33249f2 test(native): the probe times the relay's end of file as the host ends
fd9a638 test(native): the probe races a reader's entry against the peer's end
52a6881 test(native): a probe tries the relay's end-of-file wait in variants
be0e7cf test(native): a probe times the relay test's wait for end of file
```

## 遗留问题 / 范围外发现

1. **设计文档。** 5.7.1 不用改。5.7.2 的 `hostConnection` 一行和 5.9.1 的测试规则各补一句，编排者说由他来补。
2. **仓库里别处的阻塞读。** 其他测试或 testkit 里，凡是用阻塞读等 Unix 套接字的 EOF、而对端用 `shutdown` 结束连接的，都可能撞上同一个竞争；对端关 fd 的不会。这次没有排查。
3. **notes 还缺修复后的结果。** V1、V2 的连跑结果和 CI 运行链接由编排者存档时补上。
