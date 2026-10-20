# T129 运行时安装器 III：接进宿主启动

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师）。T059 的执行者接手（它给本任务留好了接入点）
- 分支：`feat/T129-installer-startup`
- 依赖：T059、T115（都已合并）。T099 依赖本任务

## 背景

运行时安装器能按内嵌清单复制、按锁文件下载并本机打包（T056、T115），但宿主启动时没有任何地方构造并运行它。所以在接线之前，生产环境的运行时目录是空的，Claude 适配器拉起前的核对会失败，报 `RuntimeFailed`。

T059 已在监护者里留好了接入点。

## 范围

**做**（计划 T129 一行；设计 5.2.12、5.2.13、ADR-08）：
1. **构造 `Installer`**（T115 的交接）：
   - `plans`：`runtime::plans(manifest::embedded())`；
   - `bundle`：`Arc::new(BundleDir::of_this_host()?)`；
   - `dir`：`runtime::runtime_dir(data_dir)`；
   - `in_use`：运行中适配器所用的版本；
   - `registry`：`test_config.runtime.registry_url`，没有时用 `npm::REGISTRY`；
   - `proxy`：登录 shell 的 `HTTPS_PROXY`；
   - `durations`。
2. **启动时机**：在 `Host::start` 的 `Watch::new()` 之后（也就是宿主的 cdhash 自检之后）在后台 `run(&clock, publish)`（T059 的交接）。
3. **状态与监护**：
   - 状态写进索引流的 `agents`；
   - 安装器忙闲填进 `Observed.installer_busy`，现在固定为 `false`，有注释；
   - `Failed(BundleChanged)` 时调 `Watch::bundle_changed()` 转入待升级。
4. **下载大小上限**（T115 的评审）：`npm::download` 现在用 `response.bytes()` 整个读进内存，没有上限。改为边读边计，超过上限（例如 64 MB，或按锁文件推出）就按网络失败处理。
5. **静止屏障**（T047）：后台运行的安装器是脱离发起方的 task，要持有 `Hold`，或者登记为 actor 种类，并说明理由；等待外部 I/O（下载、打包子进程）不在屏障范围内。

**不做**：运行时安装器本身的逻辑（T056、T115 已完成）；设置页里的运行时状态显示（M1b）。

## 验收标准

- [ ] **IT-runtime-startup**（进程级）：
  - 宿主启动后经本地 registry（照 T115 的 IT-runtime-download 的离线 registry 与假 Node）装好，索引流 `agents` 的运行时状态到 `ready`；
  - 包内文件被改时转入待升级。
- [ ] 下载大小上限有用例（超过上限按网络失败）。
- [ ] 新增的 IT 连跑 10 次全绿；不留残留进程（照 T008 修复 1 的守卫做法）。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T129 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T129-installer-startup/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T129 的行）

- T115：构造 `Installer` 的参数；大小上限。
- T059：`Watch::new()` 之后启动；`Watch::bundle_changed()`；`Observed.installer_busy`。

## 注意

- 不访问真实的 npm 仓库，用例一律用本地 registry；请求不带用户信息（User-Agent 已是 `cupper-runtime-installer/<版本>`）。
- 宿主运行期间不读取 App 包内的文件，唯一的例外就是安装器按内嵌清单核对哈希后的复制。
- 代码约束：时间只经 `Clock`；子进程一律经 `cupper_sys::spawn`。

## 规模

计划估为 S。**开工前先在 `notes.md` 里按模块写行数估计；推算超过约 400 行（包括贴线）就先停下汇报**；硬停止线 470。

## 工作方式

在原来的 T054 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T129-installer-startup main`。T059 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
