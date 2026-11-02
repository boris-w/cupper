# T215 汇报

- 执行者：子 agent（工程师，宿主网关与桌面主进程）
- 分支：`fix/T215-restart-handshake`
- 完成日期：2026-11-02
- PR：https://github.com/boris-w/cupper/pull/32 。CI 运行 https://github.com/boris-w/cupper/actions/runs/41390631704 （f244919，第 1 次 attempt，8 个检查全部通过）。
- e2e-repeat：https://github.com/boris-w/cupper/actions/runs/41390534308 （0bb75c4，120 次全部通过）。

**状态**：完成。两处问题都已修好，并加了用例：握手被拒时桌面能稳定拿到拒绝原因、弹出"需要重启"；重启宿主之后窗口会重新连接。原因有直接证据：被丢掉的正是那条握手错误应答。

## 做了什么

**第 1 条的原因（有证据）**
- 复现方式：e2e 构建，本机单份跑 4 次。第一个 app 带着运行中的回合按关机方式退出，宿主留着；再启动第二个 app，它的 key 不同。原生扩展的 relay 和主进程握手失败处各临时加了一处日志，都没有提交。
- 修前：8 次握手里 5 次报 `closed`，4 次里只有 2 次弹出重启。
  - 报 `closed` 的每一次，relay 的关闭原因都是 `token_changed`。
  - relay 记下的被丢字节正是 182 字节的握手错误应答 `{"error":{…,"data":{"code":"unauthenticated","reason":"bad_key"}}}`。
  - 比对令牌时，`getsockopt(LOCAL_PEERTOKEN)` 返回 ENOTCONN（PeerExited）。
- 宿主那边：握手失败后，错误发完就关掉 fd。客户端要是在这之后才读，令牌已经取不到，按 5.8.1 拒收，主进程只看到连接关闭。这和 T213 是同一类问题。
- 对照实验（直接测 unix 套接字）：
  - 服务端只 `shutdown(SHUT_WR)` 时，客户端读数据、读到文件末尾，全程都取得到令牌；
  - 服务端 `close` 之后，立即 ENOTCONN；
  - 只半关闭时，客户端一关，服务端就读到文件末尾。

**修复**
1. **宿主**（`gateway/conn.rs`，经你批准）：
   - 范围：连接还在握手阶段、并且送了错误时（`HandshakeFailed`，以及 `initialize` 之前收到别的请求或坏 JSON 的 `NotInitialized`）。
   - 送完错误、写端关掉之后，不立即关 fd，而是继续读、读到就丢，直到客户端关闭，最多等关闭宽限 2 s。这个 2 s 和送错误共用同一个计时。
   - 进入 Ready 之后的结束，以及握手阶段因宿主退出（`HostExiting`）而结束，都和原来一样。
   - 5.8.1 没有放宽。
2. **桌面**（`bridge/bridges.ts`、`window-bridge.ts`、`index.ts`）：
   - `WindowBridges` 记下连接失败的窗口，两种情况：第一次 `open` 就失败（失败的是当前文档），或者断开后重连失败。
   - 控制连接每次回到 `ready`，就对这些窗口 `webContents.reload()` 并清空记录，新文档自己重新 `open`。
   - 连接成功的窗口从记录里去掉；窗口销毁时也立即去掉。正常的窗口不会被 reload。

**用例**
- **IT-handshake**（新增）：握手被拒后，宿主一直留着自己那端，直到客户端关闭。
  - 用 ManualClock，宽限不会到期。
  - 客户端拿到 `unauthenticated` 并读到文件末尾后，等静默屏障；断言网关的连接任务还在、客户端仍取得到宿主令牌。客户端关闭后，审计记 `HandshakeFailed`。
  - 原有三处握手失败用例改成先 drop 客户端，再等 `Closed`。
- **UNIT-bridge-lifecycle**（新增），四个窗口：
  - 窗口 1 第一次 `open` 失败；
  - 窗口 2 断开后重连失败；
  - 窗口 3 一直连着；
  - 窗口 4 失败后被关掉。
  - 断言：第一次 `reload()` 只重新载入 1、2，第二次什么都不做。
- **E2E-SEC-fake-host**（新增，`@smoke`）：key 不同的 app 连上前一个 app 拉起的宿主 → 弹出重启 → 确认 → 窗口连上新宿主（pid 不同）。

## 关键取舍

- **在宿主侧半关闭，不在桌面侧补救**：
  - 被拒收的字节，按 5.8.1 不能交付。
  - 桌面把 `closed` 猜成"未认证"会误判：握手中途宿主崩溃也是 `closed`。
  - 重试也一样碰运气（修前大约一半失败）。
  - 宿主只半关闭写端，令牌就一直有效，客户端能正当地读到应答。CLI 同样受益。
- **只改握手阶段**：不改变 Ready 之后连接的结束方式和宿主退出的时长，T213 的结论（调用方把连接结束当作退出已受理）也不动。
- **窗口用 reload，不改 ShellAPI**：
  - ShellAPI 已冻结，没法通知渲染进程"重新打开"。
  - 渲染进程只 `open` 一次。需要 reload 的窗口本来就停在失败状态、没有数据，reload 相当于用户手动重新载入。
- **你要求写明的几点**：
  - **等待期间的数据**：读到即丢，固定 1 KiB 缓冲，不缓存、不解析，内存不会随对端写入增长；对端附带的 fd 照旧经 `recv_closing_fds` 关掉。
  - **连接上限**：网关**没有**连接数上限。`connections` 只是给连接编号；`live` 是在跑的连接任务数，`close_all` 等它归零。等待中的连接仍算在 `live` 里，占一个 fd 和 1 KiB，最多 2 s。没认证的连接本来就能在 10 s 的握手期限内一直不发 `initialize` 占着，这 2 s 不超出已有的暴露面。
  - **`participant.idle` 与有序退出**：`idle` 只对静默屏障（测试里的 `test/clock_advance`）有意义，不影响退出。宿主退出时，`close_all` 会等这条连接，最多等它剩下的宽限：从错误进入发送队列算起不超过 2 s，与原来"送完错误再关"的上限是同一个计时，所以最坏情况不变。正常客户端读到文件末尾就关（桌面 relay 双向 shutdown，CLI 握手失败即退出），实际是毫秒级。握手阶段因宿主退出而结束时，不进这段等待。
  - **失败窗口的记录**：第一版里，关掉的窗口要等到下一次 `ready` 才清，不是有意的，已经改成在 `destroyed` 时立即去掉（af737ea），并加了用例。
- **规模**：`cargo xtask size` 为 +133 −12，在停止线 150 以内。每个提交的累计规模见 `notes.md`。

## 验证方式

**修前修后对照**
- **IT-handshake 新用例**：宿主改动换回 1828dbf，连跑 3 次都失败（`running(CONN)` 为 0：宿主已经关掉了自己那端）；修后通过。
- **e2e 测量**（4 次，临时日志）：
  - 修前：8 次握手里 5 次是 `closed`（relay 为 `token_changed`），弹窗 2/4；
  - 修后：8 次都是 `unauthenticated`（relay 为 `host_closed`），弹窗 4/4，relay 没有丢任何字节。
- **新 e2e 用例本机跑**（屏幕解锁后补跑的）：
  - 产品代码全换回 1828dbf：连跑 3 次都失败在"a native dialog"，`main.log` 里控制连接是 `failed: closed`，窗口连接也是 `closed`；
  - 只带宿主修复：弹出重启，确认后失败在"the window connects to the host"（窗口没有重连）；
  - 两处都修：通过。
  - 这一组本机对照只写在这份汇报里，`notes.md` 里没有，避免为此再推一次。
- **UNIT-bridge-lifecycle**：去掉 `destroyed` 时的清理，用例失败，重新载入了 1、2、4。

**runner 与 CI**
- e2e-repeat：spec `src/connection.spec.ts`，test "E2E-SEC-fake-host: a host an earlier app started is restarted"，120 次，workers 1。
  - 运行：https://github.com/boris-w/cupper/actions/runs/41390534308
  - 提交 0bb75c4；下载 `repeat.json` 核对：expected 120，unexpected 0，flaky 0。
  - 之后的 af737ea 只改了窗口销毁时的清理，有单测守着；你同意不重跑。
- PR #32 在 f244919 上的 8 个检查，第 1 次 attempt 全部通过：compat-framework、e2e、license、protocol、rust、testdata-scan、ts、ui。
  - 运行：https://github.com/boris-w/cupper/actions/runs/41390631704

**本机检查**（都只跑一份）
- 通过：`cargo fmt --check`；clippy（cupper-host，all-targets、all-features，`-D warnings`）。
- 通过：cupper-host lib 454 个，tests/gateway 和 tests/startup，cupper-cli 测试。
- 通过：桌面单测；prettier、eslint、tsc。
- 没跑 `@focus`，没有运行真实的 Claude Code、Codex CLI。临时日志和测量 spec 都已删除，工作区干净。

**验收标准**
- [x] 第 1 条的原因有证据；修后握手被拒时，桌面稳定拿到拒绝原因并弹出重启（4/4，加上 runner 120/120）。
- [x] 重启宿主后，窗口重新连接。
- [x] 两条都有用例守着；新 e2e 用例在 runner 上连跑 120 次全部通过。
- [x] 设计要补的句子见下。
- [x] PR #32 的 8 个检查全部通过。

**设计补充**（经你同意；请合并时写进 03-protocol-v1.md，"T030 开工前的裁定"里"握手失败一律先回错误，再断开"那一条之后）：

> 断开时宿主送完错误只半关闭写端，等客户端关闭再关自己那端，最多等关闭宽限（2 s）：客户端每次读取都比对宿主的令牌（5.8.1），宿主关掉自己那端之后读到的字节一律拒收（T215）。

## 提交列表

```
f244919 docs(task): T215 notes on idle waits at exit and closed failed windows
af737ea fix(desktop): a closed window leaves the failed windows' record at once
0bb75c4 docs(task): T215 notes on the cause, the evidence and the fixes
aa3b091 test(e2e): E2E-SEC-fake-host, a host an earlier app started is restarted
d59835b fix(desktop): windows that could not connect load again once the host is back
866b16d fix(host): a refused handshake keeps the host's end open until the client closes
```

## 遗留问题 / 范围外发现

1. **宿主退出时的应答（T213 那种情况）还是可能被拒收**：本任务按批准只改了握手阶段。如果以后希望客户端能稳定读到 `host/exit` 的应答，可以把同样的"半关闭、等客户端关闭"用到退出时的连接上。代价是宿主退出可能多等最多 2 s。目前桌面和 CLI 都已经能正确处理这种情况，不急。
2. **控制连接回到 `ready` 之前就失败的窗口才会被 reload**。窗口的第一次 `open` 如果在重启完成之后才失败（实测没有见到；需要它的握手一直悬着、跨过整个重启），它要等下一次 `ready`。这和修复前的行为一样，不算退步。
