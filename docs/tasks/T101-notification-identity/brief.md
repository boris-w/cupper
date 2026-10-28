# T101 SP-10 通知身份（Q2）

- 派发日期：2026-10-28
- 执行者：子 agent（原型实测工程师）
- 分支：`spike/T101-notification-identity`
- 依赖：无（可以提前，计划 1.7）。结论即检查点 K5：通过则按原设计做 T150，不通过则启用 T158

## 背景

ADR-01 规定系统通知全部由宿主 helper 弹出：`Cupper.app/Contents/Helpers/CupperHost.app` 用 UserNotifications 弹通知，点击后回调到宿主，宿主经 control 连接给桌面主进程发 `ui/navigate`，并经 LaunchServices 按 bundle id 激活 Cupper（设计 5.2.9）。helper 的通知身份还没验证过（Q2）：
- 它能不能申请到通知授权；
- 通知和系统设置里显示的名称、图标是不是用户认得出的 Cupper；
- 点击回调能不能回到 helper，并激活主 App。

AP-3 是 M1b 的 P0。不行的话，启用主进程的 `notifications` 后备模块（T158）：UI 在运行时由 UI 主进程弹通知。

## 范围

**做**（计划 T101 一行和注；设计 5.13 Q2、5.2.9）：
1. **照产品的布局搭一个最小原型**：
   - 主 App 里的 `Contents/Helpers/` 放一个 helper bundle；
   - helper 由主 App 直接拉起，和产品里一样：经 `posix_spawn` 并调用 setsid，不经 `open` 或 LaunchServices（ADR-01；`cupper-sys::spawn` 的做法）；
   - 语言不限，Swift 或 Rust 都行；
   - bundle id 用原型专用的 `dev.cupper.spike.sp10` 和 `dev.cupper.spike.sp10.host`，不用产品的任何 bundle id，免得在系统设置里留下产品名下的授权记录；
   - 签名用 ad-hoc，和自用构建现在的签法一样（5.7.9）。
2. **逐项验证并记录现象**：
   1. helper 申请通知授权：授权框显示的名称和图标；
   2. helper 弹出通知：通知上的名称和图标；
   3. 系统设置 → 通知里列出的条目，名称和图标是什么、是哪个 bundle；
   4. 点击通知：回调是否回到正在运行的 helper；helper 能否按 bundle id 激活主 App；
   5. helper 已经退出时点击通知：系统会不会单独启动 helper bundle，会的话会发生什么。产品里不能有一个脱离 UI、自己跑起来的宿主，所以这一项决定设计要不要补规则；
   6. 主 App 没在运行时，helper 按 bundle id 启动它，并只把一个 URL 交给它，不走 `cupper://` 这类通用路由（安全 #7）。
3. 判定 K5，写明 T150 或 T158 需要知道的事：Info.plist 的哪些键要怎么写、helper 需不需要 `LSUIElement` 之类的设置、名称和图标从哪里来。

**不做**：
- TCC 归属与 disclaim（MAN-tcc，M2 的 SP-10）；
- 产品代码。原型代码放在 `spikes/T101-notification-identity/`，产品代码不依赖它。

## 约束

- 不申请辅助功能、自动化、屏幕录制权限，Info.plist 里不写这几项的用途说明（5.8 的系统权限）。
- 原型留在系统里的东西（通知授权条目、LaunchServices 登记），列成清单写进主仓库的 `.private/cleanup/T101-leftovers.md`（不入库），写明清理方法。由编排者请用户确认后再清理，子 agent 不删除。
- 入库的截图和记录不含个人信息：用户名、本机路径、其他应用的通知等。截图只截原型自己的部分。

## 用户参与

是：会弹系统授权框，需要用户在场点"允许"，并确认现象（计划第 6 节第 8 行）。

- 原型准备好、要弹授权框之前，用 SendMessage 发给 `main`，写清用户要做什么、要看什么，由编排者转给用户。用户确认之后再开始。
- 授权框、通知、系统设置里的现象，用户看到的和你记录的不一致时，以用户的确认为准。
- 不要用自动化去点授权框或通知。

## 验收标准

- [ ] 第 2 条的 6 项各有现象记录（截图或文字），并给出 K5 的判定：通过或不通过，以及理由。
- [ ] 写明 T150 或 T158 要用的结论：Info.plist 的键、名称和图标的来源、helper 退出后点击通知的处理；以及设计 5.2.9 要不要补规则，交编排者。
- [ ] MAN-notification-spike 的证据（用户确认的现象）。
- [ ] 遗留清单已写进 `.private/cleanup/`；仓库里没有个人信息。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：300；硬停止线：400

<!-- 原型代码也计入。执行者开工前在 `notes.md` 里按模块写行数估计。每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.2.9（关注度与通知）；5.7.2（主进程的 `notifications` 后备模块）；5.7.9（构建种类、bundle id、签名）；5.8（系统权限）；5.13 的 Q2、Q21；ADR-01。
- `docs/03-plan.md`：T101、T150、T158 三行，检查点 K5（4.4）。
- 卷宗：`docs/tasks/SP-4b-local-security/`（本机签名与进程拉起的实测）；`docs/tasks/T005-host-bundle/`（CupperHost.app 的布局）。

## 工作方式

在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b spike/T101-notification-identity main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-28，编排者）

原型写到 +298 行，实测时多半还要改几处，执行者预计最后在 300–320 行。接受，不为压行数删 README 的说明。

- 停止线：340；硬停止线：400
