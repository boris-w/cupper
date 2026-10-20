# T075 桌面主进程 II：拉起宿主、control 连接与退出确认

- 派发日期：2026-10-20
- 执行者：子 agent（工程师）。T004、T005、T094、T028 的执行者接手，因为宿主打包（`CupperHost.app`、自用构建的 cdhash 配套）和宿主的启动、退出码、日志都是它熟悉的
- 分支：`feat/T075-desktop-host-launch`
- 依赖：T005、T071、T073、T074（都已合并）。离剩余关键路径有 2 个单位的松弛

## 背景

桌面主进程要拉起宿主、建立并校验 control 连接、在退出前确认（设计 5.7.1、5.7.2、5.5.1）。拉起和连接的底层都已就绪：原生扩展（T073）负责 `cupper-sys::spawn` 拉起、套接字校验与 socketpair 转发；host-client（T071）负责切帧、握手和请求关联；主进程骨架（T074）定下了构建种类和 e2e 目录布局；`cargo xtask host-bundle`（T005）产出各构建种类的 `CupperHost.app`。本任务把它们接起来。

## 范围

**做**（计划 T075 一行；设计 5.7.1 的"注销或关机"一行、5.7.2 的 `hostLauncher`、`hostConnection`、`controlClient`、`lifecycle`；5.5.1；5.8.1 的退化模式）：
1. **`hostLauncher`**：
   - 经原生扩展拉起 `Contents/Helpers/CupperHost.app/Contents/MacOS/cupper-host`，参数与拉起适配器同一套（`POSIX_SPAWN_SETSID`、`POSIX_SPAWN_CLOEXEC_DEFAULT`、环境变量白名单）；
   - 开发和 e2e 构建经 stdin 写入启动载荷（一行 JSON 加换行，写完立即关闭；交接，T023），**载荷只留在内存里**，宿主被杀后重新拉起时原样再传，并复用同一个 `HostKey`（交接，T073）；发布构建不写载荷；
   - 连接重试总预算 500 ms；
   - 按退出码处理：3 改为直接连接；4 提示"数据来自更新的版本"；5 提示数据目录不可用；6 提示迁移失败并给出备份位置。这些取值并入 `ConnectionState` 的 `failed.reason`（开放枚举，交接，T071）；
   - 安装更新的空窗期间不自动拉起（留接口，更新流程是后续任务）。
2. **`hostConnection`**：用原生扩展建立并校验连接，主进程拿到 socketpair 的一端；处理 `host/hello` 和 `initialize`；退化模式下校验 `host_proof`；发布构建遇到 `weak_auth: true` 拒绝连接。胶水按 T073 的交接：`auth`、`verifyHostProof` 的写法；`host_proof` 缺失时握手失败要调用 `conn.close()`；`connectHost` 抛出的 `code` 并入 `failed.reason`；用 `buildInfo()` 核对扩展的构建种类与 App 的 flavor 一致。握手超时由调用方传 `AbortSignal.timeout(...)`，套接字关闭时同样 abort（交接，T071）。
3. **`controlClient`**：control 连接上的 JSON-RPC 客户端。
4. **`lifecycle`**：单实例锁；退出前经 control 连接查 `host/status`，有运行中的回合或待处理的请求时弹原生确认框（NFR-8），确认后发 `host/exit`；注销或关机（`powerMonitor` 的 `shutdown`）和 `before-quit-for-update` 时跳过确认。
5. **"无法验证宿主"与"重启宿主"**（退化模式）：UI 崩溃后重启，新实例连不上还在运行的宿主时提示"需要重启宿主"；确认后向 `host.lock` 里记录的 pid 发 SIGTERM。**发信号之前核对这个 pid 的可执行文件路径和 cdhash 属于本构建的宿主**（安全 #9，计划第 4 节 T075 的注），不属于就不发，给出提示。
6. **`CUPPER_E2E_DIR`**：按 T074 定下的布局，e2e 构建的宿主数据目录和 harness 密钥文件放在它下面；只编进 e2e 构建。

**不做**：
- `windowBridge`、preload、数据连接（T076）；
- 更新流程（后续任务）；界面上的提示文案只走 `@cupper/i18n` 的语义码，页面本身由界面任务做。

## 验收标准

- [ ] 新增 UNIT-launcher：载荷只在内存、被杀后原样重传且复用同一个 `HostKey`；500 ms 重试预算；退出码 3–6 各自的处理；**pid 不属于本构建的宿主时不发信号**（路径不符、cdhash 不符两种）；发布构建遇到 `weak_auth: true` 拒绝连接；关机时跳过退出确认。
- [ ] 能用 `cargo xtask host-bundle --kind e2e` 产出的宿主在本机实际拉起并完成握手（可以是一条集成用例，也可以在汇报里写明手动验证的步骤和结果；不要求进 `pr` 组合作业）。
- [ ] 代码约束：主进程在调用时才取 `dialog.showMessageBox` 这类函数，不在模块加载时解构保存；界面文案一律走 `@cupper/i18n`。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T075 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T075-desktop-host-launch/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T074：e2e 构建的目录布局（`CUPPER_E2E_DIR` 下的 `data/`、`logs/`）。
- T023：启动载荷的格式；两种密钥都是 64 位十六进制；出现未知的键时宿主以退出码 1 退出。
- T071：`failed.reason` 是开放枚举；握手没有内置超时；mock 宿主的 `conn.port()` 可以直接给 control 连接用。
- T073：原生扩展与 host-client 的胶水（见上文第 2 项）；`token_changed` 不要一律当作攻击提示。
- T005：自用构建的 CLI 必须与同一次 `host-bundle` 产出的 `CupperHost.app` 配套；产物在 `target/host-bundle/<kind>/`。

## 注意

- 用例里的宿主数据目录、日志目录都用临时目录；**不要在本机真实的 `~/Library/Application Support/cupper*` 或 `~/Library/Logs/cupper*` 下建任何东西**。
- 发信号的用例只能向测试自己拉起的进程发，绝不向测试之外的进程发信号；用例结束后确认没有残留的宿主进程。
- 不运行真实的 Claude Code 或 Codex CLI。
- 依赖锁精确版本，许可证在 ADR-11 的 allowlist 内。

## 规模

计划估为 M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 1,000 行（包括贴线）时，开工前就停下来汇报，并给出拆分建议**。

## 参考资料

- 设计：`07-desktop.md` 5.7.1、5.7.2（含"T071 开工前的裁定"）、5.7.9；`05-flows.md` 5.5.1；`08-security.md` 5.8.1；`02-host.md` 5.2.13（退出码）。
- `docs/tasks/T071-host-client-conn/`、`T073-desktop-native/`、`T074-desktop-main-skeleton/`、`T005-host-bundle/`。
- `apps/desktop/`、`packages/host-client/`、`crates/cupper-desktop-native/`。

## 工作方式

在原来的 T004 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T075-desktop-host-launch main`。T028 的分支由编排者合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 1,460–1,760 行，在写代码之前停下汇报，并查出任务书没写、但不做就跑不起来的三处：原生扩展缺按 pid 取可执行文件路径和 cdhash 的接口；App 里还没打包原生扩展和宿主；主进程缺基于 `net.Socket` 的 `HostPort`。

**裁定：按执行者的建议拆分（计划 v1.3.17）。**
- **T075 收窄为"拉起宿主与 control 连接"**：加载扩展并用 `buildInfo()` 核对 flavor；**dev、e2e 构建的打包配置**（`cupper_native.node` 放 asar 外，`target/host-bundle/<kind>/CupperHost.app` 放进 `Contents/Helpers/`）；`SocketPort`（主进程的 `HostPort`）；`hostLauncher`；`hostConnection`；`controlClient`；`CUPPER_E2E_DIR`；启动时的接线。验收 UNIT-launcher 的拉起部分，加一条用 e2e 宿主产物真实拉起并握手的集成用例。规模改为 L，上限约 1,000 行，硬停止线 1,100；推算贴线也先停下。打包配置留在本任务，因为要让 App 本身真正拉起宿主；自用构建和发布构建的打包配置仍归 T160、T180。
- **新增 T007"退出确认与重启宿主"**（依赖本任务）：`cupper-sys` 的 `proc_pidpath` 与按 pid 取 cdhash、扩展的 `processImage(pid)`、`fileCdhash(path)` 与 TS 声明；`lifecycle`；"无法验证宿主"与"重启宿主"。新增编号 UNIT-host-restart（`tests/ids.toml` 已在 main 上改好）。T090 改为也依赖 T007。
- **e2e 启动载荷的约定**（已写进设计 5.9.9）：测试夹具在 `CUPPER_E2E_DIR` 下写 `harness_key`（64 位小写十六进制加换行，权限 0600）和 `test_config.json`（宿主 `test_config` 的 JSON）；e2e 版主进程拉起宿主时读这两个文件，连同自己生成的 `dev_key` 组装成启动载荷。用例里由测试自己写这两个文件。
