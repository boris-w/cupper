# T178 e2e 不联网，启动时就装好对话框替身

- 派发日期：2026-10-20
- 执行者：子 agent（测试工程师）。T176 的执行者接着做（发现了这两个问题）
- 分支：`test/T178-e2e-offline`
- 依赖：T176（合并中）

## 背景

T176 发现两个 e2e 设施的问题：
1. **所有 e2e 用例都会连公网的 npm 仓库**。夹具没有设 `test_config.runtime.registry_url`，每个用例启动的宿主都按运行时安装对 registry.npmjs.org 建约 8 条 HTTPS 连接，去下载 claude-agent-acp 的依赖树，尽管用的是假 agent。测试不应联网（SEC-6；e2e 的结果也不该受网络影响）。
2. **启动时弹出的对话框接不住**。T090、T167 的对话框替身在 `_electron.launch` 之后才经 `app.evaluate` 装上，App 在 `whenReady` 时就弹的对话框（例如宿主校验失败时的"需要重启会话宿主"）在 e2e 里是真的原生对话框，窗口要等它关掉才出现。所以"启动时套接字就被占住"这条路径测不了。

## 范围

**做**：
1. **夹具默认不联网**：
   - `tools/e2e/src/fixture.ts` 默认的 `test_config` 设 `runtime.registry_url`，指向回环上一个关着的端口，或者本地的夹具仓库；选哪种、为什么，在汇报里说明。
   - 确认运行时安装失败的状态不影响别的用例的界面，例如侧边栏的宿主状态、横幅、agent 可用状态。有影响的，用例里按需覆盖。
   - 用 T176 的 `sampleNetwork` 加一条检查：一个普通用例跑完，App 进程树里没有非回环的连接。可以并进夹具自检，或者在 `@smoke` 里加一条。
2. **启动时就装好对话框替身**：
   - 替身要在 App 的主进程弹出第一个对话框之前就位。做法由你定，例如：
     - e2e 构建的主进程入口读一个只在 e2e 构建里存在的开关，启动时就装上一个记录并挂起的替身；
     - 或者通过 `_electron.launch` 的参数注入一个预加载到主进程的脚本。
   - 发布构建、开发构建里不能有这条路径，`apps/desktop/src/flavors.test.ts` 要能证明。
   - 补一条 E2E-SEC-fake-host 的变体：App 启动前假宿主就占住套接字，App 不发令牌，弹出的重启确认由替身记下。

**不做**：e2e 构建的 CLI 不带密钥时的弱校验（另议）；其他 e2e 用例。

## 验收标准

- [ ] 夹具默认不联网，有检查证明普通用例跑完没有非回环连接。
- [ ] 启动时就装好替身；"启动时套接字就被占住"的用例通过；`flavors.test.ts` 证明这条路径不进非 e2e 构建。
- [ ] 经 `pnpm test:e2e` 连跑 10 次全绿，跑完没有残留进程；屏幕没锁、显示器醒着（T093 的前提）。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T178-e2e-offline/ci/` 通过，摘要随任务提交。

## 交接事项

- T176：`raw.ts` 的假宿主、`sampleNetwork` 与正对照、E2E-SEC-fake-host 的"接管"做法。
- T090、T167：`dialogs.ts` 的替身、`fixture.ts`。
- T093：e2e 一律经 `pnpm test:e2e`、`test:e2e:one` 持锁运行。
- 设计 5.9.9 的测试接口表：`test_config.runtime.registry_url` 只在 test-harness 构建里生效。

## 注意

- 测试本身不向外网发任何请求；外发采样只采本用例的进程树。
- 生产代码里不加测试分支，测试入口只在 e2e 构建里存在。

## 规模

S，估约 150 行。**开工前先在 `notes.md` 里按模块写行数估计；推算超过约 200 行就先停下汇报**；硬停止线 240。每写完一个模块统计一次。

## 工作方式

T176 合并之后，在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b test/T178-e2e-offline main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
