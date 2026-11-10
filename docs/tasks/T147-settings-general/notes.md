# T147 笔记

## 开工前的规模估算（2026-11-10）

按 `cargo xtask size` 的口径（新增行数，含测试，不含生成物）。基点是 T248 的 `031efc1f`：T248 还没合并，`cargo xtask size` 对 main 算会把 T248 的 365 行也算进来，所以每个模块之后用 `--rev <提交> --main 031efc1f` 量。

| 模块 | 代码 | 测试 | 合计 |
|---|---|---|---|
| 宿主防休眠：协议键 `host.prevent_idle_sleep`（旧的 `settings.updated` 事件里没有这个键，要按缺省值读）、样例和夹具、默认值、`SettingsPort` 变化时调 `Power::enabled`、启动时读初值、`PowerCore` 的 `Enabled` | 约 50 | 约 25 | 约 75 |
| 特权 `settings`：`policy/describe` → 有 `security_setting` 就弹原生框 → `settings/update`；宿主回"不需要确认"就直接 `settings/update`；确认框的构造和文案 | 约 45 | 约 45 | 约 90 |
| 通用：发送快捷键、跟进消息默认排队（含 Claude 插话副作用的说明）、防休眠；读写设置的 hook | 约 85 | 计入组件测试 | 约 85 |
| 通知：三个开关、授权被拒的提示 | 约 35 | 计入组件测试 | 约 35 |
| 存储：`host/storage` 各项大小和"未统计完"；`purge_snapshots` 的特权操作（主进程、确认框、ShellAPI 附加）；`turn_running` 的提示 | 约 95 | 约 25 | 约 120 |
| 命令行工具：`install_cli`（"应用程序"文件夹检查、按构建取链接名、建 `~/.local/bin`、只建符号链接不覆盖）、确认框、界面一行、ShellAPI 附加 | 约 95 | 约 50 | 约 145 |
| 组件测试和截图用例（UI-settings-general 三页两主题） | — | 约 115 | 约 115 |
| **合计** | | | **约 665** |

超过硬停止线 600。拿掉命令行工具后约 510，仍略超停止线 500。

## 开工前发现的问题

1. **"默认权限"没有可接的功能。** 宿主的设置里没有"新会话的权限模式"这个键（设计 5.2.11 的设置表里也没有），新建会话时界面也不传 `config.permission_mode`（新会话一律跟随 CLI 自己的配置）；各 agent 的权限模式清单只在会话建好后从 capabilities 里来，设置页拿不到。做这一行要先定设计：键名和取值（按 agent 分开？）、算不算安全设置、宿主在 `session/create` 没带模式时怎么套用、默认值是完全访问时 `session/create` 还要不要再确认。上面的估算没有算这一行。
2. **`policy/describe` 对普通设置回的是 `invalid_params`**（`DescribeError::NotConfirmable`，没有 `reason`）。主进程的 `settings` 只能把 `invalid_params` 当作"不需要确认"，直接发 `settings/update`。这样是安全的：补丁里有安全键时宿主一定给出描述，补丁本身不合法时 `settings/update` 也会以同样的原因失败。
3. **协议样例和宿主不一致**（范围外）：`admin.rs` 里 `policy/describe` 的样例写的 `details` 是 `{key, value}`，宿主实际给的是 `{settings: {键: 值}}`。主进程按宿主的实际回答写。
4. T242 已经合并到 main（`cac877a4`），本分支的起点比它早，`composer.followUpQueue` 那一行要等 merge main 之后再接。

## 编排者的裁定（2026-11-10，main `ae6a0dd6`）

- "默认权限"移出，成为待定问题 Q44；特权操作 `settings` 也移出（剩下的各行都是 `write` 级设置或界面偏好，直接 `settings/update`），上面第 2、3 条随之不再适用（第 3 条已记进交接事项）。
- 安装命令行工具留在本任务。估算约 575；停止线 600，硬停止线 680。
- T248 合并前不 merge main；跟进消息排队那一行等 merge 之后再做。

## 宿主的防休眠

- 新键 `host.prevent_idle_sleep` 在 `Settings` 里带 serde 缺省值 true：T151 起索引流里存着 `settings.updated`，检查点里也有索引视图的 `settings`，旧数据没有这个键，严格解码会失败。用缺省值，不用 upcaster 加版本：升版本要连带提高 `VIEW_VERSION`，所有检查点作废一次。生成的 schema 里这个键因此是可选的（zod 带 `.default(true)`），界面按"没有就是开"读。
- `SettingsPort` 每次更新成功后把当前值交给 `Power::enabled`，核心收到与现状相同的值时什么也不做，所以不在端口里另判"变没变"。启动时在 `SettingsHandle::load` 之后发一次初值，这时监护 actor 还没启动，邮件排在最前。
- IT-power-assertion 加了一段：关掉后重启，回合不持有；运行中打开立即持有、关掉立即释放。把启动时那一行去掉，这个用例会失败（已验证）。

## 规模（宿主模块之后）

宿主模块实际 +146，估的是 75。超出的部分都是用例和 rustfmt 的排版：IT-power-assertion 加的一段约 40 行（含抽出的 `interrupt`），旧设置按缺省值读的单元测试、样例和夹具约 25 行。产品代码约 50 行。预计总数约 646，超过停止线 600，报告后编排者选了继续（保留 IT 的那一段：去掉启动时读初值那一行，它会失败），停止线改为 660，硬停止线 720。

## 通知

- `host.updated.notification_auth` 的提示没有做：宿主还没有这个字段（T250），生成的 zod 会把不认识的字段剥掉，所以界面就算写了判断，在 T250 合并前永远读不到，用例也无法验证。等 T250 合并后再加（约 10 行），或者交给 T250，由编排者定。
