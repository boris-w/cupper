# T090 工作笔记

- 执行者：子 agent（测试工程师；T043、T055、T008、T116、T133、T046 的执行者）

## 开工前读过的

- 任务书；计划 T090 一行和 1.8（机器级锁、workers 为 2）；设计 5.9.4（环境、DOM 标记）、5.9.9（注入点）、5.2.16（`test/clock_advance` 的静止语义）、5.1.5 和 5.1.6（`pnpm test:e2e`、e2e 作业按改动路径过滤）。
- `handoff-notes.md` 第一列含 T090 的 12 行：T074 两行、T051、T075、T007、T077、T118、T038、T133、T059、T086、T046。
- 桌面端：
  - `main/index.ts`：e2e 构建读 `CUPPER_E2E_DIR`；主进程的钩子 `globalThis.__cupperE2E.disconnectWindow(webContentsId)` 已有。
  - `main/host/index.ts`：`e2ePayload` 读 `harness_key`、`test_config.json`。
  - `main/host/dialogs.ts`、`privileged/dialogs.ts`：调用时才取 `dialog.showMessageBox`。
  - `electron-builder.e2e.yml`：`dev.cupper.app.e2e`，保留 inspector。
  - `build:e2e` 的步骤。
- ui-core：`mount(api, slots)`；`HostProvider` 在内部 `new HostClient`，外面拿不到。所以 `window.__cupperTest` 要先在 ui-core 开一个口子。
- 宿主：`test_harness` 拥有全部 scope，可以直接 `new`、`send`；`test/clock_advance` 在定时器触发、写入提交、事件扇出之后才返回；`host/status` 在 test-harness 构建里带 `pid`。
- CLI：test-harness 构建的 `BUILD` 是 `E2e`，用 harness 校验宿主，正好对上 e2e 构建的宿主；但没有发 `test/clock_advance` 的命令。
- xtask：`e2e` 已登记为持锁作业，`run: None`；`JobCtx::playwright` 已能解析 Playwright 的 JSON 报告；摘要里的 `"artifacts": []` 留给本任务。
- 现状：T083（请求面板）还没合并，T086（输入框）、T029 已合并；本机有 `nettop`；Node 24.19。

## 开工前的规模估计

行数指格式化后的行，含注释。按前几个任务的实测比例，实现放大 1.15 倍、测试放大 1.1 倍。

**实现**

| # | 模块 | 内容 | 草估 |
|---|---|---|---|
| 1 | 套件骨架 | 新的工作区包 `tools/e2e/`（`@cupper/e2e`，`@playwright/test` 与截图回归同版本）；`playwright.config.ts`（workers 2、`@smoke`、输出目录）；globalSetup 检查 App、假 agent、CLI 是否已构建；globalTeardown 清理 `dev.cupper.app.e2e` 的 plist；根目录的 `pnpm test:e2e` | 70 |
| 2 | 夹具 | 每用例一个临时根目录：<br>• `harness_key`（0600）、`test_config.json`（假 agent 路径、时钟、时长、`notifier: record`、`login_env`）；<br>• 临时 git 仓库；<br>• `_electron.launch` 加 `CUPPER_E2E_DIR`。<br>回收：关 App，按 pid 停宿主，清理假 agent 和 CLI，断言没有残留，删除目录 | 200 |
| 3 | CLI 包装 | 拉起 `cupper`，key 经 fd 3 传入，带 `--data-dir`、`--json`；一次性命令返回退出码和 JSON 行；`Repl` 按 `repl.exit` 分条；`watch` 加 `until(谓词)`；子进程登记后统一回收 | 140 |
| 4 | 原生确认框 | 经 `electronApp.evaluate` 在主进程里替换 `showMessageBox`（及 Sync 版）；按脚本给出回答（按钮序号、复选框）；记录调用参数 | 50 |
| 5 | `xtask ci e2e` | <br>• 构建：假 agent；test-harness 版 CLI；`runtime-pack` 的暂存目录不在时先跑一次；`pnpm build:e2e`。<br>• 两个产物（`Cupper E2E.app`、`CupperHost.app`）的目录摘要写进摘要的 `artifacts`（作业到摘要要接一条通道）。<br>• `ci pr` 只跑 `@smoke`，并按改动路径过滤（5.1.6，照 `ui` 作业）；`ci e2e`、`ci nightly` 跑全集 | 140 |
| 6 | `__cupperTest` | ui-core 的 `mount` 加一个可选的 `onClient`（或让 `HostProvider` 接受外部的 client）；渲染进程入口按 `__CUPPER_FLAVOR__ === "e2e"` 挂上 `window.__cupperTest`，导出 `counters`、连接状态（视图导出由 T091 按 NFR1-1 的需要补）；读取辅助 | 40 |
| 7 | CLI `clock-advance <ms>` | 只在 test-harness 构建里有，发 `test/clock_advance` | 25 |
| 8 | 按窗口断连 | 调主进程的 `__cupperE2E.disconnectWindow`，等重连 | 20 |
| 9 | 事件屏障 | 推进时钟；`untilEvent`（会话流）；`untilIndex`（`sessions --watch`） | 45 |
| 10 | `kill -9` | 从 `status --json` 取 pid，SIGKILL，等进程消失；等 UI 拉起新宿主（pid 变化） | 35 |
| 11 | CLI 提权 | 不带 key 的 CLI（`cli` 身份）在 `repl` 里 `elevate`；点侧边栏横幅的"查看"；替换后的确认框批准并勾选 approve | 50 |
| 12 | 外发采样 | 沿 ppid 找出 App 的进程树（含宿主、适配器）；`nettop -L 1 -P -J …` 采样并解析 | 70 |
| 13 | 按键 | `webContents.sendInputEvent` 发 keyDown、char、keyUp，支持修饰键和键名 | 50 |
| 14 | 请求面板 | 等 `data-ready`；按数字键；按 Tab 激活 | 35 |
| | 实现小计 | | **约 970**，×1.15 ≈ **1,115** |

**测试**

| 用例 | 草估 |
|---|---|
| E2E-single-instance：同一 `CUPPER_E2E_DIR` 再启动一个实例，它立即退出，原窗口被带到前面 | 35 |
| E2E-quit-confirm（T007 的做法）：<br>• 假 agent 的回合运行中退出：确认框的说明带回合数；取消后 App 留下；确认后 App 退出、宿主收到 `host/exit` 而退出；<br>• 空闲时退出：不弹框，宿主不退出（之后由夹具停掉） | 80 |
| `@smoke` 的夹具自检：App 起来，UI 连上，CLI 看得到宿主 | 20 |
| xtask 单元测试：产物摘要、PR 的 `@smoke` 与路径过滤、摘要的 `artifacts` 字段 | 50 |
| 辅助的自检：<br>• `__cupperTest` 与按窗口断连（重连后缺口、重复计数为 0）；<br>• 事件屏障与 `kill -9`（UI 重新拉起宿主）；<br>• UI 批准的 CLI 提权；<br>• 按键输入到输入框；<br>• 外发采样能取到样本；<br>• 请求面板（要等 T083）；<br>• `clock-advance` 的参数解析 | 205 |
| 测试小计 | **约 390**，×1.1 ≈ **430** |

**合计约 1,545 行**，比约 900 的线多出七成。按任务书，开工前停下，没有写代码。

## 拆分建议

**方案一（建议，照任务书的例子）：两份**

| 拆分 | 内容（模块编号） | 用例 | 估计 |
|---|---|---|---|
| T090（收窄）e2e 设施与 `xtask ci e2e` | 1–5 | E2E-single-instance、E2E-quit-confirm、`@smoke` 的夹具自检、xtask 单元测试 | 实现约 690 + 测试约 205 ≈ **895**（贴线） |
| T090b e2e 辅助函数 | 6–14 | 各辅助的自检 | 实现约 425 + 测试约 225 ≈ **650** |

- 第一份交付后，T092、T093 里只用夹具和确认框的用例就可以开始写。
- T091 要用第二份的屏障、`kill -9`、`__cupperTest`、断连、提权。
- 两份由同一个执行者接着做，关键路径的总时间基本不变；中间多一次 review 和合并。
- 第一份贴着线，建议上限给到约 1,000。

**方案二**：不拆，上限约 1,600，硬线约 1,700。按 1–5 先提交、6–14 后提交，每个模块统计一次。

## 请一并确认

1. **`__cupperTest` 要改 ui-core**：ui-core 的 `mount` 加一个可选的 `onClient(client)`，渲染进程入口只在 e2e 构建里传入。发布构建经 Vite `define` 删除这段（5.10.7 的产物扫描查 `__cupperTest`）。也可以改成让 `HostProvider` 接受外部的 client，请选一种。
2. **CLI 加一条只在 test-harness 构建里有的 `clock-advance <ms>`**：事件屏障要用 `test/clock_advance`，CLI 现在发不了。改的是 T046 的代码，有编译期门控。
3. **请求面板的辅助**：T083 还没合并，没有真实面板可做自检。建议先按 5.9.4 的 DOM 标记写好；自检等 T083 合并、编排者把 main 合进来后补上。在那之前，"每个辅助都被自检调用过"这一条，请求面板的三个辅助除外。
4. **外发采样**：只给出采样器，自检只断言能取到样本；`nettop` 能不能当验收方法（Q27）留给 T092。
5. **`@smoke` 的范围**：`ids.toml` 里两个编号都标的是 `nightly`。建议 PR 只跑一条夹具自检（App 起来、UI 连上、CLI 看得到宿主），两个编号留在每晚。
6. **位置**：新建工作区包 `tools/e2e/`（与 `tools/ui-screenshots/` 并列），根目录加 `pnpm test:e2e`（5.1.5）。
7. **`ci e2e` 的构建**：`runtime-pack` 的暂存目录不在时，作业自己先跑一次（只下载公开物料）；在的话直接用。e2e 的 App 和 `CupperHost.app` 每次都由当前提交构建，摘要记下两者的目录摘要。
