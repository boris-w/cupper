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
