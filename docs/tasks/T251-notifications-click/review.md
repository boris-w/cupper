# T251 评审

- 评审人：编排者
- 日期：2026-11-10
- 结论：**通过，合并**（PR #103）

## 核对

- **规模**：+710 −67。开工前估约 715，整体做时停止线 750；宿主这一侧做完后推算约 935，越过硬线，桌面部分拆到 T254，本任务停在 750 以内。
- **CI**：
  - PR #103 https://github.com/boris-w/cupper/pull/103 ，fef0d1c0 上 8 个检查全过（run 42764723874）；
  - cupper-sys.yml 的 security-peer（macos-26）通过（run 42764723664）；
  - 分支上的 nightly `selfuse`（42764723192）、`release-config`（42764731499）通过。

## 逐项意见

1. **拆分守住了"合并时有人用"**：宿主这一侧单独合并后，点通知能激活外层 App，UI 不在时能启动它；`ui/navigate` 发给 control 连接，T254 接上导航。
2. **点击路径和设计一致**：`userInfo` 带 ID（系统单独拉起的宿主不开数据库）；激活排在系统把宿主放到前台之后（`notifier_settle` 300 ms）；外层 App 按路径找并核对 bundle id；UI 不在时只交一个 URL，不走通用路由。
3. **系统单独拉起的宿主**排在读载荷之前，不加锁、不绑套接字、不开数据库，只在非测试构建里存在。
4. **测试入口走真实路径**：`test/notification_click` 只在 test-harness 构建里存在，属于 5.8.3 的 `test/*` 一档，按已记录的通知构造点击，送进同一个 channel。设计 08 的通配行已覆盖，不另加行。
5. **过程**：执行者在推送前自己重排过本地提交（rebase），违反守则第 1 条；提交没有推送过，没有造成影响，已提醒以后先报告。

## 记下的点

- 设计 5.2.9 补上 `notifier_settle`、`notifier_forward` 两个时长和外层 App 的核对方式（合并时由编排者补）。
- 写进交接事项：
  - 给 T180：外层 App 的签名要求；
  - 给 T254：转发 `ui/navigate`、`deeplink` 的路由和 ID 校验、E2E 用 `test/notification_click`、MAN-notification-product 核对激活时机和系统单独拉起的宿主。
