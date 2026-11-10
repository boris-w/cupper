# T250 汇报

- 执行者：子 agent（宿主工程师；T150 的执行者）
- 分支：`feat/T250-notifications-pop`
- 完成日期：2026-11-10

## 做了什么

开工前按模块估约 600 行（`notes.md`），在停止线 650 以内。

1. **App 图标（拿铁）**
   - `apps/desktop/build/icon.sh` 用 `sips` 和 `iconutil`，从会议母版 `cupper-latte.png` 生成 `icon.icns`，产物入库。脚本可以重复运行，连跑两次产物的哈希相同。
   - 四份 electron-builder 配置的 `mac.icon` 指向它。
   - `cargo xtask host-bundle` 把同一个文件复制进 `CupperHost.app/Contents/Resources/icon.icns`，并写 `CFBundleIconFile`。
   - UNIT-host-bundle-identity 补了两条断言：四种构建的宿主都有 `CFBundleIconFile`；每份配置的 `mac.icon` 就是 `host-bundle` 取的那个文件。
   - **macOS 15 上看过了**（`spikes/T250-icon-macos15/`，spike-run，macOS 15.8.2）：
     - 第一次运行 42757587406：探针 App 没有可执行文件，访达在图标上盖了"禁止"标记。
     - 补上可执行文件后重跑 42758561300，画面干净：256 px 和 64 px 两档里，拿铁的底板大小、圆角形状、投影都和计算器、备忘录、文本编辑、终端一致，没有被套进灰色底板。和设计师在 macOS 26.4 上的结论相同。
2. **cupper-sys 的 UserNotifications**（`notify.rs`），用的是 T150 的主循环：
   - 查询授权状态；
   - 申请授权：只要提醒，不要声音和角标。申请结束后再查一次状态，所以系统没问用户就拒掉的申请，仍然记为"未决定"；
   - 弹通知：只有标题和正文，不设 category，所以没有任何操作按钮，也没有声音；
   - 清掉已送达的通知；
   - 读取系统首选语言。
   - UNIT-notifier-no-actions 断言通知内容没有 category、没有声音。
3. **协议**：`host.updated.notification_auth`，取值 `not_determined`、`authorized`、`denied`，是可选字段，只做附加；schema 和 TS 类型已重新生成。
4. **宿主 `notifier`**
   - `IndexActor` 处理意图的地方把每条意图交给 notifier，带上会话标题；如果是 `needs_action`，再带上最早一个请求的类别，据此分成"需要批准"和"需要回答"。
   - 文案内置中英文，跟随系统语言，措辞参照 Codex：
     - 标题用会话标题，没有时用按原因的后备标题；
     - 正文只用按原因选的内置文案，不放 agent 的原文。
   - 开关沿用已有的三个键：`needs_action`、`turn_completed`、`error`，回合失败归 `error`。关掉之后不弹通知，但 `notification.intent` 事件照写。
   - 授权状态写进 `host.updated`。
   - 有序退出时，在释放电源断言之后、删除套接字之前，清掉已送达的通知。
5. **用例**
   - IT-notifier-record（索引的测试夹具 + record 后端）：
     - 三种原因（`needs_action` 再分批准、回答两种）各弹一条，文案对，无标题时用后备标题；
     - 通知 ID 就是意图的 ID；
     - 开关全关之后不再弹，意图照写；
     - 前台的 `desktop_ui` 正在看的会话不弹；
     - 授权状态进 `host.updated`。
   - 一个单元测试覆盖中文文案。

## 关键取舍

- **授权在宿主启动时申请，只有发布、自用、开发构建申请**（编排者同意，合并时补进设计 5.2.9）。
  - 理由：宿主由打开 Cupper 拉起，用户这时就在电脑前，能当场回答。如果等第一次要弹通知时再申请，授权框会在用户不看窗口的时候弹出（通知本来就只在窗口不在前台时发），那一条通知还要等用户回答。
  - 启动时就知道状态，设置页（T147）可以一开始就显示被拒的提示。
  - 每次启动只查询，只有"未决定"时才申请；被拒后不再申请。
- **test-harness 构建一律不碰通知服务**：
  - `test_config.notifier: record` 记录通知，并当作已授权上报，好让 `host.updated` 的路径也被测到；
  - `system`（宿主集成测试的默认值）就是关闭。
  - 不在 App 包里运行的宿主也关闭：通知服务只认 App 包。
- **不建 actor**：notifier 是一个可以共享的句柄，`IndexActor` 同步调用。投递和授权都由系统异步完成，回调只记日志，或者给 `IndexHandle` 发消息。
- **IT-notifier-record 放在索引的测试夹具上**，在 crate 内用 record 后端直接读记录，不新增测试方法；模拟点击的入口留给 T251。
- **意图的标题和请求类别从状态核心当前的会话摘要取**，不读索引流的视图：视图要等这一批写完才更新，刚进入 `needs_action` 时还没有请求类别。

## 验证方式

验收标准对照：
- [x] IT-notifier-record、UNIT-notifier-no-actions 通过，宿主的其余用例照常通过：
  - 本机 `cargo test -p cupper-host --all-features` 全部通过；
  - cupper-sys、xtask `host_bundle` 的测试通过；
  - CI 的 `rust` 作业通过。
- [x] 主 App 和宿主都用拿铁母版生成的图标；macOS 15 上的检查有结果（见上文和 notes.md）。
- [x] 宿主的约束都守住：
  - `unsafe` 只在 cupper-sys；
  - 宿主不新增读时间的地方；
  - 没有新的子进程；
  - 索引流的写者仍只有 `IndexActor`，notifier 不写流；
  - 协议只做附加，本机 `cargo xtask ci protocol` 通过，schema-compat ok；
  - 四种特性组合下 clippy 都无告警。
- [x] 不在本机碰系统的通知服务和授权：本机的用例和 e2e 一律走 record。探针只在 runner 上跑。
- [x] PR 上 GitHub CI 的 8 个检查都通过；nightly 的 `selfuse`、`release-config` 在分支上通过：
  - PR #99：https://github.com/boris-w/cupper/pull/99
  - ci 运行：https://github.com/boris-w/cupper/actions/runs/42758900925 ，8 个作业全过
  - cupper-sys.yml 的 security-peer（macos-26）通过：https://github.com/boris-w/cupper/actions/runs/42758900641
  - nightly（selfuse 那次）：https://github.com/boris-w/cupper/actions/runs/42758898878 ，全过
  - nightly（release-config 那次）：https://github.com/boris-w/cupper/actions/runs/42758907152 ，全过
  - spike-run：https://github.com/boris-w/cupper/actions/runs/42757587406 、https://github.com/boris-w/cupper/actions/runs/42758561300

本机还跑过：fmt、prettier、eslint、`trace-ids check`、protocol 包的 TS 测试（171 个）。

规模：`cargo xtask size` +679 −21，原型 +60 单列，计入 +619（停止线 650）。图标文件是二进制，不计入。

## 提交列表

```
d9e7ea47 docs(task): T250 notes: the icon on macOS 15, its making, the size
1143b541 chore(spike): give the icon probe an executable
0fee7e50 feat(host): post the notification of each intent
927e4d68 feat(protocol): host.updated.notification_auth
7957e0e5 feat(sys): UserNotifications for the host
559427db chore(spike): look at the latte icon on macOS 15
9d3dd503 feat(desktop): the latte icon for the app and the host bundle
```

## 遗留问题 / 范围外发现

1. **要补进设计 5.2.9 的**（编排者合并时补）：授权的申请时机和理由；test-harness 构建 `notifier: system` 的含义改为"关闭"。
2. **交给 T251 的事**：
   - 点击的回调要在通知中心设代理。本任务没有设代理：宿主平时不在前台，不设代理时横幅照常出现。
   - 通知的 `userInfo` 要带会话 ID 和请求 ID。现在的通知 ID 就是意图 ID（`n_…`），也可以由它反查。
   - 测试用的点击入口。
3. **真实的弹出、授权框、名称和图标**都没有在本机验证（守则不允许），留给 MAN-notification-product。
4. **objc2 的一处小坑**：`UNMutableNotificationContent.subtitle` 在没设置时返回 nil，绑定却声明为非空，一调用就 panic。产品代码不读它，测试里也去掉了这一条断言。以后有人读这个属性要注意。
5. **本机遗留**：没有新增；`/Applications/SP10.app` 等 T101 的遗留照旧，等用户决定清理。
