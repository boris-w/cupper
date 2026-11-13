# T150 系统通知 I：宿主主线程与通知身份

- 派发日期：2026-11-10（同日开工估算后拆分，本任务书是拆分后的版本）
- 执行者：子 agent（宿主与桌面工程师，cupper-sys、宿主、桌面主进程）
- 分支：`feat/T150-notifications`
- 依赖：T099、T101（都已合并）。T250 依赖本任务，T251 依赖 T250

## 拆分

开工前按模块估约 1,690 行（`notes.md`），远超停止线 800。计划 v1.3.90 按"每一块合并时都有人用"拆成三块：

| 任务 | 内容 | 用到它的地方 |
|---|---|---|
| **T150（本任务）** | 主线程改跑 NSApplication、LaunchServices 登记、宿主的名称和图标 | 主线程的 run loop 本身就被用上；登记和身份随每次拉起生效 |
| T250 | UserNotifications 与 `notifier`：弹出通知、开关、record、授权状态、退出时清掉 | 系统通知弹出来 |
| T251 | 点击与导航：`ui/navigate`、激活外层 App、交 URL、系统单独拉起的宿主、`deeplink` | 点击通知跳到会话 |

T250、T251 的任务书到时另写。

## 范围

**做**（计划 T150 一行；设计 5.2.3、5.2.9 六条规则的第 1、2、6 条、5.7.2 的 `hostLauncher`、5.10.1）：

0. **第一条验收的补验**：已完成并通过（`results.md`，2026-11-10，用户在场）。
1. **cupper-sys**（`unsafe` 只在这里）：
   - 主线程改跑 NSApplication（accessory：不进 Dock、不开窗口），取代 `runloop.rs` 的裸 CFRunLoop。现在挂在主线程上的东西（IOKit 通知等）照常工作，停止和退出的路径不变。
   - `LSRegisterURL`，以及查询某个 bundle 是否已登记。
   - 引入 objc2 系列 crate 时锁定精确版本，许可证要在 ADR-11 的白名单里。
2. **登记**：
   - 桌面 `hostLauncher` 拉起宿主之前，经 `cupper_native` 确保 `CupperHost.app` 已登记；
   - 宿主启动时也检查，没登记就自己登记，只记日志，不影响启动。
3. **宿主的名称和图标**（`cargo xtask host-bundle`）：
   - Info.plist 写 `CFBundleDisplayName`、`CFBundleName`，取构建种类的显示名称（5.7.9）；
   - 把 App 的 icns 复制进 `CupperHost.app/Contents/Resources`，`CFBundleIconFile` 指向它；
   - **本任务只写名称，不写图标**（2026-11-10 裁定）：
     - App 还没有自己的图标。Electron 的默认图标不在 node_modules 里，取它要么下载、要么入库一份替身，都是以后要扔掉的东西。
     - 用户决定图标先放一放，T250 合并前定稿；`CFBundleIconFile` 和图标文件到时随 T250 一起加。
     - 通知中心在 helper 第一次连上通知服务时才读取名称和图标（T101 的 `results.md`），T150 不调用通知接口，所以先不写图标没有风险。
4. **用例**：
   - 新增 **UNIT-host-bundle-identity**（已登记）：四种构建的宿主 Info.plist 名称都对（图标由 T250 补进这个用例）。
   - 宿主现有的用例在 NSApplication 主循环上照常通过。
   - 登记的调用不在本机的用例里真的执行（不改用户的 LaunchServices 数据库），按 sys 层的接口测，或者放到 runner 上。

**不做**：
- UserNotifications、`notifier`（T250）；
- 点击、`ui/navigate`、激活、交 URL、系统单独拉起的宿主、`deeplink`（T251）；
- 设置页的通知开关和授权提示（T147）。

## 验收标准

- [x] 第 0 步的补验通过（`results.md`）。
- [ ] UNIT-host-bundle-identity 通过；宿主的用例在 NSApplication 主循环上全部通过。
- [ ] 宿主的约束都守住：`unsafe` 只在 cupper-sys；时间只经 `Clock`；子进程只经 `cupper-sys::spawn`。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，nightly 的 `selfuse`、`release-config` 在分支上通过（由 `main` 触发），汇报里写上链接（守则第 9 条）。

## 规模

- 停止线：450；硬停止线：540。
- 原型的改动、图标文件不计入。
- 不要靠压缩代码或注释去守线。

## 参考资料

- 设计：5.2.3（主线程）、5.2.9（六条规则、点击）、5.2.13（启动顺序）、5.7.2（`hostLauncher`）、5.7.9（构建种类）、5.10.1（包结构、宿主的名称和图标）。
- 卷宗：T101（`report.md`、`results.md`、原型）；本目录的 `results.md`、`notes.md`。
- 交接事项：`docs/tasks/handoff-notes.md` 里接收任务为 T150 的各行。

## 注意

- 不在本机碰系统状态：
  - 不往 `/Applications` 装东西；
  - 不对真实的 bundle 调 `lsregister` 或 `LSRegisterURL`；
  - 不改通知授权。
- `/Applications/SP10.app` 留着，等用户决定清理（`.private/cleanup/T101-leftovers.md`）。
- 不在本机跑会抢焦点的用例（守则第 10 条）。不运行真实的 Claude 或 Codex CLI。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

worktree `.claude/worktrees/T150`，分支 `feat/T150-notifications`。第 0 步的提交留在这条分支上。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
