# T101 工作笔记

- 执行者：子 agent（原型实测工程师）

## 开工前的规模估计

原型代码在 `spikes/T101-notification-identity/`，全部计入；`docs/` 下的记录不计（`cargo xtask size`）。

| 模块 | 内容 | 估计 |
|---|---|---|
| `Common.swift` | 两个 bundle id、URL scheme、日志（主 App 和 helper 写同一个日志文件） | 15 |
| `Host.swift` | helper：不建 NSApplication，主线程跑 run loop（与宿主的 `cupper-sys::runloop` 同形）；申请授权、弹通知、点击回调；按 bundle id 激活或启动主 App 并交一个 URL；系统单独启动它时的记录；`status`（只读授权状态、已送达的通知） | 100 |
| `Main.swift` | 主 App：一个窗口，显示收到的 URL；按参数经 `posix_spawn`（SETSID、CLOEXEC_DEFAULT、环境变量白名单）拉起 helper，可以拉起后立即退出 | 50 |
| `icon.swift` | 画两枚可区分的图标（主 App 蓝色 M、helper 橙色 H） | 15 |
| `build.sh` | 编译、组装 `SP10.app/Contents/Helpers/SP10Host.app`、写两份 Info.plist（两种 helper 写法：自己的名称和图标 / 照 T005 现在的写法）、ad-hoc 签名（与自用构建相同，开 hardened runtime） | 50 |
| `README.md` | 用法、每一步的命令、副作用 | 35 |
| **合计** | | **约 265**，在停止线 300 以内 |

每次提交后跑 `cargo xtask size`。

## 实际规模

`cargo xtask size`：+305（spikes），在编排者改定的停止线 340 以内（任务书原写 300，2026-10-28 补充裁定改为 340）。比估计多约 40 行，主要是实测中加的两处：helper 自己登记到 LaunchServices，以及系统启动 helper 时改跑 NSApplication。

## 过程与取舍

- **安装位置**：先在会话 scratchpad 的临时目录预检，LaunchServices 把 App 标成 `in-temp-dir`，嵌套的 helper 也没登记。为了和产品一致，经编排者和用户同意，实测时装在 `/Applications/SP10.app`。
- **名称和图标的区分**：own 写法里 helper 的名称和图标都和主 App 不同（SP10 Notifier、橙色 H 对 SP10 Spike、蓝色 M），看到哪个就知道系统用的是哪个 bundle。bare 写法照 T005 现在的宿主 Info.plist，用来看产品不改时会怎样。
- **第一次申请失败**：usernoted 报 "Failed to find or validate client"。helper 没在 LaunchServices 登记，主 App 装好后自动登记了，嵌套的 helper 没有；对主 App 做 `lsregister -f` 也不带上它。helper 改为发现自己没登记就调用 `LSRegisterURL`，之后正常。
- **授权请求被误关**：关掉就记为 denied，不会再弹框。用户在系统设置里打开后继续。没有换 bundle id，因为任务书只给了两个 id。
- **步骤重做**：
  - s2：用户在 helper 退出前就点了，改做 s2b，helper 弹完 1 秒就退出；
  - 第 4 步：helper 去找主 App 时主 App 还没退干净，改成 2 秒后再找，重做为 s4b；
  - 之后按编排者的要求一次只发一条通知，每一步先说明要看什么。
- **加做 s2c**：s2b 表明系统启动的 helper 只跑 CFRunLoop 时收不到点击。为了给 T150 找出路，系统不带参数启动 helper 时改跑 NSApplication，结果收到了。由主 App 拉起、正在运行的 helper 改跑 NSApplication 之后会怎样，没有测（编排者宣布实测结束），记在 results.md 的"缺口"里。
- **用户的记忆与日志**：用户中途说"点了好几次"、"收到好多条通知"。按日志，一共 6 次点击，每次都对得上自己的那条通知；拒绝那一轮发出的 s1 被静默放进了通知中心。results.md 里以日志为准，用户确认过的单独列出。
- **脱敏**：
  - `logs/` 里去掉了本机路径和其他应用的 bundle id（`frontmost=<another app>`）；
  - 系统日志只留与原型有关的行；
  - 用户截图不入库，编排者转述的内容写成文字。
- **范围外**：实测期间，系统日志里出现过另一个应用对原型窗口做辅助功能查询时 tccd 的记录。原型没有申请任何 TCC 权限，这些行没有入库。
