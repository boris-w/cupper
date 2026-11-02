# T215 笔记

## 规模

停止线 150，硬停止线 200（`cargo xtask size`，含测试）。

| 提交 | 内容 | 累计（`size --rev`） |
|---|---|---|
| 866b16d | 宿主：握手被拒后半关闭写端，等客户端关闭 + IT-handshake 用例 | +43 −3 |
| d59835b | 桌面：连接失败的窗口在控制连接回到 ready 时重新载入 + UNIT-bridge-lifecycle 用例 | +105 −11 |
| aa3b091 | E2E-SEC-fake-host 新用例 | +124 −11 |
| af737ea | 关掉的窗口立即从失败记录里去掉（编排者审 diff 时问到） | +133 −12 |

## 第 1 条：握手被拒时只看到 `closed` 的原因

**复现**：e2e 构建，本机单份跑 4 次。第一个 app 带着运行中的回合按关机方式退出，宿主留着；再启动第二个 app，它的 `HostKey` 是新的，宿主拒绝它的 `dev_key`。临时加了两处日志，都没有提交：

- 原生扩展 relay 的 `read_host`：令牌比对失败时，把读到的字节数、错误和字节内容写进临时文件；
- 主进程 `openConnection` 握手失败时，记下握手的失败原因和 relay 的 `closeReason`。

**修前**：8 次握手（控制连接、窗口连接各 4 次）里 5 次报 `closed`，只有 2 次弹出"需要重启会话宿主"。

| 次 | 控制连接 | 窗口连接 | 弹窗 |
|---|---|---|---|
| 1 | `unauthenticated`（relay `host_closed`） | `unauthenticated`（`host_closed`） | 有 |
| 2 | `closed`（`token_changed`） | `closed`（`token_changed`） | 无 |
| 3 | `unauthenticated`（`host_closed`） | `closed`（`token_changed`） | 有 |
| 4 | `closed`（`token_changed`） | `closed`（`token_changed`） | 无 |

报 `closed` 的每一次，relay 的记录都一样：读到 182 字节，令牌比对时 `getsockopt(LOCAL_PEERTOKEN)` 返回 `PeerExited`（ENOTCONN，57），丢掉的正是握手的错误应答：

```
token_changed len=182 err=Some(Refusal { kind: PeerExited, call: "getsockopt(LOCAL_PEERTOKEN)", code: 57 })
bytes= {"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"dev_key is only accepted from the process that started the host","data":{"code":"unauthenticated","reason":"bad_key"}}}
```

**宿主侧**（`gateway/conn.rs`）：握手失败时，错误进入发送队列，等写任务发完（写半端 drop 时 shutdown 写端），然后连接任务结束、关掉 fd。客户端在 fd 关掉之后才读的话，令牌已经取不到，按 5.8.1 拒收，主进程只读到文件末尾，握手以 `closed` 失败。和 T213 是同一类事。

**对照实验**（Python 直接测 unix 套接字，不涉及产品代码）：

- 服务端 `shutdown(SHUT_WR)` 之后，客户端读数据、读到文件末尾，`LOCAL_PEERTOKEN` 一直取得到；
- 服务端 `close` 之后立即 ENOTCONN；
- 服务端只半关闭时，客户端一关，服务端就读到文件末尾。

**修后**：同样跑 4 次，8 次握手全部是 `unauthenticated`（relay `host_closed`），4 次都弹出"需要重启会话宿主"，relay 没有丢弃任何字节。

## 第 1 条的修法（编排者批准）

- 只在握手阶段（连接还没进入 Ready），而且送了错误的结束：`HandshakeFailed`，以及在 `initialize` 之前发了别的请求或坏的 JSON（`NotInitialized`）。宿主照旧送完错误；写半端关掉（写任务结束）后，不立即关 fd，而是继续读、读到就丢，直到客户端关闭（读到文件末尾或出错）；最多等关闭宽限 `gateway_close_grace`（2 s），与送错误共用同一个计时。
- 进入 Ready 之后的结束不改，包括 `host/exit` 时的 `HostExiting`；握手阶段因宿主退出而结束（`HostExiting`，没有错误要送）也不等。
- 等待期间读到的数据直接丢弃：固定 1 KiB 缓冲，不缓存、不解析，内存不随对端写入增长；附带的 fd 照旧经 `recv_closing_fds` 关掉。
- **连接上限**：网关没有连接数上限。`connections` 只是给连接编号的计数，`live` 是在跑的连接任务数，`close_all` 等它归零。等待中的连接仍是一个在跑的连接任务，算在 `live` 里，占一个 fd 和 1 KiB 缓冲，最多 2 s。没有认证的连接本来就能在握手期限（10 s）内不发 `initialize` 一直占着，这 2 s 不超出已有的暴露面。
- **`participant.idle` 的含义**：它只对静默屏障（测试里的 `test/clock_advance`）有意义，表示这个连接任务在等外部（对端），不算忙；它不影响宿主退出。
- **对有序退出时长的影响**：宿主退出时，`close_all` 等所有连接任务结束。正好有一条握手被拒、还在等客户端关闭的连接时，最多再等这个连接剩下的宽限，总共不超过 2 s；这和原来送错误这一步的上限是同一个计时，最坏情况不变。正常的客户端读到文件末尾就关闭（桌面 relay 读到文件末尾即双向 shutdown，CLI 握手失败即退出），实际是毫秒级。

**设计补充**（编排者合并时改 03-protocol-v1.md，"T030 开工前的裁定"里"握手失败一律先回错误，再断开"一条）：

> 断开时宿主送完错误只半关闭写端，等客户端关闭再关自己那端，最多等关闭宽限（2 s）：客户端每次读取都比对宿主的令牌（5.8.1），宿主关掉自己那端之后读到的字节一律拒收（T215）。

## 第 2 条：重启后窗口不再连接

- 原因：渲染进程（ui-core 的 `HostProvider`）只 `open` 一次，失败就停在 `host_unreachable`。主进程这边，第一次 `open` 失败后窗口桥就关了；断开后重连失败的窗口桥也停在失败状态。之后控制连接回到 `ready`，没有人再为这些窗口建连接。ShellAPI 已冻结，不能加"请重开"的通知。
- 改法：`WindowBridges` 记下连接失败的窗口：第一次 `open` 失败（失败的是当前文档），或者断开后重连失败（窗口桥没关）。控制连接每次进入 `ready`，对这些窗口 `webContents.reload()`，然后清空记录；新文档自己 `open`。成功 `open` 的窗口从记录里去掉，正常的窗口不动。
- 窗口关掉（`destroyed`）时立即从记录里去掉，不留到下一次 `ready`：下一次 `ready` 可能永远不来，留着就一直持有已销毁的 `webContents` 的引用（第一版是这样，编排者审 diff 时问到，已改）。
- 修前的实测（T214 的重启测量）：重启后控制连接 `ready`，20 s 内 `host/status.clients` 一直没有 `window`。

## 用例

- **IT-handshake**（`gateway/tests.rs`）"refused, the host keeps its end until the client closes"：ManualClock，宽限不会到期。
  - 客户端拿到 `unauthenticated/bad_nonce` 并读到文件末尾后，等静默屏障（`settled`），断言网关的连接任务还在（`running(CONN) == 1`），客户端仍能取到宿主的令牌；
  - 客户端关闭后，审计记 `HandshakeFailed`。
  - 修前连跑 3 次都失败（`running(CONN)` 为 0：宿主已经关掉自己那端）；修后通过。
  - 原有的三处握手失败用例改成拿到错误后先 drop 客户端，再等审计的 `Closed`，否则每个都要白等 2 s 宽限。
- **UNIT-bridge-lifecycle**（`bridges.test.ts`）"windows whose connection failed load again once the control connection is back"：
  - 窗口 1 第一次 `open` 失败；窗口 2 连上后断开，重连失败；窗口 3 一直连着。
  - 窗口 4 第一次 `open` 失败后关掉了。
  - 第一次 `reload()` 只重新载入 1、2，第二次什么都不做。去掉 `destroyed` 时的清理，这个用例失败（重新载入了 1、2、4）。
- **E2E-SEC-fake-host**（`connection.spec.ts`，`@smoke`）"a host an earlier app started is restarted when asked, and the window connects"：
  - 第一个 app 带运行中的回合按关机方式退出；第二个 app 启动后弹出"需要重启会话宿主"；确认之后，窗口连上了新的宿主（pid 不同）。
