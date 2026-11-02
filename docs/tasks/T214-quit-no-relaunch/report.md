# T214 汇报

- 执行者：子 agent（TypeScript / Electron 工程师）
- 分支：`fix/T214-quit-no-relaunch`
- 完成日期：2026-11-02
- PR：https://github.com/boris-w/cupper/pull/30 。CI 运行 https://github.com/boris-w/cupper/actions/runs/41386791153 （提交 7ba27e5，第 1 次 attempt，8 个检查全部通过）。
- e2e-repeat：https://github.com/boris-w/cupper/actions/runs/41386846044 （7ba27e5，120 次全部通过）。

**状态**：完成。T213 读代码时的推测成立：确认退出后桌面会再拉起宿主，窗口连接也会跟着重连。已经修复，并加了用例。

## 做了什么

**三件事的实测结论**（都在本机单份跑，e2e 构建，假 agent；证据取自 `main.log`、`host.log` 和 `host.lock`，详细日志在 `notes.md`）

1. **确认退出后会拉起新宿主。**
   - 做法 A（新 e2e 用例的做法）：决定退出之后，让 app 晚一点退。
     - 宿主有序退出、关掉连接后，控制连接立即进入 `reconnecting`，app 拉起了新宿主。
     - `host.log` 里第二次出现 "the host started"，新宿主拿到了锁，`host.lock` 换成它的 pid。
     - 窗口在同一毫秒也连上了新宿主。
   - 做法 B：不拦退出，连跑 3 次，每次都拉起两个宿主。
     - 一个以退出码 3 退出（锁还在旧宿主手里）。
     - 另一个拿到锁，app 退出约 1 s 后按 `dev-auth` 构建的"孤立"规则退出。
     - **发布构建没有孤立规则，新宿主会一直留到空闲退出（10 分钟）。**
   - 还见过另一种结局：新宿主抢锁早于旧宿主放锁，以退出码 3 退出，控制连接最后报 `failed: host_unreachable`。每种结局都先重连、再拉起宿主。
2. **窗口连接断开后会重连。**
   - 控制连接重连上之后，窗口马上也连上（`window connected`）。
   - 主进程有时先处理到窗口连接的断开，这时控制连接还是 `ready`，`controlReady()` 直接放行，窗口就自己经 launcher 拉起宿主。做法 B 里那"第二个宿主"就是这样来的：`launcher.open` 只有控制连接和 `openWindow` 两处调用。
   - 修复后的旁证：不拦退出再跑 3 次，每次在控制连接 `closed` 之前都有一条 `window connection failed`，说明窗口确实去连了，被关掉的 launcher 拒绝了。
3. **重启宿主时，目前只建一次连接。**
   - 读代码：重启只在控制连接以 `unauthenticated` 或 `host_unverified` 失败之后触发。这时还没有挂上 socket，SIGTERM 触发不了 `close` 里的重连。
   - 实测：第一个 app 带着运行中的回合按关机方式退出，宿主留着；再启动第二个 app，它的 key 是新的。
     - 结果：`failed unauthenticated` → `restarting the host {stopped}` → 只有一次 `connecting` → `ready`。
     - `host.log` 只多了一条 "the host started"。

**修复**（`apps/desktop/src/main`）
- `ControlConnection.connect()` 同一时间只有一条连接：已经连上或正在连时，再调用就返回同一个 promise，不再调 `launcher.open`。
- `ControlConnection.end()`：表示已决定退出。
  - 之后连接一断就 `client.close()`，不再重连。还在等的 `host/exit` 随之作废，不会等满 3 s，也不会带着同一个 `command_id` 重发给新宿主。
  - 同时调用 `HostLauncher.close()`，之后 `open()` 直接抛 `HostFailure("closed")`，不拨号、不拉起。
  - `close()`（`will-quit` 时）也先调 `end()`。
- `QuitGuard`：`QuitHost` 加了 `end()`。
  - 决定退出时调用，在发 `host/exit` 之前；不需要问用户、直接退出时也调用。
  - `skip()`（关机、安装更新）也调用；用户取消退出时不调用。
  - 判定规则（什么时候问、问什么）没有改。
- `index.ts` 的 `quitHost()` 补上 `end: () => host?.control.end()`。

**用例**
- **UNIT-launcher**（新文件 `host/control.test.ts`，假 launcher、假 socket）：
  - 决定退出后，连接断开不再重连；挂着的 `host/exit` 作废；launcher 已关闭。
  - 已连上或正在连时，再调 `connect()` 不会开第二条连接。这例守着"重启宿主只建一次连接"。
- **UNIT-host-restart**（`restart.test.ts`）：按顺序记录对宿主的调用，断言：
  - 需要确认时是先 `end` 再 `exit(force)`；
  - 不需要问时只有 `end`；
  - 取消时什么都不调；
  - 关机、更新时调 `end`。
- **E2E-quit-confirm**：新增 "once quitting is confirmed, the host's exit starts no other host"。
  - 做法：弹出确认框后，挂一个只生效一次的 `before-quit` 拦住决定之后的那次退出，再点"退出"。
  - 等原宿主的进程树消失后断言：
    - 决定之后，控制连接的状态只有 `closed`；
    - 数据目录上没有宿主进程；
    - `host.lock` 还是原来的 pid。

## 关键取舍

- **"不再重连"必须在 `host/exit` 之前生效**：宿主一收到 `host/exit` 就会关连接，而从决定退出到 `will-quit` 之间要关窗口，时间长短不定。所以由 `QuitGuard` 在决定退出的那一刻调用 `end()`，不能等 `will-quit`。
- **还要在 launcher 这一层拦**：只停控制连接的重连还不够，窗口桥会在控制连接仍是 `ready` 时自己经 launcher 拉起宿主。launcher 是唯一拉起宿主的地方，在这里拦最彻底。
- **断开时用 `client.close()`，不用 `fail`**：`fail` 会把命令留在队列里。改成关闭后，读不到应答时（T213 那种情况）不再空等 3 s。
- **`skip()` 也结束连接**：`skip()` 本来就是永久的，之后的退出都不再问，所以在这时结束连接和它一致。
- **"一次一条连接"做成 `ControlConnection` 自己的规则**：目前重启宿主只建一次连接靠的是触发时机。但 `restart` 是导出的，T007 交接说界面以后可能经 ShellAPI 触发它；换个时机，就可能在挂着连接时发 SIGTERM。把规则放在连接对象里，用例能直接守住。
- **e2e 让 app 晚一点退**：这样结果是确定的。直接退出时会不会出问题要看时序。用例判断的是"决定之后有没有出现 `reconnecting`"，所以不管新宿主是拿到了锁还是以退出码 3 退出，修前都会失败。
- **规模**：`cargo xtask size` 为 +129 −16，超过原停止线 120。提交前停下来报给你，你选了方案 A（完整方案），停止线提到 140。各提交的累计规模：
  - 42cc8c8：+48 −2
  - d7af184：+102 −15
  - 94a18c4：+129 −16

## 验证方式

**修前修后对照**
- 单测：产品代码换回 c0d1cc5（给旧的 `ControlConnection` 临时补一个空的 `end()`，只为能编译）。
  - 修前，新增和改动的 5 例失败：
    - 断开后重连了：`opens: 2`；
    - 并发调 `connect()` 开了两条：`3 ≠ 1`；
    - `QuitGuard` 的 3 例没有调 `end`。
  - 修后全部通过。
- e2e：修前用最终版用例跑两次都失败。
  - 一次状态是 `["reconnecting","ready"]`，多了一个宿主进程；
  - 一次是 `["reconnecting","failed"]`，新宿主以退出码 3 退出。
  - 修后本机单跑通过。

**runner 与 CI**
- e2e-repeat：spec `src/app.spec.ts`，test "E2E-quit-confirm: once quitting is confirmed"，120 次，workers 1。
  - 运行：https://github.com/boris-w/cupper/actions/runs/41386846044
  - 下载 `repeat.json` 核对：expected 120，unexpected 0，flaky 0。
- PR #30 的 8 个检查第 1 次 attempt 全部通过：compat-framework、e2e、license、protocol、rust、testdata-scan、ts、ui。
  - 运行：https://github.com/boris-w/cupper/actions/runs/41386791153

**本机检查**（都只跑一份）
- 桌面单测 99 个通过。
- `app.spec.ts`、`connection.spec.ts`、`nfr1-restart.spec.ts` 里非 `@focus` 的 11 个用例通过。
- 改动范围内 prettier、eslint、`tsc --build` 都通过。
- 没有跑 `@focus` 用例，没有运行真实的 Claude Code 或 Codex CLI。
- 临时的测量 spec 已删除，没有残留进程，工作区干净。

**验收标准**
- [x] 三件事的实测结论和证据写进了汇报和 `notes.md`。
- [x] 确认退出后不会拉起新宿主；重启宿主只建一次连接；都有用例守着。
- [x] 新 e2e 用例在 runner 上连跑 120 次，全部通过。
- [x] PR #30 的 8 个检查全部通过。

## 提交列表

```
7ba27e5 docs(task): T214 notes on the measurements, the fix and the comparisons
94a18c4 test(e2e): E2E-quit-confirm, the host's exit starts no other host
d7af184 fix(desktop): once quitting is decided, connect no more and start no host
42cc8c8 fix(desktop): the control connection makes one connection at a time
```

## 遗留问题 / 范围外发现

1. **握手被拒时，桌面有时只看到连接关闭，"需要重启宿主"不弹**（需要另开任务）。
   - 第二个 app 去连前一个 app 拉起的宿主时，失败原因有时是 `closed`，不是 `unauthenticated`。测重启时 3 次里控制连接有 2 次报 `closed`，窗口连接也见过。
   - 控制连接报 `closed` 时不弹重启确认，新的 app 一直连不上，也不再重试。
   - 推测：宿主回完错误就关连接，扩展在宿主关闭之后读到的字节按 5.8.1 被拒收。看起来和 T213 是同一件事，只是发生在桌面这一侧。
2. **重启宿主之后，窗口不再连接**。重启前窗口的第一次 `open` 已经失败，重启后控制连接恢复了 `ready`，但没人再为窗口发起 `open`。20 s 内 `host/status.clients` 里一直没有 `window`。
3. **发布构建没有"孤立"规则**：修复之前，发布构建里退出时被拉起的宿主会一直留到空闲退出；修复之后不会再拉起。
4. 修复后，退出过程中窗口会短暂显示 `failed: closed`，`main.log` 多一条 `window connection failed`（warn）。只在退出时出现，不影响用户。
5. `cargo xtask size` 在这个分支上仍按旧的停止线 120 提示超出 9 行，按裁定以 140 为准。
