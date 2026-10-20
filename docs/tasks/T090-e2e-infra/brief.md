# T090 e2e 基础设施

- 派发日期：2026-10-20
- 执行者：子 agent（测试工程师）。T043、T055、T008、T116、T133、T046 的执行者接手（熟悉假 agent、宿主的测试接口、CLI 与提权探针）
- 分支：`feat/T090-e2e-infra`
- 依赖：T007、T019、T046、T077、T081、T134（已合并或合并中），T029（并行进行中）。T091（NFR-1 验收套件）、T092、T093 依赖本任务。**本任务在剩余关键路径上**：T090 → T091 → T099

## 背景

桌面端的各部分已经齐了：
- 主进程拉起宿主、真实输入、特权操作与原生确认框（T075、T076、T007、T077、T134）；
- 界面的外框、侧边栏、输入框（T084、T081、T086 进行中）；
- 宿主的测试接口（T047）、CLI（T046）、提权探针（T133）。

但还没有一个能把真实的 Electron App、真实宿主、假 agent、CLI 串起来跑的 e2e 设施。本任务建这套设施，并一次给齐后续 e2e 任务（T091–T093）要用的辅助函数。

## 范围

**做**（计划 T090 一行；设计 5.9.4 的环境、5.9.9；计划 1.8 的机器级锁与 Playwright workers）：
1. **Playwright `_electron` 夹具**：
   - e2e 构建的 App（`CUPPER_E2E_DIR` 布局，T074 的交接）；
   - 每个用例一个临时根目录；临时 git 仓库；假 agent（经 `test_config.agents` 注入）；
   - CLI 以 `--harness-key-fd` 接入（T046）。
2. **一次给齐辅助函数**（照 5.7.8 的"测试"一节和各任务的交接）：
   - 原生确认框的替换与记录（在主进程里替换 `dialog.showMessageBox` 等，记录调用参数）；
   - `window.__cupperTest`（例如导出 `client.counters`，T087 的交接）；
   - 按窗口断连；事件屏障（经宿主的静止屏障与 `test/clock_advance`）；
   - `kill -9` 宿主；CLI 提权（照 IT-elevate、IT-cli-commands 的做法）；外发采样；
   - 按键：经 `webContents.sendInputEvent` 发出，走生产的 inputGuard（Q26 的结论，T077 的交接）。Playwright 的键盘输入不会被记成真实输入；
   - 请求面板的辅助：等 `data-ready`、按数字键、按 Tab 激活（面板由 T083 做，辅助先按设计写好）。
3. **套件与标签**：`@smoke` 标签与 PR、每晚套件的对应；Playwright workers 固定为 2。
4. **`cargo xtask ci e2e`**：
   - e2e 的 App 和 `CupperHost.app` 由当前提交构建，摘要记下产物哈希；
   - 取机器级锁；
   - 接进 `ci pr`（只跑 `@smoke`）和 `ci nightly`（全集）。
5. **清理**：
   - 每轮测试后清理 Chromium 写到 `~/Library/Preferences/dev.cupper.app.e2e.plist` 的系统默认值（T074 的交接）；
   - 用例结束时宿主、假 agent、CLI 都要被回收，照 T008 修复 1 的守卫做法。

**不做**：NFR-1 的五个用例（T091）；安全 e2e（T092）；交互与项目 e2e（T093）。

## 验收标准

- [ ] E2E-single-instance、E2E-quit-confirm（T007 的交接：需要运行中的回合或待处理的请求，再在主进程里替换确认框；空闲时退出 App 不发 `host/exit`）。
- [ ] 每个辅助函数至少被一条自检用例调用过，确认能用（可以不登记编号，写进 notes）。
- [ ] `cargo xtask ci e2e` 通过；`ci pr` 里的 `@smoke` 子集通过。
- [ ] 新增的 E2E 用例连跑 10 次全绿（workers 为 2）；跑完没有残留进程。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T090 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T090-e2e-infra/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T090 的行；开工前逐行读一遍）

- T074：`CUPPER_E2E_DIR` 的布局；plist 的清理。
- T075、T016：`build:dev`、`build:e2e`、`test:launch` 都需要本机先跑过一次 `cargo xtask runtime-pack`（要联网下载 Node 等公开物料，请求里不带个人信息）。
- T007：E2E-quit-confirm 的做法。
- T077：Q26 的结论；按键经 `sendInputEvent`。
- T133、T046：提权探针；CLI 的退出码；`--json` 下 `elevate` 被拒时输出两行；手动时钟下索引要先推进超过 100 ms 才看得到新会话。
- T059：中断回合的回复可能因待升级退出而丢失，不要断言一定收到。
- T118：截图回归用的是 Playwright 自带的 Chromium，和 e2e 的 Electron 不是一个引擎。
- T008 修复 1、T063：进程级用例不留残留进程的守卫做法。

## 注意

- 不运行真实的 Claude Code、Codex CLI，不登录；agent 一律用假 agent。
- 生产代码里不加测试分支；测试用的入口只在 e2e 构建里存在（照 `CUPPER_E2E_DIR` 的做法）。
- 不读、不改用户的 `~/.claude`、`~/.codex`；e2e 的数据目录全部在临时目录里。

## 规模

计划估为 M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项；推算超过约 900 行（包括贴线）就开工前停下汇报，并给出拆分建议（例如设施与 `xtask ci e2e` 一份，辅助函数一份）。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

## 工作方式

- T046 合并之后，在原来的 T043 worktree 里从最新的 main 开新分支：`git checkout -b feat/T090-e2e-infra main`。T046 的分支由编排者合并，不要再往它上面提交。
- T029（blob）在并行进行。设施本身不依赖它；用到 blob 的辅助等 T029 合并后，由编排者把 main 合进来再补。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 1,545 行（实现约 1,115、测试约 430），在写代码之前停下，建议拆成两份。

**裁定：方案一（计划 v1.3.36）。**
- **T090 收窄为**：
  - 套件骨架（新的工作区包 `tools/e2e/`，根目录 `pnpm test:e2e`）、夹具、CLI 包装、原生确认框的替换与记录、`xtask ci e2e`；
  - E2E-single-instance、E2E-quit-confirm、`@smoke` 夹具自检、xtask 单元测试。
  - 上限约 1,000 行，硬停止线 1,100。
- **新增 T167（e2e 辅助函数）**：`__cupperTest`、CLI 的 `clock-advance`、按窗口断连、事件屏障、`kill -9`、CLI 提权、外发采样、按键、请求面板，加各自的自检。由同一个执行者接着做。T091、T092、T093 改为也依赖它。

**七条确认**：
1. **`__cupperTest` 的开口**：给 `mount` 加可选的 `onClient(client)`，只在 e2e 构建的渲染进程入口传入，发布构建经 Vite `define` 去掉（T167 做）。发布构建里不能留这条路径，用例或构建检查要证明这一点。
2. **CLI 加 `clock-advance <ms>`**：只在 test-harness 构建里有（T167 做）。认可。
3. **请求面板的辅助先写、自检后补**：认可。T083 快合并了，T167 开工时多半已经能做自检。
4. **外发采样只给采样器**，`nettop` 能否作验收归 T092。认可。
5. **`@smoke` 只放一条夹具自检**（App 起来、UI 连上、CLI 看得到宿主）；两个 E2E 编号留在每晚的全集。认可。
6. **`tools/e2e/` 与 `pnpm test:e2e`**：认可。
7. **`ci e2e` 的构建**：
   - `runtime-pack` 的暂存目录不在时先跑一次（只下载公开物料，请求不带个人信息）；
   - App 和 `CupperHost.app` 每次由当前提交构建，目录摘要写进 `artifacts`；
   - `ci pr` 只跑 `@smoke`，照 `ui` 作业按改动路径过滤。
   - 认可。
