# T214 笔记

## 规模

停止线 120，硬停止线 160（`cargo xtask size`，含测试）。写完第一版、提交之前量到 +129，超出停止线 9 行，先停下来报给编排者。编排者选了完整方案（保留"一次一条连接"和它的用例），停止线提到 140，硬停止线不变（brief 的补充裁定，main df68e1c）。

| 提交 | 内容 | 累计（`size --rev`） |
|---|---|---|
| 42cc8c8 | 控制连接一次一条 + 单测 | +48 −2 |
| d7af184 | 决定退出后不再连接、不再拉起 + 单测 | +102 −15 |
| 94a18c4 | E2E-quit-confirm 新用例 | +129 −16 |

## 实测

都在本机单份跑：e2e 构建（`pnpm build:e2e`）、Playwright 一个 worker，假 agent。临时的测量 spec 没有提交。下面的时间取自 `main.log`（主进程）和 `host.log`（宿主），只保留秒和毫秒。

### 1. 确认退出后会拉起新宿主（修前）

**做法 A：让 app 在决定退出后停一会儿**（新 e2e 用例的做法）：弹出退出确认框后，测试在主进程里挂一个只生效一次的 `before-quit`，把决定之后的那次 `app.quit()` 拦住；然后点"退出"。

```
main.log
  31.545 host connection ready
  31.611 window connected
  32.671 host connection reconnecting      ← 宿主关连接，控制连接立即重连
  32.695 host connection ready             ← 连上了新宿主
  32.695 window connected                  ← 窗口也跟着连上新宿主
host.log
  31.381 the host started
  32.228 orderly exit {exit: Requested { force: true }}
  32.676 the host exits {code: 0}
  32.683 the host started                  ← 新宿主，拿到了锁
```

用例失败时进程表里多一个 `cupper-host --data-dir <本用例的数据目录>`，父进程是 app。

**做法 B：不拦退出**（和已有的"yes ends the host"一样，确认后等 app 退出）：连跑 3 次，3 次都拉起了两个宿主。

```
main.log                                     host.log
  57.314 host connection reconnecting          57.211 orderly exit {Requested { force: true }}
  57.315 host connection closed  (will-quit)   57.226 the host exits {code: 0}
                                               57.320 the host cannot run: another host holds …/host.lock {code: 3}
                                               57.326 the host started
                                               58.327 orderly exit {Orphaned}
```

- `host.lock` 里的 pid 换成了新宿主的。
- 新宿主在 app 退出后约 1 s 按"孤立"规则退出。这条规则只在 `dev-auth` 构建里有（dev、e2e、自用构建）；**发布构建没有它，新宿主会留到空闲退出（10 分钟）**。
- 两个宿主是两次 `launcher.open`：控制连接的重连一次，窗口桥的重连一次（见第 2 条）。另一种结局也见过：新宿主比旧宿主释放锁早，以退出码 3 退出，launcher 等满 500 ms 报 `host_unreachable`，控制连接停在 `failed`（修前跑最终版 e2e 用例时的那一次：`reconnecting` → `failed`）。哪种结局取决于新宿主抢锁和旧宿主放锁的先后，但"重连并拉起"每次都发生。

### 2. 窗口连接断开后会重连

- 读代码：`WindowBridge.#lost` 先 `await controlReady()`，再 `connect()`（`openWindow` → `launcher.open`）。控制连接还没察觉断开时（仍是 `ready`），`controlReady()` 立即返回，窗口就自己去连、自己拉起宿主；控制连接已在重连时，窗口等它 `ready` 之后再连。
- 做法 A 的日志：控制连接连上新宿主的同一毫秒，`window connected`，窗口连上了新宿主。
- 做法 B：两个宿主，只有一次控制连接的 `reconnecting`；另一次 `launcher.open` 只能来自窗口桥（`launcher.open` 只有控制连接和 `openWindow` 两个调用方，重启宿主没被触发）。
- 修后同样不拦退出跑 3 次：每次都在 `host connection closed` 之前（同一毫秒）有一条 `window connection failed`，即窗口的连接先断、它照样去连，被关掉的 launcher 拒绝（`closed`），没有拉起宿主；`host.lock` 还是旧 pid，`host.log` 只有一条 `the host started`。

### 3. 重启宿主只建一次连接

- 读代码：重启只由控制连接的 `failed`（`unauthenticated`、`host_unverified`）触发。这两种失败来自 `launcher.open` 抛错，那时还没有挂上 socket，也就没有 `close` 监听；SIGTERM 让旧宿主退出，触发不了 `close` 里的重连。流程最后显式的 `await control.connect()` 是唯一的一次连接。
- 实测：第一个 app 带着运行中的回合按关机方式退出（宿主留着），再启动第二个 app（新的 `HostKey`）。

```
main.log（第二个 app）
  17.230 host connection failed unauthenticated
  17.360 window connection failed (closed)
  17.362 restarting the host {outcome: stopped}
  17.413 host connection connecting        ← 只有这一次
  17.426 host connection ready
host.log
  17.362 orderly exit {Terminated}
  17.376 the host exits {code: 0}
  17.423 the host started                  ← 只多这一个
```

- 结论：现有的触发方式下只建一次连接。但 `restart` 是 `startHost` 导出的（T007 交接说界面以后可能经 ShellAPI 触发它），换个触发时机就可能在挂着连接时 SIGTERM，于是"close 里的重连"和"流程最后的 connect"并存。所以把"同一时间只有一条连接"做成 `ControlConnection` 自己的不变式，并用单测守着。

## 修法

- **`ControlConnection.connect()` 一次一条**：已连上或正在连时，返回那一次的 promise，不再 `launcher.open`。连接失败或断开时清掉。
- **`ControlConnection.end()`**：退出已决定。之后连接断开就 `client.close()`，不再重连；同时 `launcher.close()`。`close()`（`will-quit`）也先 `end()`。
  - `client.close()` 让还在等的 `host/exit` 立即作废（`lost`）：修前它在断开时重新排队，要么等满 3 s 超时，要么重连后把同一个 `command_id` 发给新宿主。
- **`HostLauncher.close()`**：之后 `open()` 直接抛 `HostFailure("closed")`，不拨号、不拉起。这一层挡住窗口那条路：窗口的断开先于控制连接被处理时，`controlReady()` 还会放行。
- **`QuitGuard`**：`QuitHost` 加 `end()`。决定退出时（发 `host/exit` 之前；不需要问时也一样）调 `end()`；`skip()`（关机、安装更新）也调。取消退出时不调。判定规则（什么时候问、问什么）没有改。
  - `skip()` 本来就是永久的（之后的退出都不再问），所以在 `skip()` 时结束连接与它一致。

**为什么不只在 `will-quit` 之前关**：从决定退出到 `will-quit` 之间要关窗口，时长不定；宿主一收到 `host/exit` 就会关连接，所以"不再重连"必须在 `host/exit` 发出之前生效。

**为什么还要 launcher 这一层**：只停控制连接的重连，窗口桥仍会在控制连接还是 `ready` 时自己经 launcher 拉起宿主（第 2 条）。launcher 是唯一会拉起宿主的地方，在它这里拦最彻底。

## 用例与修前修后对照

- **UNIT-launcher**（`control.test.ts`，假 launcher、假 socket）：
  - "once quitting is decided, a dropped control connection is not made again"
  - "connecting while a connection is up or being made opens no other"
- **UNIT-host-restart**（`restart.test.ts`）：记录对宿主的调用顺序，断言决定退出时先 `end` 再 `exit(force)`；不需要问时只 `end`；取消时什么都不调；关机、更新时 `end`。
- **E2E-quit-confirm**："once quitting is confirmed, the host's exit starts no other host"。做法 A；等旧宿主的进程树消失，再等控制连接在 `main.log` 里的状态，断言决定之后只有 `closed`，数据目录上没有宿主进程，`host.lock` 还是旧 pid。
- 修前（产品代码换回 c0d1cc5，单测里给旧的 `ControlConnection` 补一个空的 `end()` 只为能编译）：
  - 新单测 5 例失败：`opens: 2`（断开后重连了）、`3 ≠ 1`（并发 connect 开了两条）；`QuitGuard` 的 3 例没有 `end`。
  - 新 e2e 失败：决定后的状态为 `["reconnecting", "ready"]`（多一个宿主进程）或 `["reconnecting", "failed"]`（新宿主以退出码 3 退出）。
- 修后：桌面单测 99 个全过；新 e2e 本机单跑通过；`app.spec.ts`、`connection.spec.ts`、`nfr1-restart.spec.ts` 中非 `@focus` 的 11 个用例本机单份跑全过。

## 范围外的发现（没有改）

1. **握手被拒时，桌面有时只看到连接关闭**：第二个 app 连旧宿主时，控制连接或窗口连接的失败原因有时是 `closed` 而不是 `unauthenticated`（重启宿主的测量 3 次里有 2 次控制连接报 `closed`）。宿主回了错误就关连接，扩展在宿主关闭之后读到的字节按 5.8.1 拒收，看起来和 T213 是同一件事，只是在桌面这边。后果：控制连接报 `closed` 时**不弹"需要重启宿主"**，新的 app 一直连不上，也不再重试。
2. **重启宿主之后窗口不再连接**：重启前窗口的第一次 `open` 已经失败（`unauthenticated` 或 `closed`），重启后控制连接 `ready` 了，但没人再为窗口发起 `open`，20 s 内窗口一直没连上（`host/status.clients` 里没有 `window`）。
3. 修后，退出过程中窗口会先收到 `failed: closed` 再随 app 关掉；`main.log` 里多一条 `window connection failed`（warn）。只在退出时出现，不影响用户。
