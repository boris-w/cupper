# T178 汇报（e2e 不联网，启动时就装好对话框替身）

- 执行者：子 agent（测试工程师）
- 分支：`test/T178-e2e-offline`
- 完成日期：2026-10-20

T178 已完成。两个问题都修好了：
- e2e 默认不再连公网，有检查证明普通用例跑完没有非回环连接；
- 对话框替身改由 e2e 构建的主进程在启动时装好，补上了"App 启动前套接字就被占住"的用例。

`pnpm test:e2e` 连跑 10 次全绿，跑完没有残留进程。`ci pr --record` 一次通过，没有遇到锁屏，摘要已提交。规模：新增 86 行、删除 40 行，估计约 107 行。

## 做了什么

**夹具默认不联网**（`tools/e2e/src/fixture.ts`、`helpers.ts`、`smoke.spec.ts`）
- 夹具默认的 `test_config` 加了 `runtime: { registry_url: "http://127.0.0.1:9/" }`。
- `helpers.ts` 新增 `offMachine(flows)`，从流里挑出远端不在本机的。
- 夹具自检用例（`@smoke`）在宿主启动后、建完会话后各用 `sampleNetwork` 采一次，断言没有非回环的连接。
- egress 用例改用同一个 `offMachine`。它原有的 `registry_url` 覆盖保留了：去掉它要把整个用例缩进一层，改动行数会翻倍；而且写明"这条用例依赖安装不联网"本身也有用。

**启动时就装好对话框替身**
- 新模块 `apps/desktop/src/main/e2e-dialogs.ts`，替身代码从 `tools/e2e/src/dialogs.ts` 挪过来。主进程入口的第一件事就在 `__CUPPER_FLAVOR__ === "e2e"` 分支里安装它，挂在 `globalThis.__e2eDialogs`。
- `tools/e2e` 的 `Dialogs.install` 改成 `Dialogs.attach`：不再替换，只核对替身已经在，之后照旧读取和应答。
- `apps/desktop/src/flavors.test.ts` 把 `__e2eDialogs` 加进检查的名字：e2e 构建含有它，release 和 dev 构建不含。

**E2E-SEC-fake-host 的启动时变体**（`connection.spec.ts`，`@smoke`）
- 假宿主在 App 启动前就占住套接字。
- App 启动时弹出的"需要重启会话宿主"由替身记下，窗口照常出现。
- 假宿主收到的只有带证明的 `initialize`，没有其他命令，也没有 `harness_key`。

## 关键取舍

**本地仓库选"关着的回环端口"，不起夹具仓库**
- e2e 用的是假 agent，用不到装出来的依赖树。连关着的端口会被立即拒绝，宿主按 2 s 间隔重试，流量都在回环上。
- 起一个本地仓库还得准备 tarball 夹具，这里没有收益。
- 将来要测安装本身的用例，可以用 `testConfig` 覆盖这个默认值。

**安装失败不影响别的用例**
- 界面只在 agent 本身不可用（`unavailable`、`crashed`）时才去看 `adapter.runtime`，而夹具配的是假 agent。
- e2e 里没有断言运行时状态的用例。
- 10 次连跑全套 44 项都过。

**检查确实有效**
- 临时去掉夹具里的 `registry_url` 再跑自检用例：`offMachine` 抓到宿主对外的 HTTPS 连接（本机代理分配的地址，443 端口），用例失败。加回后通过。

**替身无条件安装**
- e2e 构建只给测试用，而且启动时就要求 `CUPPER_E2E_DIR`，所以不再另设开关。
- 入口那一行在非 e2e 构建里随死分支一起被摇掉，替身模块本身没有顶层副作用。`flavors.test.ts` 会证明这一点。

## 验证方式

- **连跑**：10 次 `pnpm test:e2e`（持机器锁，两段跑法）。
  - 每次 26 + 18 = 44 项全过，包括 T092 合并进来的 `security.spec.ts`。
  - 每次 111–118 s；第 1 次等了别的 worktree 的锁，用了 254 s。
  - 每次跑完，本 worktree 的 e2e App、宿主、假 agent、`nc` 都没有残留，`$TMPDIR/cupper-e2e/` 为空。
  - 屏幕没锁，显示器醒着。
- **单项**：
  - `connection.spec.ts` 的 5 条都通过，启动时的变体 0.7 s 跑完（T176 时这种情形要等 60 s 超时）。
  - `flavors.test.ts` 通过。
  - ESLint、prettier、`tsc`（`apps/desktop`、`tools/e2e`）都通过。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T178-e2e-offline/ci/`，一次通过，摘要 `pr-6e7d2f2561be.json` 已提交。跑的期间没有往工作区写文件。

  | 作业 | 结果 |
  |---|---|
  | rust | 2,005 通过 |
  | protocol | 185 通过 |
  | ts | 473 通过 |
  | license | 通过 |
  | testdata-scan | 3 通过（本机没有敏感词表，那一步跳过） |
  | ui | 按路径筛选跳过 |
  | e2e | 30 通过（`@smoke` 两段） |

- **试合并**：用 `git merge-tree` 试合并最新的 main，没有冲突。
- **规模**（不含 notes）：

  | 模块 | 新增 | 删除 | 估计 |
  |---|---|---|---|
  | 夹具默认不联网与检查 | 14 | 4 | 25 |
  | 主进程里的替身 | 54 | 36 | 62 |
  | 启动时的变体 | 18 | 0 | 20 |
  | 合计 | 86 | 40 | 107 |

## 提交列表

```
a50d3c0 docs(T178): record the ci pr summary
6e7d2f2 docs(T178): record the size per module and the runs
8f446ca test(e2e): E2E-SEC-fake-host with the socket taken before the app starts
35e3f5b feat(desktop): the e2e build's main process stands in for dialogs from its start
f5c13fb test(e2e): keep the runtime install off the network by default
d0ba442 docs(T178): record the checks, the approach and the size estimate
```

## 遗留问题 / 范围外发现

- **egress 用例里的覆盖已经多余**：它的 `registry_url` 覆盖和夹具默认值相同。以后改那条用例时可以顺手去掉。
- **CLI 的弱校验**：e2e 构建的 CLI 不带密钥时只做弱校验，按任务书另议，这次没有动。

按你的安排，交完这份汇报我就停下，不再开始别的工作。
