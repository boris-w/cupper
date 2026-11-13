# T258 汇报

- 执行者：子 agent（宿主与桌面；T150、T250、T251、T254 的执行者）
- 分支：`fix/T258-notification-auth-recheck`
- 完成日期：2026-11-13

## 做了什么

T254 复核时发现：宿主只在启动时查询通知授权。用户在系统设置里打开通知以后，`host.updated.notification_auth` 一直停在 denied，设置页的提示也一直显示，直到宿主重启。本任务让宿主在桌面窗口回到前台时重新查询一次。

1. **判定放在索引的纯核心**（`index/status.rs`）：它本来就在处理在场，现在多记一份"哪些 desktop_ui 连接在前台"，不论是否在看会话。
   - 某个 desktop_ui 的 `client/presence` 从"不在前台"变成"在前台"时，核心发出新动作 `Foreground`；
   - 报"不在前台"或连接关闭时，忘掉它的前台状态；
   - 已在前台、只换了会话，不发出动作。
2. **只查询，不申请**：`IndexActor` 收到 `Foreground` 后，调 notifier 新增的 `recheck`。它只读授权状态（T250 的查询接口），从不弹授权请求；结果经原来的 `Mail::NotificationAuth` 写进 `host.updated`，和现有值相同就不写。启动时的 `authorize` 照旧：只有"未决定"才申请，被拒后不重复申请。
3. **record 后端**加了一个可以设置的授权状态（默认 authorized），供用例模拟用户在系统设置里改授权。record 模式照旧不碰系统的通知服务。

## 关键取舍

- **时机选"窗口回到前台"**（方案经编排者确认）：用户去系统设置打开通知后，一定会切回 Cupper，这时更新最及时，设置页的提示随即消失。其他候选都没选：
  - 设置页打开：要加协议方法；
  - 定时：白白浪费；
  - 有客户端连上：T257 会在连接重建后补报一次在场，已经覆盖；
  - 弹通知之前：被拒时系统本来就不显示，对设置页的提示帮助不大。
- **由宿主自己判断，不加协议方法**：`client/presence` 本来就在协议里；宿主只采信 desktop_ui 的，和在场的判定规则一致。
- **只在"从后台到前台"时查**（编排者补充）：T257 切会话时也会报 `foreground: true`，这种情况不查。
- **判定写进纯核心**：符合"有状态的决策逻辑写成纯核心"的约束，核心里不做 I/O，查询由 actor 执行。
- **和 T257 不互相等**：T258 只改宿主，T257 只改 ui-core，两边不碰同一个文件；用例直接给索引发在场，不依赖界面。T257 没合并之前，界面还不发在场，这条路径不会被触发，所以合并排在 T257（PR #110）之后。

## 验证方式

验收标准对照：
- [x] IT-notifier-auth-recheck 通过（`crates/cupper-host/src/index/tests.rs`）：
  - record 后端设成 denied，启动时上报 denied；
  - 改成 authorized 后，CLI 的在场、desktop_ui 报不在前台，都不触发查询，仍是 denied；
  - desktop_ui 回到前台，`host.updated` 变成 authorized；
  - 再把后端设回 denied，已在前台只换了会话，不查询，仍是 authorized。
  
  宿主其余用例照常通过：本机 `cargo test -p cupper-host --all-features` 全套通过，CI 的 `rust` 作业也通过。
- [x] 宿主的约束都守住：
  - 没有新增读时间或计时的地方；
  - 本任务没碰 cupper-sys，`unsafe` 仍只在那里；
  - 协议和界面都没改；
  - 四种特性组合下 clippy 都无告警。
- [x] PR 上 GitHub CI 的 8 个检查都通过：
  - PR #111：https://github.com/boris-w/cupper/pull/111
  - ci 运行：https://github.com/boris-w/cupper/actions/runs/43379668633 ，8 个检查全过

本机还跑了 fmt、`trace-ids check`，都通过。规模：`cargo xtask size` +118 −20，在停止线 200 以内。

## 提交列表

```
6d669739 feat(host): read the notification authorization again on coming to the front
```

## 遗留问题 / 范围外发现

1. **合并顺序**：排在 T257（PR #110）之后。T257 合并后如果和 main 冲突，按编排者的通知再合 main。
2. **端到端要等 T257 之后才看得到**：真实使用时，用户在系统设置里打开通知、切回 Cupper，设置页的提示应当消失。本任务没有人工复核，这一点可以在 T257 合并后的人工复核里顺带看一眼。
3. 本任务没在本机碰通知服务和授权，全部用 record 后端。
4. 补记 T254（已合并，汇报已存档）：编排者在 T254 收尾时要求的最终汇报，我没能用这次交回送出（这次交回当时已经用掉了），改用 SendMessage 发给了 `main`。要点：两轮修正（抢前台、导航丢失；重启宿主后导航丢失，修复后第 7 步重做通过）；合并 main；E2E ×10 和 nightly 都通过；MAN-notification-product 第 1–9、11 步通过，第 10 步移出人工检查范围、由 runloop.rs 的用例覆盖，记录在 `docs/tasks/T254-notifications-navigate/man-notification-product.md`，证据已入库；规模 +320。单列的发现：点授权横幅本身等于拒绝、宿主只在启动时查询授权（即本任务）、`client/presence` 不上报（T257）、出错卡片的 stderr 太长（T148）、自用构建短暂的"无宿主"（T215，去向 Q35）。
