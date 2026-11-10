# T250 工作笔记

- 执行者：子 agent（宿主工程师；T150 的执行者）

## 开工前按模块估算（2026-11-10）

按 `cargo xtask size` 的口径（新增行，含测试；图标文件、生成物、`docs/` 不计）。

| 模块 | 内容 | 估计 |
|---|---|---|
| 图标：生成脚本 | `apps/desktop/build/icon.sh`：由拿铁母版用 `sips`、`iconutil` 生成 `icon.icns`，可重复运行 | 20 |
| 图标：electron-builder | 四份配置的 `mac.icon` 指向 `build/icon.icns` | 8 |
| 图标：`host-bundle` | 复制同一个 icns 进 `Resources/`，写 `CFBundleIconFile`；UNIT-host-bundle-identity 断言图标，并核对四份配置的 `mac.icon` 是同一个文件；布局测试补上图标文件 | 35 |
| sys：`notify.rs` | 查询授权状态、申请授权（只要提醒，不要声音和角标）、弹通知（不设 category，没有 action）、清掉已送达的通知、通知内容的构造、系统首选语言；依赖 | 150 |
| sys：用例 | UNIT-notifier-no-actions（内容没有 category，也就没有任何操作按钮），内容的字段 | 25 |
| 协议 | `NotificationAuth` 开放枚举、`host.updated.notification_auth`（附加）、样例 | 20 |
| 宿主：`notifier` | system / record / 不可用三种后端；中英文案（4 种原因 × 后备标题、正文）；三个开关键；启动时查询、需要时申请授权，被拒不再申请；有序退出时清掉 | 160 |
| 宿主：`IndexActor` | 意图连同会话标题、`pending_kind` 交给 notifier；授权状态写进 `host.updated` | 30 |
| 宿主：supervisor | 按构建和 `test_config` 选后端、启动时申请、有序退出时清掉 | 25 |
| 宿主：IT-notifier-record | 索引测试夹具上接 record 后端：三种原因的文案、开关、前台在看、`host.updated` | 110 |
| 测试夹具 | `IndexDeps`、`HostUpdated` 字面量的补字段 | 15 |
| **合计** | | **约 600** |

另有 macOS 15 上看图标的原型（`spikes/T250-icon-macos15/`，约 45，原型不计入）。

在停止线 650 以内，开工。

## 什么时候申请授权：宿主启动时

宿主由打开 Cupper 拉起，这时用户就在电脑前，能当场回答授权请求；第一次要弹通知时再申请，授权框会在用户不看窗口的时候弹出（通知本来就只在窗口不在前台时发），那一条通知还要等用户回答。启动时就知道授权状态，设置页（T147）也能从一开始就显示被拒的提示。每次启动只查询状态，只有 `not_determined` 时才申请；被拒后系统也不会再弹框（T101）。

裁定（编排者）：只有发布、自用、开发构建申请；record 模式不碰通知服务。实现上，带 `test-harness` 的构建按 `test_config.notifier` 记录（`record`，并当作已授权上报）或关闭（`system`），都不调用 UserNotifications。

## 图标的生成

- `apps/desktop/build/icon.sh [母版]`：默认取 `docs/meetings/2026-11-10-app-icon/icon/cupper-latte.png`，用 `sips` 缩出 16–512 的 1x、2x 共十张，`iconutil` 合成 `apps/desktop/build/icon.icns`。连跑两次产物的 SHA-1 相同（04a60fa2…），可重复。
- 四份 electron-builder 配置的 `mac.icon` 指向它；`host-bundle` 把同一个文件复制成 `CupperHost.app/Contents/Resources/icon.icns`，写 `CFBundleIconFile: icon`。UNIT-host-bundle-identity 断言四种构建的 `CFBundleIconFile`，并核对每份配置的 `mac.icon` 就是 `host-bundle` 取的那个文件。

## macOS 15 上看一眼（spike-run，macos-15 runner，macOS 15.8.2 24H232）

原型 `spikes/T250-icon-macos15/check.swift`：做一个只带 icns 的探针 App，向 NSWorkspace 要它和计算器、备忘录、文本编辑、终端的图标，各画 256 px 和 64 px。

- 第一次（42757587406）：拿铁图标的底板和系统 App 一样大、圆角一样，没有灰色底板；但探针 App 没有可执行文件，访达在图标上盖了"禁止"标记。
- 加上可执行文件后重跑（42758561300）：画面干净。256 px 和 64 px 两档里，拿铁的底板大小、圆角形状、投影都和四个系统 App 一致，没有被重新裁边，也没有灰色底板。结论与设计师在 macOS 26.4 上看到的相同。
- 图片在两次运行的 artifact 里（`spike-results/compare.png`），没有入库。

## 规模

`cargo xtask size`：+679 −21，其中原型 `spikes/` +60 不计（编排者裁定单列），计入 **+619**（停止线 650）。图标文件是二进制，不计入。

| 目录 | 新增 | 内容 |
|---|---|---|
| crates/cupper-host | 391 | `notifier.rs`（后端、文案、开关、授权、清除、中文单元测试）、`IndexActor` 接线、supervisor、IT-notifier-record、夹具 |
| crates/cupper-sys | 159 | `notify.rs`、依赖与导出、UNIT-notifier-no-actions |
| crates/xtask | 26 | 图标复制、`CFBundleIconFile`、断言 |
| apps/desktop | 25 | `icon.sh`、四份配置的 `mac.icon` |
| crates/cupper-protocol | 18 | `NotificationAuth`、`host.updated` 字段、样例 |
