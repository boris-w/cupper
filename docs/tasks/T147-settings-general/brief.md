# T147 设置页 II：通用、通知、存储与命令行工具

- 派发日期：2026-11-10
- 执行者：子 agent（前端与桌面主进程工程师）
- 分支：`feat/T147-settings-general`，从 T248 的分支（`031efc1f`）开出：T248 和本任务都往 T151 那张设置分页表里加页。T248 合并后由你 merge main。
- 依赖：T127、T151（都已合并）、T248（等合并）。T252 依赖本任务

## 范围调整（计划 v1.3.92，开工前）

计划原来让本任务也做"外观"（主题覆盖、半透明侧边栏、语言覆盖）。这三项背后的功能都还不存在：
- 主题的覆盖和半透明侧边栏由 T152 实现；
- 英文由 T154 补齐（i18n 现在只有 zh-CN）；
- 图标颜色由 T252 实现。

现在做开关，就是没有功能可接的空控件。所以外观的每一行，改由实现那项功能的任务自己加；外观分页由第一个这样的任务建。本任务不做外观。

## 范围

**做**（计划 T147 一行；设计 5.7.4 设置页、5.7.2 的 `privileged`、5.7.8；P-6）：

1. **（开工前移出，2026-11-10）** "默认权限"和桌面特权操作 `settings` 都不做：
   - 宿主没有新会话默认权限的设置（待定问题 Q44），这一行要先设计；
   - 剩下的各行都是 `write` 级设置或界面偏好，直接 `settings/update` 或写 prefs，特权操作 `settings` 在本任务里没有使用方，随第一个安全设置行一起做。
2. **通用**：
   - **发送快捷键**：prefs 的 `composer.sendKey`，T086、T244 已接好。
   - **跟进消息默认排队**：prefs 的 `composer.followUpQueue`（T242）。说明里写 Claude 插话的副作用：当前的工具调用会被丢弃，正在跑的命令会被移到后台（设计 5.7.4）。
   - **防休眠**：设置键 `host.prevent_idle_sleep`，默认 true。宿主侧约 15 行加测试，按交接事项里 T156 那一行：
     - 启动时读初值；
     - `SettingsPort` 在它变化时调 `Power::enabled`；
     - `PowerCore` 加输入 `Enabled(bool)`。
3. **通知**：
   - 已有的三个开关：`notifications.needs_action`、`.turn_completed`、`.error`。
   - `host.updated.notification_auth` 为 `denied` 时，提示去系统设置里打开。这个字段由 T250 附加；T250 还没合并时，界面按"没有这个字段"处理，不显示提示。
4. **存储**（交接事项里 T127 那两行）：
   - `host/storage` 的各类大小，`partial` 为 true 时注明"未统计完"。
   - "清除快照对象"走 `PrivilegedAction` 的 `purge_snapshots`。
   - 宿主回 `conflict/turn_running` 时，提示先停下回合。
5. **"安装命令行工具"**：
   - `PrivilegedAction` 附加 `install_cli`，默认装到 `~/.local/bin`。
   - 先检查 App 在"应用程序"文件夹里，不在就说明原因、不装。
   - 退化模式下 CLI 照常按 uid 只读接入，入口不隐藏（计划 T147 一行）。
   - 主进程建符号链接，不经 shell；已经存在同名文件时，不覆盖用户自己的文件。
6. **用例**：
   - **UI-settings-general**（已登记）：通用、通知、存储各页，两种主题，基线只由 `ui-baseline` 生成。
   - **组件测试**：
     - 各开关读写；
     - 安全设置走特权路径；
     - 授权被拒的提示；
     - `partial` 和 `turn_running` 的提示。
   - **主进程的单元测试**：`settings` 和 `install_cli` 两个特权操作，`install_cli` 的位置检查和不覆盖。

**不做**：
- 外观分页和里面的各行（见上文"范围调整"）；
- 快捷键自定义页、Git 和 Worktrees 分区（P1）；
- `lfs_object_missing` 的界面提示（交接事项里 T127 的另一行，留给显示 git 错误的任务）。

## 验收标准

- [ ] UI-settings-general 通过；组件测试和主进程单元测试覆盖上面列的各项。
- [ ] 渲染进程和主进程的安全约束都守住：
  - 安全设置只经原生确认框；
  - 特权操作只在主进程执行；
  - 文案走 i18n；
  - 宿主给的路径和大小只作为文本显示。
- [ ] ShellAPI 和协议只做附加，`frozen.ts`、schema-compat 的检查通过。
- [ ] 宿主的约束都守住（防休眠那一处）。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 计划估为 M。开工前先按模块估一遍：特权 `settings`、通用、通知、存储、命令行工具、宿主的防休眠、用例。
- 估算超过停止线就先报告，不要动手。可能的拆法：命令行工具单独拿出去。
- 开工前估约 665，移出"默认权限"和特权操作 `settings` 后约 575（安装命令行工具留在本任务）。
- 停止线：600；硬停止线：680。
- **每完成一个模块就跑一次 `cargo xtask size`**（守则第 8 条）。
- 不要靠压缩代码或注释去守线。

## 参考资料

- 设计：5.7.4（设置页的布局和分区）、5.7.2（`privileged`、`prefs`）、5.7.8（特权操作、真实输入）、5.3（`policy/describe`、`settings/update`、`host/storage`、`host/purge_snapshots`）、5.2.13（防休眠）、08 的特权操作矩阵。
- 卷宗：T151（设置页框架、分页表）、T248（已归档会话、`CommandButton`）、T127（存储与清除快照）、T156（防休眠）、T160（自用构建、安装说明）、T231（`requestPrivileged` 的用法）。
- 交接事项：`docs/tasks/handoff-notes.md` 里接收任务为 T147 的各行。

## 注意

- 本机不跑会抢焦点、发真实按键的用例（守则第 10 条）。
- 不在本机真的往 `~/.local/bin` 装东西：用例用临时目录。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T037`，编排者已切到新分支 `feat/T147-settings-general`，起点是 T248 的 `031efc1f`。任务书在 main 上，用 `git show main:docs/tasks/T147-settings-general/brief.md` 读，不要提交到你的分支。T248 合并后，`main` 会通知你 merge main。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
