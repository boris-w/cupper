# T178 工作笔记

## 开工前的核对

- 交接：T176 的 `raw.ts`（假宿主）、`sampleNetwork` 与正对照、"接管"做法；T090、T167 的 `dialogs.ts`、`fixture.ts`；T093 的持锁入口。
- 宿主的运行时安装失败时按 `runtime_retry`（2 s）重试；界面只在 agent 本身不可用（`unavailable`、`crashed`）时才看 `adapter.runtime`，夹具的 agent 是假 agent，不受影响。e2e 用例里没有断言运行时状态的。
- 对话框替身现在由 `Dialogs.install` 经 `app.evaluate` 装上，晚于主进程在 `whenReady` 时弹出的对话框。

## 做法

- 夹具默认的 `test_config` 设 `runtime.registry_url: "http://127.0.0.1:9/"`：回环上关着的端口，连接立即被拒，不需要起一个夹具仓库。e2e 用的是假 agent，用不到安装出来的依赖树；要测安装本身的用例可以用 `testConfig` 覆盖。
- 替身挪进 e2e 构建的主进程：主进程入口第一件事就装上，挂在 `globalThis.__e2eDialogs`；`tools/e2e` 的 `Dialogs` 只负责读和应答。代码只在 `__CUPPER_FLAVOR__ === "e2e"` 的分支里引用，其他构建摇树后不含；`flavors.test.ts` 把 `__e2eDialogs` 加进检查的名字。
- 不联网的检查：`helpers.ts` 加 `offMachine`（`sampleNetwork` 里非回环的流），夹具自检的 `@smoke` 用例在宿主启动后、建会话后各采一次，断言为空；egress 用例改用它。

## 规模估计（开工前）

| 模块 | 内容 | 估计 |
|---|---|---|
| 夹具默认不联网 | `registry_url` 与注释 | 5 |
| 不联网的检查 | `offMachine` 辅助；自检用例两次采样；egress 用例去掉覆盖、改用辅助 | 20 |
| 主进程里的替身 | 新模块（从 `dialogs.ts` 挪来）、入口一行 | 50 |
| `dialogs.ts` | 只读和应答（删约 30 行） | 8 |
| `flavors.test.ts` | 加一个名字、改注释 | 4 |
| E2E-SEC-fake-host 启动时的变体 | 假宿主先占住套接字再启动 App | 20 |
| 合计 | | 新增约 **107**，删除约 45 |

停止线约 200，硬停止线 240。每写完一个模块统计一次，记在下面。

## 按模块统计

每个模块提交后用 `git diff --numstat` 统计（不含 notes）：

| 模块 | 新增 | 删除 | 估计 | 累计新增 |
|---|---|---|---|---|
| 夹具默认不联网与检查 | 14 | 4 | 25 | 14 |
| 主进程里的替身（新模块、入口、`dialogs.ts`、`flavors.test.ts`） | 54 | 36 | 62 | 68 |
| E2E-SEC-fake-host 启动时的变体 | 18 | 0 | 20 | 86 |

合计新增 86、删除 40，在约 200 的停止线以内。egress 用例保留了自己的 `registry_url` 覆盖，只改用共用的 `offMachine`：去掉覆盖要把整个用例缩进一层，改动行数会翻倍，而写明"本用例依赖安装不联网"也有用。

## 过程记录

- 检查有效：夹具去掉 `registry_url` 临时跑自检用例，`offMachine` 抓到宿主对 198.18.0.51:443（本机代理分配的地址）的 HTTPS 连接，用例失败；加回后通过。
- 安装失败对界面没有影响：界面只在 agent 本身不可用时才看运行时状态；夹具配的是假 agent。10 次连跑全部用例都过。
- 启动时的变体：替身就位后，App 启动时弹出的重启确认被替身记下，窗口照常出现；用例 0.7 s 跑完（T176 时这种情形要等 60 s 超时）。
- 10 次 `pnpm test:e2e`（持机器锁，两段）：每次 26 + 18 = 44 项全过，111–118 s（第 1 次等了别的 worktree 的锁，254 s）；每次跑完本 worktree 没有残留进程，`$TMPDIR/cupper-e2e/` 为空。
